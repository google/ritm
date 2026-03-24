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
use aarch64_paging::idmap::IdMap;
use aarch64_paging::paging::{MemoryRegion, Stage2};
use aarch64_rt::InitialPagetable;
use alloc::string::ToString;
use alloc::vec::Vec;
use arm_pl011_uart::{Interrupts, PL011Registers, Uart, UniqueMmioPointer};
use core::alloc::Layout;
use core::ptr::NonNull;
use dtoolkit::{
    Node, Property,
    fdt::Fdt,
    model::{DeviceTree, DeviceTreeNode, DeviceTreeProperty},
};
use log::warn;

/// Base address of the first PL011 UART.
const UART_BASE_ADDRESS: *mut PL011Registers = 0x900_0000 as _;

const RITM_RESERVED_START: u64 = 0x4000_0000;
const RITM_RESERVED_END: u64 = 0x4040_0000;

/// The range of memory-mapped device registers.
const DEVICE_MEMORY_RANGE: MemoryRegion = MemoryRegion::new(0, 0x4000_0000);
/// The range of High `PCIe` ECAM.
const HIGH_PCIE_ECAM_RANGE: MemoryRegion = MemoryRegion::new(0x40_1000_0000, 0x40_2000_0000);
/// The range of High MMIO.
const HIGH_MMIO_RANGE: MemoryRegion = MemoryRegion::new(0x80_0000_0000, 0x100_0000_0000);

pub struct Qemu {
    parts: Option<PlatformParts<Uart<'static>>>,
}

impl Qemu {
    /// Returns the initial hard-coded page table to use before the Rust code starts.
    #[allow(clippy::cast_possible_truncation)]
    pub const fn initial_idmap() -> InitialPagetable {
        let mut idmap = [0; 512];
        // 1 GiB of device memory.
        idmap[0] = DEVICE_ATTRIBUTES.bits();
        // 1 GiB of normal memory.
        idmap[1] = MEMORY_ATTRIBUTES.bits() | RITM_RESERVED_START as usize;
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

    /// Returns the number of address and size cells for the given FDT.
    fn get_cells_count(fdt: &Fdt) -> (usize, usize) {
        let root = fdt.root();
        let address_cells = root.property("#address-cells").map_or(2, |p| {
            let bytes: [u8; 4] = p.value().try_into().unwrap_or([0; 4]);
            u32::from_be_bytes(bytes) as usize
        });
        let size_cells = root.property("#size-cells").map_or(1, |p| {
            let bytes: [u8; 4] = p.value().try_into().unwrap_or([0; 4]);
            u32::from_be_bytes(bytes) as usize
        });
        (address_cells, size_cells)
    }

    /// Excludes the RITM reserved range from the given range.
    fn exclude_ritm_range(addr: u64, size: u64) -> Vec<(u64, u64)> {
        let mut result = Vec::new();
        let end = addr + size;
        if addr < RITM_RESERVED_START {
            let chunk_end = end.min(RITM_RESERVED_START);
            if addr < chunk_end {
                result.push((addr, chunk_end - addr));
            }
        }
        if end > RITM_RESERVED_END {
            let chunk_start = addr.max(RITM_RESERVED_END);
            if chunk_start < end {
                result.push((chunk_start, end - chunk_start));
            }
        }
        result
    }

    #[allow(clippy::cast_possible_truncation)]
    fn map_stage2_memory(fdt: &Fdt, idmap: &mut IdMap<Stage2>) {
        if let Ok(memory) = fdt.memory()
            && let Ok(Some(regs)) = memory.reg()
        {
            for reg in regs {
                let addr = reg.address::<u64>().unwrap_or(0);
                let size = reg.size::<u64>().unwrap_or(0);

                for (map_addr, map_size) in Self::exclude_ritm_range(addr, size) {
                    idmap
                        .map_range(
                            &MemoryRegion::new(map_addr as usize, (map_addr + map_size) as usize),
                            STAGE2_MEMORY_ATTRIBUTES,
                        )
                        .expect("failed to map normal memory");
                }
            }
        }
    }

    #[allow(clippy::cast_possible_truncation)]
    fn append_reg(data: &mut Vec<u8>, addr: u64, size: u64, address_cells: usize, size_cells: usize) {
        if address_cells == 2 {
            data.extend_from_slice(&addr.to_be_bytes());
        } else {
            data.extend_from_slice(&(addr as u32).to_be_bytes());
        }
        if size_cells == 2 {
            data.extend_from_slice(&size.to_be_bytes());
        } else {
            data.extend_from_slice(&(size as u32).to_be_bytes());
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

    #[allow(clippy::cast_possible_truncation)]
    fn modify_dt(&self, fdt: Fdt<'static>) -> Fdt<'static> {
        let mut dt = DeviceTree::from_fdt(&fdt).expect("expected FDT to be valid");

        let memory = fdt.memory().expect("missing memory node");
        let name = memory.name().to_string();

        let mut res = Vec::<u8>::new();
        let mut first_addr = None;

        let (address_cells, size_cells) = Self::get_cells_count(&fdt);

        if let Ok(Some(regs)) = memory.reg() {
            for reg in regs {
                let addr = reg.address::<u64>().unwrap_or(0);
                let size = reg.size::<u64>().unwrap_or(0);

                for (chunk_addr, chunk_size) in Self::exclude_ritm_range(addr, size) {
                    if first_addr.is_none() {
                        first_addr = Some(chunk_addr);
                    }
                    Self::append_reg(&mut res, chunk_addr, chunk_size, address_cells, size_cells);
                }
            }
        }

        dt.root.remove_child(&name);
        let first_addr = first_addr.unwrap_or(RITM_RESERVED_END);
        let new_name = alloc::format!("memory@{first_addr:x}");
        dt.root.add_child(
            DeviceTreeNode::builder(new_name)
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

    #[allow(clippy::cast_possible_truncation)]
    fn make_stage2_pagetable(fdt: &Fdt) -> IdMap<Stage2> {
        let mut idmap = IdMap::new(0, Stage2);

        // Device memory
        idmap
            .map_range(&DEVICE_MEMORY_RANGE, STAGE2_DEVICE_ATTRIBUTES)
            .expect("failed to map device memory");

        // Normal memory
        Self::map_stage2_memory(fdt, &mut idmap);

        // High PCIe ECAM
        idmap
            .map_range(&HIGH_PCIE_ECAM_RANGE, STAGE2_DEVICE_ATTRIBUTES)
            .expect("failed to map High PCIe ECAM");

        // High MMIO
        idmap
            .map_range(&HIGH_MMIO_RANGE, STAGE2_DEVICE_ATTRIBUTES)
            .expect("failed to map High MMIO");

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
