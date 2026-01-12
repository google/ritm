// Copyright 2025 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Architecture-specific code.

use core::arch::{asm, naked_asm};

/// Data Synchronization Barrier.
pub fn dsb() {
    // SAFETY: Data Synchronization Barrier is always safe.
    unsafe {
        asm!("dsb sy", options(nostack, preserves_flags));
    }
}

/// Data Memory Barrier.
pub fn dmb() {
    // SAFETY: Data Memory Barrier is always safe.
    unsafe {
        asm!("dmb sy", options(nostack, preserves_flags));
    }
}

/// Instruction Synchronization Barrier.
pub fn isb() {
    // SAFETY: Instruction Synchronization Barrier is always safe.
    unsafe {
        asm!("isb", options(nostack, preserves_flags));
    }
}

/// Invalidate all instruction caches.
pub fn ic_iallu() {
    // SAFETY: `ic iallu` is always safe.
    unsafe {
        asm!("ic iallu", options(nostack, preserves_flags));
    }
}

/// Invalidate all TLB entries for EL2.
pub fn tlbi_alle2is() {
    // SAFETY: `tlbi alle2is` is always safe.
    unsafe {
        asm!("tlbi alle2is", options(nostack, preserves_flags));
    }
}

macro_rules! sys_reg {
    ($name:ident, {$($const_name:ident: $const_val:expr),*}) => {
        pub mod $name {
            use core::arch::asm;
            $(pub const $const_name: u64 = $const_val;)*

            #[doc = concat!("Read the `", stringify!($name), "` system register.")]
            #[allow(unused)]
            pub fn read() -> u64 {
                let val: u64;
                // SAFETY: The caller must ensure that the register is safely readable.
                unsafe {
                    asm!(concat!("mrs {}, ", stringify!($name)), out(reg) val, options(nostack, preserves_flags));
                }
                val
            }

            #[doc = concat!("Write the `", stringify!($name), "` system register.")]
            ///
            /// # Safety
            ///
            /// This function allows fundamental changes to the CPU state. To avoid Undefined
            /// Behavior, the caller must guarantee:
            ///
            /// * The register is writable at the current Exception Level.
            /// * The write must not invalidate the stack, the heap, or any active Rust references
            ///     (e.g., by disabling the MMU).
            /// * This function emits a raw `MSR`. The caller is responsible for issuing context
            ///     synchronization (e.g., `ISB`) or memory barriers (`DSB`) if required.
            #[allow(unused)]
            pub unsafe fn write(val: u64) {
                // SAFETY: The caller must ensure that the register is safely writeable.
                unsafe {
                    asm!(concat!("msr ", stringify!($name), ", {}"), in(reg) val, options(nostack, preserves_flags));
                }
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
sys_reg!(hcr_el2);
sys_reg!(cntvoff_el2);
sys_reg!(cnthctl_el2);
sys_reg!(spsr_el2);
sys_reg!(elr_el2);
sys_reg!(sp_el1);

/// Disables MMU and caches.
///
/// # Safety
///
/// The compiler is free to emit atomic memory accesses in safe Rust code, but these have
/// undefined behavior when the data cache is disabled. It's not safe to run Rust code with
/// the MMU disabled. This function is not therefore only intended to be called by
/// assembly code.
///
/// # Registers
///
/// This function clobbers x27 and x28.
#[unsafe(naked)]
pub(super) unsafe extern "C" fn disable_mmu_and_caches() {
    naked_asm!(
        "mov x27, x29",
        "mov x28, x30",
        "bl {disable_mmu_and_caches_impl}",
        // We assume we have an identity mapped pagetables for the currently running
        // code, so disabling MMU is safe.
        "msr sctlr_el2, x0",
        "mov x29, x27",
        "mov x30, x28",
        "dsb sy",
        "isb",

        // Invalidate I-cache
        "ic iallu",
        "tlbi alle2is",

        // Final synchronization
        "dsb sy",
        "isb",
        "ret",
        disable_mmu_and_caches_impl = sym disable_mmu_and_caches_impl,
    );
}

/// Invalidates dcache and returns new value for the `sctlr` register to be set.
extern "C" fn disable_mmu_and_caches_impl() -> u64 {
    invalidate_dcache();

    // Disable MMU and caches
    let mut sctlr = sctlr_el2::read();
    sctlr &= !sctlr_el2::M; // MMU Enable
    sctlr &= !sctlr_el2::C; // Data Cache Enable
    sctlr &= !sctlr_el2::I; // Instruction Cache Enable
    sctlr
}

/// Invalidate D-cache by set/way to the point of coherency.
pub fn invalidate_dcache() {
    dmb();

    // Cache Level ID Register
    let clidr = clidr_el1::read();

    // Level of Coherence (LoC) - Bits [26:24]
    let loc = (clidr >> 24) & 0x7;

    for level in 0..loc {
        let cache_type = (clidr >> (level * 3)) & 0x7;

        // Cache Types: 0=None, 1=Instruction, 2=Data, 3=Split, 4=Unified
        // We don't care about No cache or Instruction cache
        if cache_type < 2 {
            continue;
        }

        // Select the Cache Level in CSSELR (Cache Size Selection Register)
        // SAFETY: Writing to `csselr_el1` is always safe, assuming the cache level exists.
        unsafe {
            csselr_el1::write(level << 1);
        }

        // Barrier to ensure CSSELR write finishes before reading CCSIDR
        isb();

        // Cache Size ID Register (CCSIDR)
        let ccsidr = ccsidr_el1::read();

        let line_power = (ccsidr & 0x7) + 4;
        let ways = (ccsidr >> 3) & 0x3FF;
        let sets = (ccsidr >> 13) & 0x7FFF;

        let way_shift = (ways as u32).leading_zeros();

        for set in 0..=sets {
            for way in 0..=ways {
                let dc_val = (way << way_shift) | (set << line_power) | (level << 1);

                // SAFETY: `dc cisw` is always safe, assuming the cache line exists.
                unsafe {
                    asm!("dc cisw, {0}", in(reg) dc_val);
                }
            }
        }
    }

    dsb();
    isb();
}
