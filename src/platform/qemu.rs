// Copyright 2025 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/// The QEMU aarch64 virt platform.
use super::{FDT_ALIGNMENT, Platform, PlatformParts};
use crate::hvc_response::HvcResult;
use crate::pagetable::{STAGE2_DEVICE_ATTRIBUTES, STAGE2_MEMORY_ATTRIBUTES};
use crate::{
    pagetable::{DEVICE_ATTRIBUTES, MEMORY_ATTRIBUTES},
    platform::BootMode,
};
use aarch64_paging::paging::PAGE_SIZE;
use aarch64_rt::InitialPagetable;
use alloc::vec::Vec;
use arm_pl011_uart::{Interrupts, PL011Registers, Uart, UniqueMmioPointer};
use core::alloc::Layout;
use core::ptr::NonNull;
use core::sync::atomic::{AtomicU64, Ordering};
use dtoolkit::{
    Node, Property,
    fdt::Fdt,
    model::{DeviceTree, DeviceTreeNode, DeviceTreeProperty},
};
use log::warn;

use crate::platform::{PAYLOAD_ADDRESS, RITM_IMAGE_ADDRESS};
use crate::stage2::{
    MemoryAccessHandler, MemoryAccessWidth, MemoryReadAccess, MemoryReadResult, MemoryWriteAccess,
    MemoryWriteResult, Stage2Builder,
};

pub type PlatformImpl = Qemu;

/// Base address of the first PL011 UART.
const UART_BASE_ADDRESS: *mut PL011Registers = 0x900_0000 as _;

const DRAM_START: usize = 0x4000_0000;
const DEFAULT_FDT_ADDRESS: u64 = DRAM_START as u64;
const FILTERED_MMIO_BASE: usize = 0x0f00_0000;
const FILTERED_MMIO_PAGE_COUNT: usize = 1;
const RITM_START: usize = RITM_IMAGE_ADDRESS;
const RITM_END: usize = RITM_START + 4 * 1024 * 1024;

static FILTERED_MMIO_COUNTER: AtomicU64 = AtomicU64::new(0);

fn to_pages(size: usize) -> usize {
    size.div_ceil(PAGE_SIZE)
}

fn align_down_to_page(address: usize) -> usize {
    address / PAGE_SIZE * PAGE_SIZE
}

fn align_up_to_page(address: usize) -> usize {
    address.next_multiple_of(PAGE_SIZE)
}

fn to_ipa(address: usize) -> u64 {
    u64::try_from(address).expect("IPA should fit in u64")
}

pub struct Qemu {
    parts: Option<PlatformParts<Uart<'static>>>,
}

impl Qemu {
    /// Returns the initial hard-coded page table to use before the Rust code starts.
    pub const fn initial_idmap() -> InitialPagetable {
        let mut idmap = [0; 512];
        // 1 GiB of device memory.
        idmap[0] = DEVICE_ATTRIBUTES.bits();
        // 1 GiB of normal memory.
        idmap[1] = MEMORY_ATTRIBUTES.bits() | 0x4000_0000;
        // 1 GiB of DRAM.
        idmap[2] = DEVICE_ATTRIBUTES.bits() | 0x8000_0000;
        InitialPagetable(idmap)
    }

    fn read_boot_mode_from_cmd(fdt: &Fdt) -> Option<BootMode> {
        let args = fdt
            .root()
            .child("chosen")?
            .property("bootargs")?
            .as_str()
            .ok()?;
        for arg in args.split_whitespace() {
            if let Some(boot_mode) = arg.strip_prefix("ritm.boot_mode=") {
                match boot_mode {
                    "el2" => return Some(BootMode::El2),
                    "el1" => return Some(BootMode::El1),
                    _ => warn!("Unknown boot mode specified: {arg}"),
                }
            }
        }

        None
    }

    fn dram_range(fdt: &Fdt) -> (usize, usize) {
        let memory_node_name = alloc::format!("memory@{DRAM_START:x}");
        let memory = fdt
            .root()
            .child(&memory_node_name)
            .expect("memory node not found");
        let mut regs = memory
            .reg()
            .expect("memory reg property should be valid")
            .expect("memory node should have a reg property");
        let reg = regs
            .next()
            .expect("memory node should have at least one range");
        let start: u64 = reg.address().expect("memory address should fit in u64");
        let size: u64 = reg.size().expect("memory size should fit in u64");
        (
            start
                .try_into()
                .expect("memory address should fit in usize"),
            (start + size)
                .try_into()
                .expect("memory end should fit in usize"),
        )
    }

    fn read_filtered_mmio(access: MemoryReadAccess) -> MemoryReadResult {
        if access.width != MemoryAccessWidth::U64 {
            return MemoryReadResult::Fault;
        }

        match access.offset {
            0 => MemoryReadResult::Value(0xfeed_face_cafe_beef),
            16 => MemoryReadResult::Value(FILTERED_MMIO_COUNTER.fetch_add(1, Ordering::SeqCst)),
            24 => MemoryReadResult::Value(0x1111_2222_3333_4444),
            32 => MemoryReadResult::Value(0x5555_6666_7777_8888),
            40 => MemoryReadResult::Value(0x9999_aaaa_bbbb_cccc),
            _ => MemoryReadResult::Fault,
        }
    }

    fn write_filtered_mmio(access: MemoryWriteAccess) -> MemoryWriteResult {
        if access.offset == 8
            && access.width == MemoryAccessWidth::U64
            && access.value == 0x1234_5678_9abc_def0
        {
            MemoryWriteResult::Handled
        } else {
            MemoryWriteResult::Fault
        }
    }
}

impl Platform for Qemu {
    type Console = Uart<'static>;

    const MAX_CORES: usize = 8;

    unsafe fn create() -> Self {
        let mut uart = Uart::new(
            // SAFETY: UART_BASE_ADDRESS is valid and mapped, and `create` is only called once so
            // there are no aliases
            // SAFETY: The address is a constant and thus not null.
            unsafe {
                UniqueMmioPointer::new(
                    NonNull::new(UART_BASE_ADDRESS).expect("UART_BASE_ADDRESS should not be null"),
                )
            },
        );
        uart.set_interrupt_masks(Interrupts::RXI);
        Self {
            parts: Some(PlatformParts { console: uart }),
        }
    }

    fn parts(&mut self) -> Option<PlatformParts<Uart<'static>>> {
        self.parts.take()
    }

    fn boot_mode(&self, fdt: &Fdt) -> BootMode {
        Self::read_boot_mode_from_cmd(fdt).unwrap_or(BootMode::El1)
    }

    fn payload_address(&self) -> u64 {
        PAYLOAD_ADDRESS
    }

    fn fdt_address(&self, boot_fdt_address: u64) -> u64 {
        if boot_fdt_address == 0 {
            // QEMU doesn't populate x0 when booting the RITM ELF with -kernel.
            warn!("No FDT address was passed in x0; falling back to QEMU virt default FDT address");
            DEFAULT_FDT_ADDRESS
        } else {
            boot_fdt_address
        }
    }

    fn modify_dt(&self, fdt: Fdt<'static>) -> Fdt<'static> {
        let (dram_start, dram_end) = Self::dram_range(&fdt);
        let mut dt = DeviceTree::from_fdt(&fdt);
        let memory_node_name = alloc::format!("memory@{DRAM_START:x}");

        // Modify the Device Tree to reserve the memory used by RITM, so that the operating system
        // will not try to use it.
        // See `RITM_IMAGE_ADDRESS` for the address reference.
        let mut lower_memory = Vec::<u8>::new();
        lower_memory.extend_from_slice(&(dram_start as u64).to_be_bytes());
        lower_memory.extend_from_slice(&((RITM_START - dram_start) as u64).to_be_bytes());

        let mut upper_memory = Vec::<u8>::new();
        upper_memory.extend_from_slice(&(RITM_END as u64).to_be_bytes());
        upper_memory.extend_from_slice(&((dram_end - RITM_END) as u64).to_be_bytes());

        dt.root
            .remove_child(&memory_node_name)
            .expect("memory node not found");
        dt.root.add_child(
            DeviceTreeNode::builder(alloc::format!("memory@{dram_start:x}"))
                .expect("valid node name")
                .property(
                    DeviceTreeProperty::new("reg", lower_memory)
                        .expect("fixed property name should be valid"),
                )
                .property(
                    DeviceTreeProperty::new("device_type", b"memory\0")
                        .expect("fixed property name should be valid"),
                )
                .build(),
        );
        dt.root.add_child(
            DeviceTreeNode::builder(alloc::format!("memory@{RITM_END:x}"))
                .expect("valid node name")
                .property(
                    DeviceTreeProperty::new("reg", upper_memory)
                        .expect("fixed property name should be valid"),
                )
                .property(
                    DeviceTreeProperty::new("device_type", b"memory\0")
                        .expect("fixed property name should be valid"),
                )
                .build(),
        );

        let new_dtb = dt.to_dtb();
        let shared_dtb = crate::shared_alloc(
            Layout::from_size_align(new_dtb.len(), FDT_ALIGNMENT).expect("invalid layout"),
        );
        shared_dtb.copy_from_slice(&new_dtb);

        let fdt_address = shared_dtb.as_ptr();
        // SAFETY: fdt_address is a valid pointer to a device tree in the shared heap.
        let fdt: Fdt<'_> =
            unsafe { Fdt::from_raw(fdt_address).expect("fdt_address is not a valid fdt") };

        fdt
    }

    fn configure_memory_access(builder: &mut Stage2Builder) {
        // Device memory before the filtered MMIO test page.
        builder.allow_pages(
            0,
            to_pages(FILTERED_MMIO_BASE),
            STAGE2_DEVICE_ATTRIBUTES,
        );

        builder.handle_pages(
            to_ipa(FILTERED_MMIO_BASE),
            FILTERED_MMIO_PAGE_COUNT,
            MemoryAccessHandler::read_write(
                Self::read_filtered_mmio,
                Self::write_filtered_mmio,
            ),
        );

        // Device memory after the filtered MMIO test page.
        builder.allow_pages(
            to_ipa(FILTERED_MMIO_BASE + PAGE_SIZE),
            to_pages(DRAM_START - (FILTERED_MMIO_BASE + PAGE_SIZE)),
            STAGE2_DEVICE_ATTRIBUTES,
        );

        // Normal memory before the RITM image.
        builder.allow_pages(
            DRAM_START as u64,
            to_pages(RITM_START - DRAM_START),
            STAGE2_MEMORY_ATTRIBUTES,
        );

        // Normal memory after the RITM image.
        builder.allow_pages(
            RITM_END as u64,
            to_pages(0x1_0000_0000 - RITM_END),
            STAGE2_MEMORY_ATTRIBUTES,
        );

        // High PCIe ECAM
        builder.allow_pages(
            0x40_1000_0000,
            to_pages(0x40_2000_0000 - 0x40_1000_0000),
            STAGE2_DEVICE_ATTRIBUTES,
        );

        // High MMIO
        builder.allow_pages(
            0x80_0000_0000,
            to_pages(0x100_0000_0000 - 0x80_0000_0000),
            STAGE2_DEVICE_ATTRIBUTES,
        );

        // Map the shared heap
        let shared_start = &raw const crate::SHARED_HEAP as usize;
        let shared_end = shared_start + Self::SHARED_HEAP_SIZE;
        let shared_start = align_down_to_page(shared_start);
        let shared_end = align_up_to_page(shared_end);
        builder.allow_pages(
            shared_start as u64,
            to_pages(shared_end - shared_start),
            STAGE2_MEMORY_ATTRIBUTES,
        );
    }

    fn handle_hvc(function_id: u64, _args: [u64; 17]) -> HvcResult {
        // Dummy HVC for testing
        if function_id == 0xFF00_0000 {
            return HvcResult::Handled(Ok(0x1234_5678_9ABC_DEF0.into()));
        }

        HvcResult::Unhandled
    }
}
