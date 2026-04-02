// Copyright 2026 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/// Special version of the QEMU platform intended to be run as the bl33 payload.
///
/// This is mainly useful for the integration test in tests/psci_test.
use super::{Platform, PlatformParts};
use crate::pagetable::{STAGE2_DEVICE_ATTRIBUTES, STAGE2_MEMORY_ATTRIBUTES};
use crate::{
    pagetable::{DEVICE_ATTRIBUTES, MEMORY_ATTRIBUTES},
    platform::BootMode,
};
use aarch64_paging::idmap::IdMap;
use aarch64_paging::paging::{MemoryRegion, Stage2};
use aarch64_rt::InitialPagetable;
use arm_pl011_uart::{Interrupts, PL011Registers, Uart, UniqueMmioPointer};
use core::ptr::NonNull;

/// Base address of the first PL011 UART.
const UART_BASE_ADDRESS: *mut PL011Registers = 0x900_0000 as _;

const RITM_END: usize = 0x60400000;

pub struct Qemu {
    parts: Option<PlatformParts<Uart<'static>>>,
}

impl Qemu {
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

    const MAX_CORES: usize = 4;

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

    fn boot_mode(&self, _fdt: &dtoolkit::fdt::Fdt) -> BootMode {
        BootMode::El1
    }

    fn make_stage2_pagetable() -> IdMap<Stage2> {
        let mut idmap = IdMap::new(0, Stage2);

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

        idmap
    }
}
