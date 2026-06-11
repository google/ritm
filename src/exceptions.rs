// Copyright 2025 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use core::{arch::global_asm, borrow::Borrow, ops::Deref};

/// The full EL2 trap frame used for guest exceptions.
///
/// Stage-2 MMIO emulation needs access to the faulting instruction's Rt register, which can be any
/// GPR x0-x30. The default aarch64-rt exception frame saves only volatile registers, so RITM provides
/// its own EL2 vector table and saves the full guest GPR state.
#[derive(Debug, Eq, PartialEq)]
#[repr(C)]
pub struct RegisterState {
    pub registers: [u64; 31],
    padding: u64,
    pub elr: usize,
    pub spsr: u64,
}

const _: () = assert!(size_of::<RegisterState>() == 8 * 34);

#[derive(Debug, Eq, PartialEq)]
#[repr(transparent)]
pub struct RegisterStateRef<'a>(&'a mut RegisterState);

impl RegisterStateRef<'_> {
    /// Returns a mutable reference to the register state.
    ///
    /// # Safety
    ///
    /// Any changes made to the saved register state must not cause undefined
    /// behaviour when returning from the exception.
    pub unsafe fn get_mut(&mut self) -> &mut RegisterState {
        self.0
    }
}

impl AsRef<RegisterState> for RegisterStateRef<'_> {
    fn as_ref(&self) -> &RegisterState {
        self.0
    }
}

impl Borrow<RegisterState> for RegisterStateRef<'_> {
    fn borrow(&self) -> &RegisterState {
        self.0
    }
}

impl Deref for RegisterStateRef<'_> {
    type Target = RegisterState;

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

extern "C" fn sync_lower(register_state: RegisterStateRef) {
    // sync_lower exception is most likely a HVC/SMC call, which we should
    // handle in the hypervisor.
    crate::hypervisor::handle_sync_lower(register_state);
}

extern "C" fn unexpected_exception(register_state: RegisterStateRef) {
    panic!("Unexpected EL2 exception: register_state={register_state:?}");
}

global_asm!(
    r#"
.macro save_all_to_stack el:req
	sub sp, sp, #(8 * 34)
	stp x0, x1, [sp, #8 * 0]
	stp x2, x3, [sp, #8 * 2]
	stp x4, x5, [sp, #8 * 4]
	stp x6, x7, [sp, #8 * 6]
	stp x8, x9, [sp, #8 * 8]
	stp x10, x11, [sp, #8 * 10]
	stp x12, x13, [sp, #8 * 12]
	stp x14, x15, [sp, #8 * 14]
	stp x16, x17, [sp, #8 * 16]
	stp x18, x19, [sp, #8 * 18]
	stp x20, x21, [sp, #8 * 20]
	stp x22, x23, [sp, #8 * 22]
	stp x24, x25, [sp, #8 * 24]
	stp x26, x27, [sp, #8 * 26]
	stp x28, x29, [sp, #8 * 28]
	str x30, [sp, #8 * 30]

	mrs x0, elr_\el
	mrs x1, spsr_\el
	stp x0, x1, [sp, #8 * 32]
.endm

.macro restore_all_from_stack el:req
	ldp x0, x1, [sp, #8 * 32]
	msr elr_\el, x0
	msr spsr_\el, x1

	ldp x0, x1, [sp, #8 * 0]
	ldp x2, x3, [sp, #8 * 2]
	ldp x4, x5, [sp, #8 * 4]
	ldp x6, x7, [sp, #8 * 6]
	ldp x8, x9, [sp, #8 * 8]
	ldp x10, x11, [sp, #8 * 10]
	ldp x12, x13, [sp, #8 * 12]
	ldp x14, x15, [sp, #8 * 14]
	ldp x16, x17, [sp, #8 * 16]
	ldp x18, x19, [sp, #8 * 18]
	ldp x20, x21, [sp, #8 * 20]
	ldp x22, x23, [sp, #8 * 22]
	ldp x24, x25, [sp, #8 * 24]
	ldp x26, x27, [sp, #8 * 26]
	ldp x28, x29, [sp, #8 * 28]
	ldr x30, [sp, #8 * 30]
	add sp, sp, #(8 * 34)
.endm

.macro full_exception handler:req el:req
	save_all_to_stack \el
	mov x0, sp
	bl \handler
	restore_all_from_stack \el
	eret
.endm

.section .text.vector_table_el2, "ax"
.global vector_table_el2
.balign 0x800
vector_table_el2:
sync_cur_sp0_el2:
	b unexpected_exception_el2

.balign 0x80
irq_cur_sp0_el2:
	b unexpected_exception_el2

.balign 0x80
fiq_cur_sp0_el2:
	b unexpected_exception_el2

.balign 0x80
serr_cur_sp0_el2:
	b unexpected_exception_el2

.balign 0x80
sync_cur_spx_el2:
	b unexpected_exception_el2

.balign 0x80
irq_cur_spx_el2:
	b unexpected_exception_el2

.balign 0x80
fiq_cur_spx_el2:
	b unexpected_exception_el2

.balign 0x80
serr_cur_spx_el2:
	b unexpected_exception_el2

.balign 0x80
sync_lower_64_el2:
	b sync_lower_exception_el2

.balign 0x80
irq_lower_64_el2:
	b unexpected_exception_el2

.balign 0x80
fiq_lower_64_el2:
	b unexpected_exception_el2

.balign 0x80
serr_lower_64_el2:
	b unexpected_exception_el2

.balign 0x80
sync_lower_32_el2:
	b sync_lower_exception_el2

.balign 0x80
irq_lower_32_el2:
	b unexpected_exception_el2

.balign 0x80
fiq_lower_32_el2:
	b unexpected_exception_el2

.balign 0x80
serr_lower_32_el2:
	b unexpected_exception_el2

sync_lower_exception_el2:
	full_exception {sync_lower} el2

unexpected_exception_el2:
	full_exception {unexpected_exception} el2
"#,
    sync_lower = sym sync_lower,
    unexpected_exception = sym unexpected_exception,
);
