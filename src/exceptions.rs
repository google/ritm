// Copyright 2025 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use aarch64_rt::{ExceptionHandlers, RegisterStateRef as VolatileRegisterStateRef};
use core::arch::naked_asm;

/// Non-volatile registers saved by RITM's synchronous lower-EL handler wrapper.
///
/// `aarch64-rt` saves x0-x18, x29, x30, ELR and SPSR before calling the Rust exception handler.
/// Stage-2 MMIO emulation may also need to read or update x19-x28, so the `sync_lower` handler
/// saves these registers in this separate frame before calling into Rust.
#[derive(Debug, Eq, PartialEq)]
#[repr(C)]
pub struct NonVolatileRegisters {
    registers: [u64; 10],
}

const _: () = assert!(size_of::<NonVolatileRegisters>() == 8 * 10);

#[derive(Debug, Eq, PartialEq)]
pub enum GuestRegisterWriteError {
    InvalidRegisterIndex,
}

/// Guest register view for synchronous lower-EL exceptions.
///
/// This combines the volatile registers saved by `aarch64-rt` with the x19-x28 frame saved by
/// RITM's `sync_lower` wrapper.
#[derive(Debug)]
pub struct GuestRegisterStateRef<'a> {
    volatile: VolatileRegisterStateRef<'a>,
    nonvolatile: &'a mut NonVolatileRegisters,
}

impl<'a> GuestRegisterStateRef<'a> {
    fn new(
        volatile: VolatileRegisterStateRef<'a>,
        nonvolatile: &'a mut NonVolatileRegisters,
    ) -> Self {
        Self {
            volatile,
            nonvolatile,
        }
    }

    /// Returns the volatile GPR frame saved by `aarch64-rt`.
    pub fn volatile_gprs(&self) -> &[u64; 19] {
        &self.volatile.registers
    }

    /// Returns the value of guest GPR `index`.
    pub fn read_gpr(&self, index: usize) -> Option<u64> {
        match index {
            0..=18 => Some(self.volatile.registers[index]),
            19..=28 => Some(self.nonvolatile.registers[index - 19]),
            29 => Some(self.volatile.fp),
            30 => Some(self.volatile.sp),
            31 => Some(0),
            _ => None,
        }
    }

    /// Updates guest GPR `index`.
    ///
    /// # Errors
    ///
    /// Returns [`GuestRegisterWriteError::InvalidRegisterIndex`] if `index` is not a guest GPR.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `value` is safe to write into guest GPR `index` for the trapped
    /// instruction or exception being handled.
    pub unsafe fn write_gpr(
        &mut self,
        index: usize,
        value: u64,
    ) -> Result<(), GuestRegisterWriteError> {
        match index {
            0..=18 => {
                // SAFETY: We only update the saved guest register targeted by the handler.
                unsafe {
                    self.volatile.get_mut().registers[index] = value;
                }
            }
            19..=28 => self.nonvolatile.registers[index - 19] = value,
            29 => {
                // SAFETY: We only update the saved guest frame pointer.
                unsafe {
                    self.volatile.get_mut().fp = value;
                }
            }
            30 => {
                // SAFETY: We only update the saved guest link register.
                unsafe {
                    self.volatile.get_mut().sp = value;
                }
            }
            31 => {}
            _ => return Err(GuestRegisterWriteError::InvalidRegisterIndex),
        }
        Ok(())
    }

    /// Returns the saved exception return address.
    pub fn exception_return_address(&self) -> usize {
        self.volatile.elr
    }

    /// Returns the saved exception return status.
    pub fn exception_return_status(&self) -> u64 {
        self.volatile.spsr
    }

    /// Advances the saved exception return address by `byte_count`.
    ///
    /// # Safety
    ///
    /// The caller must ensure that advancing the return address leaves it pointing to a valid guest
    /// exception return location.
    pub unsafe fn advance_pc(&mut self, byte_count: usize) {
        // SAFETY: The caller only advances the PC after emulating the trapped instruction.
        unsafe {
            self.volatile.get_mut().elr += byte_count;
        }
    }

    /// Sets the saved exception return address and status.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `elr` and `spsr` describe a valid guest exception return state.
    pub unsafe fn set_exception_return(&mut self, elr: usize, spsr: u64) {
        // SAFETY: The caller updates the saved return state to redirect guest exception handling.
        let regs = unsafe { self.volatile.get_mut() };
        regs.elr = elr;
        regs.spsr = spsr;
    }
}

pub struct Exceptions;

impl ExceptionHandlers for Exceptions {
    #[unsafe(naked)]
    extern "C" fn sync_lower(_register_state: VolatileRegisterStateRef) {
        naked_asm!(
            "sub sp, sp, #(8 * 12)",
            "stp x19, x20, [sp, #8 * 0]",
            "stp x21, x22, [sp, #8 * 2]",
            "stp x23, x24, [sp, #8 * 4]",
            "stp x25, x26, [sp, #8 * 6]",
            "stp x27, x28, [sp, #8 * 8]",
            "str x30, [sp, #8 * 10]",
            "mov x1, sp",
            "bl {handler}",
            "ldr x30, [sp, #8 * 10]",
            "ldp x19, x20, [sp, #8 * 0]",
            "ldp x21, x22, [sp, #8 * 2]",
            "ldp x23, x24, [sp, #8 * 4]",
            "ldp x25, x26, [sp, #8 * 6]",
            "ldp x27, x28, [sp, #8 * 8]",
            "add sp, sp, #(8 * 12)",
            "ret",
            handler = sym sync_lower_with_nonvolatile,
        );
    }
}

extern "C" fn sync_lower_with_nonvolatile(
    volatile: VolatileRegisterStateRef,
    nonvolatile: &mut NonVolatileRegisters,
) {
    crate::hypervisor::handle_sync_lower(GuestRegisterStateRef::new(volatile, nonvolatile));
}
