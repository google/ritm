// Copyright 2025 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/// The Pixel device platform.
use super::{Platform, PlatformParts};
use crate::pagetable::{DEVICE_ATTRIBUTES, MEMORY_ATTRIBUTES};
use aarch64_rt::InitialPagetable;
use core::ptr::NonNull;
use safe_mmio::UniqueMmioPointer;
use synopsys_dw_uart::SynopsysUart;
use synopsys_dw_uart::registers::Registers as SynopsysUartRegisters;

/// Base address of the first UART.
const UART_BASE_ADDRESS: *mut SynopsysUartRegisters = 0x900_0000 as _;
pub struct Pixel {
    parts: Option<PlatformParts<SynopsysUart<'static>>>,
}

impl Pixel {
    /// Returns the initial hard-coded page table to use before the Rust code starts.
    pub const fn initial_idmap() -> InitialPagetable {
        // TODO: is this pagetable correct for an actual device?
        let mut idmap = [0; 512];
        // 1 GiB of device memory.
        idmap[0] = DEVICE_ATTRIBUTES.bits();
        // 1 GiB of normal memory.
        idmap[1] = MEMORY_ATTRIBUTES.bits() | 0x40000000;
        // Another 1 GiB of device memory starting at 256 GiB.
        idmap[256] = DEVICE_ATTRIBUTES.bits() | 0x4000000000;
        InitialPagetable(idmap)
    }
}

impl Platform for Pixel {
    type Console = SynopsysUart<'static>;

    unsafe fn create() -> Self {
        let uart = SynopsysUart::new(
            // SAFETY: UART_BASE_ADDRESS is valid and mapped, and `create` is only called once so
            // there are no aliases
            unsafe { UniqueMmioPointer::new(NonNull::new(UART_BASE_ADDRESS).unwrap()) },
        );
        Self {
            parts: Some(PlatformParts { console: uart }),
        }
    }

    fn parts(&mut self) -> Option<PlatformParts<SynopsysUart<'static>>> {
        self.parts.take()
    }
}
