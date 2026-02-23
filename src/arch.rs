// Copyright 2025 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Architecture-specific code.

use arm_sysregs::{SctlrEl2, read_sctlr_el2};
use core::arch::naked_asm;

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
        "tlbi alle2is",
        "dsb sy",
        // We assume we have identity mapped pagetables for the currently running
        // code, so disabling MMU is safe.
        "msr sctlr_el2, x0",
        "isb",
        "mov x29, x27",
        "mov x30, x28",

        // Invalidate I-cache
        "ic iallu",

        // Final synchronization
        "isb",
        "ret",
        disable_mmu_and_caches_impl = sym disable_mmu_and_caches_impl,
    );
}

/// Returns new value for the `sctlr` register to be set.
extern "C" fn disable_mmu_and_caches_impl() -> u64 {
    // Disable MMU and caches
    let mut sctlr = read_sctlr_el2();
    sctlr -= SctlrEl2::M; // MMU Enable
    sctlr -= SctlrEl2::C; // Data Cache Enable
    sctlr -= SctlrEl2::I; // Instruction Cache Enable
    sctlr.bits()
}
