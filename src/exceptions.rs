// Copyright 2025 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use core::arch::{asm, naked_asm};

use log::info;

#[unsafe(no_mangle)]
extern "C" fn sync_exception_current(elr: u64, _spsr: u64) {
    panic!(
        "Unexpected sync_exception_current, esr={:#x}, far={:#x}, elr={:#x}",
        esr(),
        far(),
        elr
    );
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
        "add x4, x4, #4",
        "str x4, [sp, #8 * 22 + 16]",

        "ldp x29, x30, [sp], #0x10",
        "ret",
        sync_lower_impl = sym self::sync_lower_impl,
    )
}

enum ExceptionClass {
    SmcTrappedInAArch64,
    #[allow(unused)]
    Unknown(u8),
}

impl ExceptionClass {
    fn new(value: u8) -> Self {
        match value {
            0x17 => Self::SmcTrappedInAArch64,
            _ => Self::Unknown(value),
        }
    }
}

#[unsafe(no_mangle)]
extern "C" fn sync_lower_impl(elr: u64, _spsr: u64, x0: u64, x1: u64, x2: u64, x3: u64) -> u64 {
    let esr = esr();
    let ec = u8::try_from((esr >> 26) & 0x3f).expect("`& 0x3f` guarantees the value fits in u8");
    let ec = ExceptionClass::new(ec);

    match ec {
        ExceptionClass::SmcTrappedInAArch64 => {
            info!(
                "Forwarding the PSCI call: fn_id={x0:#x}, arg0={x1:#x}, arg1={x2:#x}, arg2={x3:#x}"
            );
            let out: u64;
            // SAFETY: assuming the PSCI call is correct
            unsafe {
                out = psci_forward(x0, x1, x2, x3);
            }
            info!(
                "Forwarded the PSCI call: fn_id={x0:#x}, arg0={x1:#x}, arg1={x2:#x}, arg2={x3:#x}; out={out:#x}"
            );

            out
        }
        _ => {
            panic!(
                "Unexpected sync_lower, esr={:#x}, far={:#x}, elr={:#x}, x0={:#x}, x1={:#x}, x2={:#x}, x3={:#x}",
                esr,
                far(),
                elr,
                x0,
                x1,
                x2,
                x3,
            );
        }
    }
}

#[unsafe(naked)]
unsafe extern "C" fn psci_forward(fn_id: u64, arg0: u64, arg1: u64, arg2: u64) -> u64 {
    naked_asm! {
        "smc #0",
        "ret",
    };
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

fn esr() -> u64 {
    let mut esr: u64;
    // SAFETY: This only reads a system register.
    unsafe {
        asm!("mrs {esr}, esr_el2", esr = out(reg) esr);
    }
    esr
}

fn far() -> u64 {
    let mut far: u64;
    // SAFETY: This only reads a system register.
    unsafe {
        asm!("mrs {far}, far_el2", far = out(reg) far);
    }
    far
}
