// Copyright 2025 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use core::arch::naked_asm;

#[unsafe(no_mangle)]
extern "C" fn sync_exception_current(_elr: u64, _spsr: u64) {
    panic!("Unexpected sync_exception_current");
}

#[unsafe(no_mangle)]
extern "C" fn irq_current(_elr: u64, _spsr: u64) {
    panic!("Unexpected irq_current");
}

#[unsafe(no_mangle)]
extern "C" fn fiq_current(_elr: u64, _spsr: u64) {
    panic!("Unexpected fiq_current");
}

#[unsafe(no_mangle)]
extern "C" fn serr_current(_elr: u64, _spsr: u64) {
    panic!("Unexpected serr_current");
}

#[unsafe(naked)]
#[unsafe(no_mangle)]
#[rustfmt::skip]
unsafe extern "C" fn sync_lower() {
    naked_asm!(
        // We load and store the registers from/to where aarch64 exception handler expects them:
        // https://github.com/google/aarch64-rt/blob/047f3b0962064d334f149fe5fcd46ba57ea758ab/src/exceptions.S#L16-L65
        "stp x29, x30, [sp, #-0x10]!",

        "ldp x2, x3, [sp, #0x10]",
        "ldp x4, x5, [sp, #0x20]",

        "bl {sync_lower_impl}",

        "mov x1, xzr",
        "mov x2, xzr",
        "mov x3, xzr",
        "stp x0, x1, [sp, #0x10]",
        "stp x2, x3, [sp, #0x20]",
        "ldr x4, [sp, #8 * 22 + 16]",
        // Return after the HVC call
        "add x4, x4, #4",
        "str x4, [sp, #8 * 22 + 16]",

        "ldp x29, x30, [sp], #0x10",
        "ret",
        sync_lower_impl = sym crate::hypervisor::sync_lower_impl,
    )
}

#[unsafe(no_mangle)]
extern "C" fn irq_lower(_elr: u64, _spsr: u64) {
    panic!("Unexpected irq_lower");
}

#[unsafe(no_mangle)]
extern "C" fn fiq_lower(_elr: u64, _spsr: u64) {
    panic!("Unexpected fiq_lower");
}

#[unsafe(no_mangle)]
extern "C" fn serr_lower(_elr: u64, _spsr: u64) {
    panic!("Unexpected serr_lower");
}
