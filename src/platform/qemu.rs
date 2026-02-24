// Copyright 2025 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/// The QEMU aarch64 virt platform.
use super::{FDT_ALIGNMENT, Platform, PlatformParts};
use crate::pagetable::{STAGE2_DEVICE_ATTRIBUTES, STAGE2_MEMORY_ATTRIBUTES};
use crate::{
    pagetable::{DEVICE_ATTRIBUTES, MEMORY_ATTRIBUTES},
    platform::BootMode,
};
use aarch64_paging::descriptor::Stage2Attributes;
use aarch64_paging::idmap::IdMap;
use aarch64_paging::paging::{MemoryRegion, TranslationRegime};
use aarch64_rt::InitialPagetable;
use alloc::vec::Vec;
use arm_pl011_uart::{Interrupts, PL011Registers, Uart, UniqueMmioPointer};
use core::alloc::Layout;
use core::ptr::NonNull;
use dtoolkit::{
    fdt::Fdt,
    model::{DeviceTree, DeviceTreeNode, DeviceTreeProperty},
};

/// Base address of the first PL011 UART.
const UART_BASE_ADDRESS: *mut PL011Registers = 0x900_0000 as _;

const RITM_END: usize = 0x4040_0000;

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

    fn boot_mode(&self) -> BootMode {
        // This is just hardcoded for QEMU, but a real platform implementation should most likely
        // check some external conditions (e.g. whether the bootloader is unlocked) to choose
        // beetween booting in EL1 or EL2.
        BootMode::El1
    }

    fn modify_dt(&self, fdt: Fdt<'static>) -> Fdt<'static> {
        let mut dt = DeviceTree::from_fdt(&fdt).expect("expected FDT to be valid");

        // Modify the Device Tree to reserve the memory used by RITM, so that the operating system
        // will not try to use it.
        // See `linker/qemu.ld` for the address reference.
        let mut res = Vec::<u8>::new();
        res.extend_from_slice(&0x4040_0000u64.to_be_bytes());
        res.extend_from_slice(&(124u64 * 1024 * 1024).to_be_bytes()); // 128 MiB default - 4 MiB reserved

        dt.root
            .remove_child("memory@40000000")
            .expect("memory node not found");
        dt.root.add_child(
            DeviceTreeNode::builder("memory@40400000")
                .property(DeviceTreeProperty::new("reg", res))
                .property(DeviceTreeProperty::new("device_type", b"memory\0"))
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

    fn make_stage2_pagetable() -> IdMap<Stage2Attributes> {
        let mut idmap = IdMap::<Stage2Attributes>::new(0, 0, TranslationRegime::Stage2);

        // Device memory
        idmap
            .map_range(&MemoryRegion::new(0, 0x4000_0000), STAGE2_DEVICE_ATTRIBUTES)
            .expect("failed to map device memory");

        // Normal memory
        idmap
            .map_range(
                &MemoryRegion::new(RITM_END, 0x1_0000_0000),
                STAGE2_MEMORY_ATTRIBUTES,
            )
            .expect("failed to map normal memory");

        // High PCIe ECAM
        idmap
            .map_range(
                &MemoryRegion::new(0x40_1000_0000, 0x40_2000_0000),
                STAGE2_DEVICE_ATTRIBUTES,
            )
            .expect("failed to map High PCIe ECAM");

        // High MMIO
        // We split this into two ranges to avoid mapping a full L0 entry (512 GiB) as a single
        // block, which is not supported by the architecture (L0 blocks are not supported with 4 KiB
        // pages without enabling FEAT_LPA2)
        idmap
            .map_range(
                &MemoryRegion::new(0x80_0000_0000, 0xC0_0000_0000),
                STAGE2_DEVICE_ATTRIBUTES,
            )
            .expect("failed to map High MMIO (1)");
        idmap
            .map_range(
                &MemoryRegion::new(0xC0_0000_0000, 0x100_0000_0000),
                STAGE2_DEVICE_ATTRIBUTES,
            )
            .expect("failed to map High MMIO (2)");

        // Map the shared heap
        let shared_start = &raw const crate::SHARED_HEAP as usize;
        let shared_end = shared_start + Self::SHARED_HEAP_SIZE;
        idmap
            .map_range(
                &MemoryRegion::new(shared_start, shared_end),
                STAGE2_MEMORY_ATTRIBUTES,
            )
            .expect("failed to map shared heap");

        idmap
    }
}
