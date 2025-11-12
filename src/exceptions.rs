// Copyright 2025 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

mod psci;

use core::arch::{asm, naked_asm};

use log::info;

use crate::exceptions::psci::PsciFunction;

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
        // Return after the HVC call
        "add x4, x4, #4",
        "str x4, [sp, #8 * 22 + 16]",

        "ldp x29, x30, [sp], #0x10",
        "ret",
        sync_lower_impl = sym sync_lower_impl,
    )
}

#[derive(Debug)]
enum ExceptionClass {
    HvcTrappedInAArch64,
    SmcTrappedInAArch64,
    #[allow(unused)]
    Unknown(u8),
}

impl ExceptionClass {
    fn new(value: u8) -> Self {
        match value {
            0x16 => Self::HvcTrappedInAArch64,
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
        ExceptionClass::HvcTrappedInAArch64 | ExceptionClass::SmcTrappedInAArch64 => {
            info!(
                "Forwarding the PSCI call: fn_id={x0:#x}, arg0={x1:#x}, arg1={x2:#x}, arg2={x3:#x}"
            );
            let out: u64;
            // SAFETY: assuming the PSCI call is correct
            unsafe {
                out = handle_psci(x0, x1, x2, x3);
            }
            info!(
                "Forwarded the PSCI call: fn_id={x0:#x}, arg0={x1:#x}, arg1={x2:#x}, arg2={x3:#x}; out={out:#x}"
            );

            out
        }
        ExceptionClass::Unknown(_) => {
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

unsafe fn handle_psci(fn_id: u64, arg0: u64, arg1: u64, arg2: u64) -> u64 {
    #[allow(clippy::enum_glob_use)]
    use PsciFunction::*;

    #[allow(clippy::cast_possible_truncation, reason = "the fn_id is a u32 per specification, so can be truncated")]
    let psci_fn = PsciFunction::from(fn_id as u32);
    match psci_fn {
        Version | CpuOff | AffinityInfo64 | Migrate64 | MigrateInfoType | MigrateInfoUpCpu64
        | PsciFeatures | SetSuspendMode =>
        // SAFETY: psci_forward is safe to call with these arguments.
        unsafe { psci_forward(fn_id, arg0, arg1, arg2) },
        CpuSuspend => todo!(),
        CpuOn | CpuOn64 => psci_cpu_on(fn_id, arg0, arg1, arg2),
        AffinityInfo => todo!(),
        Migrate => todo!(),
        MigrateInfoUpCpu => todo!(),
        SystemOff => todo!(),
        SystemReset => todo!(),
        SystemSuspend => todo!(),
        SystemReset2 => todo!(),
        CpuSuspend64 => todo!(),
        SystemSuspend64 => todo!(),
        SystemReset264 => todo!(),
        Unknown(_) => todo!(),
    }

    // switch (func_id) {
    // case PSCI_1_1_FN64_SYSTEM_RESET2:
    // 	return psci_system_reset2(host_ctxt);
    // case PSCI_1_0_FN_PSCI_FEATURES:
    // case PSCI_1_0_FN_SET_SUSPEND_MODE:
    // 	return psci_forward(host_ctxt);
    // case PSCI_1_0_FN64_SYSTEM_SUSPEND:
    // 	return psci_system_suspend(func_id, host_ctxt);
    // default:
    // 	return psci_0_2_handler(func_id, host_ctxt);
    // }

    // 	switch (func_id) {
    // case PSCI_0_2_FN_PSCI_VERSION:
    // case PSCI_0_2_FN_CPU_OFF:
    // case PSCI_0_2_FN64_AFFINITY_INFO:
    // case PSCI_0_2_FN64_MIGRATE:
    // case PSCI_0_2_FN_MIGRATE_INFO_TYPE:
    // case PSCI_0_2_FN64_MIGRATE_INFO_UP_CPU:
    // 	return psci_forward(host_ctxt);
    // /*
    //  * SYSTEM_OFF/RESET should not return according to the spec.
    //  * Allow it so as to stay robust to broken firmware.
    //  */
    // case PSCI_0_2_FN_SYSTEM_OFF:
    // case PSCI_0_2_FN_SYSTEM_RESET:
    // 	pkvm_poison_pvmfw_pages();
    // 	/* Avoid racing with a MEM_PROTECT call. */
    // 	hyp_spin_lock(&mem_protect_lock);
    // 	return psci_forward(host_ctxt);
    // case PSCI_0_2_FN64_CPU_SUSPEND:
    // 	return psci_cpu_suspend(func_id, host_ctxt);
    // case PSCI_0_2_FN64_CPU_ON:
    // 	return psci_cpu_on(func_id, host_ctxt);
    // default:
    // 	return PSCI_RET_NOT_SUPPORTED;
    // }
}

static mut SECONDARY_STACK: aarch64_rt::Stack<40> = aarch64_rt::Stack::<40>::new();

fn psci_cpu_on(fn_id: u64, mpidr: u64, entry_ptr: u64, arg: u64) -> u64 {
    // SAFETY: aarch64_rt::start_core is safe to call with a valid stack.
    unsafe {
        // aarch64_rt::start_core::<smccc::Smc, _, _>(mpidr, &raw mut SECONDARY_STACK, xd).unwrap();
        aarch64_rt::start_core::<smccc::Smc, _, _>(mpidr, &raw mut SECONDARY_STACK, move || {
            info!("Started core with fn_id={fn_id:#x}, mpidr={mpidr:#x}, entry_ptr={entry_ptr:#x}, arg={arg}");

            run_secondary_core_el1(arg, entry_ptr);

            panic!("secondary core returned");
        }).expect("Failed to start core");
    }
    0
}

#[unsafe(naked)]
#[rustfmt::skip]
unsafe extern "C" fn run_secondary_core_el1(arg: u64, entry_point: u64) {
    naked_asm!(
        // Disable MMU and caches
        "mrs x5, sctlr_el2",
        "bic x5, x5, #(1 << 0)",   // MMU disable
        "bic x5, x5, #(1 << 2)",   // Data Cache disable
        "bic x5, x5, #(1 << 12)",  // Instruction Cache Disable
        "msr sctlr_el2, x5",
        "isb",
        "dsb sy",

        // Invalidate D-cache by set/way to the point of coherency
        "mrs x5, clidr_el1",      // x5 = CLIDR
        "and x6, x5, #0x7000000", // x6 = LoC
        "lsr x6, x6, #23",
        "cbz x6, 2f",             // if LoC is 0, then no need to clean
        "mov x10, #0",            // x10 = cache level * 2

        // loop over cache levels
        "1:",
            "add x12, x10, x10, lsr #1", // x12 = level * 3
            "lsr x11, x5, x12",
            "and x11, x11, #7",          // x11 = cache type
            "cmp x11, #2",               // is it a data or unified cache?
            "b.lt 3f",                   // if not, skip to next level
            "msr csselr_el1, x10",       // select cache level
            "isb",                       // sync change of csselr
            "mrs x11, ccsidr_el1",       // x11 = ccsidr
            "and x12, x11, #7",          // x12 = log2(line size) - 4
            "add x12, x12, #4",          // x12 = log2(line size)
            "and x13, x11, #0x3ff",      // x13 = (number of ways - 1)
            "and x14, x11, #0x7fff000",  // x14 = (number of sets - 1)
            "lsr x14, x14, #13",
            "clz w15, w13",              // w15 = 31 - log2(ways)
            // loop over ways
            "4:",
                "mov x9, x14",           // x9 = set number
                // loop over sets
                "5:",
                    "lsl x7, x13, x15",
                    "lsl x8, x9, x12",
                    "orr x11, x10, x7",  // x11 = (level << 1) | (way << ...)
                    "orr x11, x11, x8",  // x11 |= (set << log2(line size))
                    "dc isw, x11",       // invalidate by set/way
                    "subs x9, x9, #1",
                    "b.ge 5b",
                    "subs x13, x13, #1",
                    "b.ge 4b",
            "3:",
                "add x10, x10, #2",      // next cache level
                "cmp x6, x10",
                "b.gt 1b",
    "2:",
        "dsb sy",
        "isb",

        // Invalidate I-cache
        "ic iallu",
        "tlbi alle2is",

        // Final synchronization
        "dsb ish",
        "isb",

        // Setup EL1
        // EL1 is AArch64
        "mov x5, #(1 << 31)",
        "orr x5, x5, #(1 << 19)",
        "bic x5, x5, #(1 << 4)",
        // "bic x5, x5, #(1 << 3)",
        "msr hcr_el2, x5",

        //todo
        "mov     x5, #0",
        "msr     CNTVOFF_EL2, x5",
        
        // Allow access to timers
        "mov x5, #3",
        "msr cnthctl_el2, x5",

        // Setup SPSR_EL2 to enter EL1h
        // Mask debug, SError, IRQ, and FIQ
        "mov x5, #(0b1111 << 6)",
        // EL1h
        "mov x6, #5",
        "orr x5, x5, x6",
        "msr spsr_el2, x5",

        // Set ELR_EL2 to the kernel entry point
        "msr elr_el2, x1",

        // Set stack pointer for EL1
        "mov x5, sp",
        "msr sp_el1, x5",

        "mov x1, xzr",
        "mov x2, xzr",
        "mov x3, xzr",

        "eret",
    )
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
