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
            ///
            /// # Safety
            ///
            /// This function emits a raw `MRS` instruction. The caller must guarantee that:
            ///
            /// * The register is readable at the current Exception Level.
            /// * Reading the register does not destructively alter hardware state (e.g.,
            ///     acknowledging an interrupt by reading `ICC_IAR1_EL1`).
            #[allow(unused)]
            pub unsafe fn read() -> u64 {
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

pub(super) fn disable_mmu_and_caches() {
    invalidate_dcache();

    // Disable MMU and caches
    let mut sctlr: u64;
    // SAFETY: We are reading a non-destructive register at our current Exception Level.
    unsafe {
        sctlr = sctlr_el2::read();
    }
    sctlr &= !sctlr_el2::M; // MMU Enable
    sctlr &= !sctlr_el2::C; // Data Cache Enable
    sctlr &= !sctlr_el2::I; // Instruction Cache Enable
    // SAFETY: We assume we have an identity mapped pagetables for the currently running
    // code, so disabling MMU is safe.
    unsafe {
        sctlr_el2::write(sctlr);
    }
    dsb();
    isb();

    // Invalidate I-cache
    ic_iallu();
    tlbi_alle2is();

    // Final synchronization
    dsb();
    isb();
}

/// Invalidate D-cache by set/way to the point of coherency.
pub fn invalidate_dcache() {
    dmb();

    // Cache Level ID Register
    let clidr: u64;
    // SAFETY: We are reading a non-destructive register at a higher Exception Level.
    unsafe {
        clidr = clidr_el1::read();
    }

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
        let ccsidr: u64;
        // SAFETY: We are reading a non-destructive register at a higher Exception Level.
        unsafe {
            ccsidr = ccsidr_el1::read();
        }

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
