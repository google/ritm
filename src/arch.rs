// Copyright 2025 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Architecture-specific code.

use arm_sysregs::{
    CacheLevel, CacheType, CsselrEl1, SctlrEl2, read_ccsidr_el1, read_clidr_el1, read_sctlr_el2,
    write_csselr_el1,
};
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

/// Disables MMU and caches.
///
/// # Safety
///
/// The compiler is free to emit atomic memory accesses in safe Rust code, but these have
/// undefined behavior when the data cache is disabled. It's not safe to run Rust code with
/// the MMU disabled. This function is therefore *only* intended to be called by
/// assembly code.
///
/// The caller must ensure there are identity mapped pagetables for the code executed.
///
/// # Registers
///
/// In addition to the regular caller-saved registers, this function clobbers x27 and x28.
#[unsafe(naked)]
pub(super) unsafe extern "C" fn disable_mmu_and_caches() {
    naked_asm!(
        "mov x27, x29",
        "mov x28, x30",
        "bl {disable_mmu_and_caches_impl}",
        // We assume we have identity mapped pagetables for the currently running
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
    let mut sctlr = read_sctlr_el2();
    sctlr.remove(SctlrEl2::M); // MMU Enable
    sctlr.remove(SctlrEl2::C); // Data Cache Enable
    sctlr.remove(SctlrEl2::I); // Instruction Cache Enable
    sctlr.bits()
}

/// Invalidate D-cache by set/way to the point of coherency.
pub fn invalidate_dcache() {
    dmb();

    // Cache Level ID Register
    let clidr = read_clidr_el1();

    // Level of Coherence (LoC)
    let loc = clidr.loc();

    for level in 1..=loc {
        // We don't care about No cache or Instruction cache
        let level = CacheLevel::new(level);
        match clidr.cache_type(level) {
            CacheType::NoCache | CacheType::InstructionOnly => {
                continue;
            }
            CacheType::DataOnly | CacheType::SeparateInstructionAndData | CacheType::Unified => {}
        }

        // Select the Cache Level in CSSELR (Cache Size Selection Register)
        write_csselr_el1(CsselrEl1::new(false, level, false));

        // Barrier to ensure CSSELR write finishes before reading CCSIDR
        isb();

        // Cache Size ID Register (CCSIDR)
        let ccsidr = read_ccsidr_el1();

        let line_size = ccsidr.linesize();
        let ways = (ccsidr.bits() >> 3) & 0x3FF;
        let sets = (ccsidr.bits() >> 13) & 0x7FFF;

        let way_shift = (ways as u32).leading_zeros();

        for set in 0..=sets {
            for way in 0..=ways {
                const LEVEL_SHIFT: u8 = 1;
                const SET_WAY_SHIFT: u8 = 4;
                let dc_val = (way << way_shift)
                    | (set << (line_size + SET_WAY_SHIFT))
                    | (u64::from(level.level()) << LEVEL_SHIFT);

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
