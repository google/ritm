// Copyright 2025 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Architecture-specific code.

use core::arch::asm;

/// Data Synchronization Barrier.
pub fn dsb() {
    // SAFETY: `dsb sy` is always a valid instruction.
    unsafe {
        asm!("dsb sy", options(nostack));
    }
}

/// Data Synchronization Barrier (inner shareable).
pub fn dsb_ish() {
    // SAFETY: `dsb ish` is always a valid instruction.
    unsafe {
        asm!("dsb ish", options(nostack));
    }
}

/// Instruction Synchronization Barrier.
pub fn isb() {
    // SAFETY: `isb` is always a valid instruction.
    unsafe {
        asm!("isb", options(nostack));
    }
}

/// Invalidate all instruction caches.
pub fn ic_iallu() {
    // SAFETY: `ic iallu` is always a valid instruction.
    unsafe {
        asm!("ic iallu", options(nostack));
    }
}

/// Invalidate all TLB entries for EL2.
pub fn tlbi_alle2is() {
    // SAFETY: `tlbi alle2is` is always a valid instruction.
    unsafe {
        asm!("tlbi alle2is", options(nostack));
    }
}

/// Get the current stack pointer.
pub fn sp() -> u64 {
    let val: u64;
    // SAFETY: Reading from the sp register is always safe.
    unsafe { asm!("mov {}, sp", out(reg) val, options(nostack)) };
    val
}

macro_rules! sys_reg {
    ($name:ident, {$($const_name:ident: $const_val:expr),*}) => {
        pub mod $name {
            use core::arch::asm;
            $(pub const $const_name: u64 = $const_val;)*

            #[allow(unused)]
            pub fn read() -> u64 {
                let val: u64;
                // SAFETY: Reading from a system register is safe, if the register exists.
                unsafe { asm!(concat!("mrs {}, ", stringify!($name)), out(reg) val, options(nostack)) };
                val
            }

            #[allow(unused)]
            pub fn write(val: u64) {
                // SAFETY: Writing to a system register is safe, if the register exists.
                unsafe { asm!(concat!("msr ", stringify!($name), ", {}"), in(reg) val, options(nostack)) };
            }
        }
    };
    ($name:ident) => {
        sys_reg!($name, {});
    };
}

sys_reg!(sctlr_el2, {
    M: 1 << 0,
    C: 1 << 2,
    I: 1 << 12
});
sys_reg!(clidr_el1);
sys_reg!(csselr_el1);
sys_reg!(ccsidr_el1);
sys_reg!(hcr_el2, {
    RW: 1 << 31,
    TID1: 1 << 19,
    AMO: 1 << 4
});
sys_reg!(cntvoff_el2);
sys_reg!(cnthctl_el2);
sys_reg!(spsr_el2, {
    MASK_ALL: 0b1111 << 6,
    EL1H: 5
});
sys_reg!(elr_el2);
sys_reg!(sp_el1);

/// Invalidate D-cache by set/way to the point of coherency.
pub fn invalidate_dcache() {
    // SAFETY: This is a standard ARM64 cache invalidation routine.
    unsafe {
        asm!(
            "mrs x5, clidr_el1",      // x5 = CLIDR
            "and x6, x5, #0x7000000", // x6 = LoC
            "lsr x6, x6, #23",
            "cbz x6, 2f",  // if LoC is 0, then no need to clean
            "mov x10, #0", // x10 = cache level * 2
            // loop over cache levels
            "1:",
            "add x12, x10, x10, lsr #1", // x12 = level * 3
            "lsr x11, x5, x12",
            "and x11, x11, #7",         // x11 = cache type
            "cmp x11, #2",              // is it a data or unified cache?
            "b.lt 3f",                  // if not, skip to next level
            "msr csselr_el1, x10",      // select cache level
            "isb",                      // sync change of csselr
            "mrs x11, ccsidr_el1",      // x11 = ccsidr
            "and x12, x11, #7",         // x12 = log2(line size) - 4
            "add x12, x12, #4",         // x12 = log2(line size)
            "and x13, x11, #0x3ff",     // x13 = (number of ways - 1)
            "and x14, x11, #0x7fff000", // x14 = (number of sets - 1)
            "lsr x14, x14, #13",
            "clz w15, w13", // w15 = 31 - log2(ways)
            // loop over ways
            "4:",
            "mov x9, x14", // x9 = set number
            // loop over sets
            "5:",
            "lsl x7, x13, x15",
            "lsl x8, x9, x12",
            "orr x11, x10, x7", // x11 = (level << 1) | (way << ...)
            "orr x11, x11, x8", // x11 |= (set << log2(line size))
            "dc isw, x11",      // invalidate by set/way
            "subs x9, x9, #1",
            "b.ge 5b",
            "subs x13, x13, #1",
            "b.ge 4b",
            "3:",
            "add x10, x10, #2", // next cache level
            "cmp x6, x10",
            "b.gt 1b",
            "2:",
            options(nostack)
        );
    }
}
