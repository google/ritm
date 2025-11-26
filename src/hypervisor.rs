// Copyright 2025 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

mod psci;

use core::arch::naked_asm;

use aarch64_rt::Stack;
use alloc::{boxed::Box, collections::btree_map::BTreeMap};
use log::debug;
use spin::mutex::SpinMutex;

use crate::{
    arch::{self, esr, far},
    hypervisor::psci::PsciFunction,
};

/// Entry point for EL1 execution.
///
/// This function configures the environment for EL1 execution and then jumps to the EL1 entry point.
///
/// # Safety
///
/// This function is unsafe because it modifies system registers and performs a context switch to EL1.
/// The caller must ensure that the provided arguments are valid and that the entry point is a valid
/// address for EL1 execution.
pub unsafe fn entry_point_el1(arg0: u64, arg1: u64, arg2: u64, arg3: u64, entry_point: u64) -> ! {
    arch::disable_mmu_and_caches();

    // Setup EL1
    // SAFETY: We are configuring HCR_EL2 to allow EL1 execution.
    unsafe {
        let mut hcr = arch::hcr_el2::read();
        hcr |= arch::hcr_el2::RW;
        hcr |= arch::hcr_el2::TID1;
        hcr &= !arch::hcr_el2::AMO;
        arch::hcr_el2::write(hcr);
    }

    // Reset the timer offset.
    // SAFETY: Resetting the CNTVOFF_EL2 is needed as part of
    // preparing an environment to run Linux.
    unsafe {
        arch::cntvoff_el2::write(0);
    }

    // Allow access to timers
    // SAFETY: Writing to CNTHCTL_EL2 is needed as part of
    // preparing an environment to run Linux.
    unsafe {
        let mut cnthctl = arch::cnthctl_el2::read();
        cnthctl |= arch::cnthctl_el2::ENABLE;
        cnthctl |= arch::cnthctl_el2::IMASK;
        arch::cnthctl_el2::write(cnthctl);
    }

    // Setup SPSR_EL2 to enter EL1h
    // Mask debug, SError, IRQ, and FIQ
    // SAFETY: Configuring SPSR_EL2 for the return to EL1.
    unsafe {
        let mut spsr = arch::spsr_el2::read();
        spsr |= arch::spsr_el2::MASK_ALL;
        spsr |= arch::spsr_el2::EL1H;
        arch::spsr_el2::write(spsr);
    }

    // Set ELR_EL2 to the kernel entry point
    // SAFETY: We trust the caller the entry point is valid.
    unsafe {
        arch::elr_el2::write(entry_point);
    }

    // Set stack pointer for EL1
    // SAFETY: We only affect EL1.
    unsafe {
        arch::sp_el1::write(arch::sp());
    }

    debug!("Exiting to EL1.");

    // SAFETY: This is a call to the hypervisor, which is safe.
    unsafe {
        eret_to_el1(arg0, arg1, arg2, arg3);
    }
}

/// Returns to EL1.
///
/// This function executes the `eret` instruction to return to EL1 with the provided arguments.
///
/// # Safety
///
/// This function is unsafe because it executes `eret` which changes the execution level and jumps
/// to the address in `ELR_EL2`.
#[unsafe(naked)]
pub unsafe extern "C" fn eret_to_el1(x0: u64, x1: u64, x2: u64, x3: u64) -> ! {
    naked_asm!("eret");
}

#[unsafe(no_mangle)]
pub extern "C" fn sync_lower_impl(elr: u64, _spsr: u64, x0: u64, x1: u64, x2: u64, x3: u64) -> u64 {
    let esr = esr();
    let ec = u8::try_from((esr >> 26) & 0x3f).expect("`& 0x3f` guarantees the value fits in u8");
    let ec = ExceptionClass::new(ec);

    match ec {
        ExceptionClass::HvcTrappedInAArch64 | ExceptionClass::SmcTrappedInAArch64 => {
            debug!(
                "Forwarding the PSCI call: fn_id={x0:#x}, arg0={x1:#x}, arg1={x2:#x}, arg2={x3:#x}"
            );
            let out: u64;
            // SAFETY: We are handling a trapped HVC or SMC instruction, which is likely a PSCI call.
            // The arguments are passed from the guest.
            unsafe {
                out = handle_psci(x0, x1, x2, x3);
            }
            debug!("PSCI call output: out={out:#x}");

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

/// Handles a PSCI call.
///
/// # Safety
///
/// This function is unsafe because it may call into the secure monitor via SMC or perform other
/// privileged operations.
unsafe fn handle_psci(fn_id: u64, arg0: u64, arg1: u64, arg2: u64) -> u64 {
    #[allow(clippy::enum_glob_use)]
    use PsciFunction::*;

    #[allow(
        clippy::cast_possible_truncation,
        reason = "the fn_id is a u32 per specification, so can be truncated"
    )]
    let psci_fn = PsciFunction::from(fn_id as u32);
    match psci_fn {
        Version | CpuOff | AffinityInfo64 | Migrate64 | MigrateInfoType | MigrateInfoUpCpu64
        | PsciFeatures | SetSuspendMode | CpuSuspend | CpuSuspend64 | AffinityInfo | Migrate
        | MigrateInfoUpCpu | SystemOff | SystemReset | SystemSuspend | SystemSuspend64
        | SystemReset2 | SystemReset264 =>
        // SAFETY: We assume that we've got a valid PSCI call and that it's safe to forward it.
        unsafe { psci_forward(fn_id, arg0, arg1, arg2) },
        CpuOn | CpuOn64 => psci_cpu_on(fn_id, arg0, arg1, arg2),
        Unknown(_) => panic!("Unsupported PSCI call"),
    }
}

fn psci_cpu_on(fn_id: u64, mpidr: u64, entry_ptr: u64, arg: u64) -> u64 {
    let stack = get_secondary_stack(mpidr);

    // SAFETY: aarch64_rt::start_core is safe to call with a valid stack.
    unsafe {
        aarch64_rt::start_core::<smccc::Smc, _, _>(mpidr, stack, move || {
            debug!("Started core with fn_id={fn_id:#x}, mpidr={mpidr:#x}, entry_ptr={entry_ptr:#x}, arg={arg}");

            entry_point_el1(arg, 0, 0, 0, entry_ptr);
        }).expect("Failed to start core");
    }

    #[allow(clippy::cast_sign_loss)]
    {
        i32::from(psci::PsciReturn::Success) as u64
    }
}

/// Forwards a PSCI call to the secure monitor.
///
/// # Safety
///
/// This function is unsafe because it executes an SMC instruction.
#[unsafe(naked)]
unsafe extern "C" fn psci_forward(fn_id: u64, arg0: u64, arg1: u64, arg2: u64) -> u64 {
    naked_asm! {
        "smc #0",
        "ret",
    };
}

/// The class of an exception.
#[derive(Debug)]
enum ExceptionClass {
    /// HVC instruction execution in `AArch64` state.
    HvcTrappedInAArch64,
    /// SMC instruction execution in `AArch64` state.
    SmcTrappedInAArch64,
    #[allow(unused)]
    /// Unknown exception class.
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

/// The number of pages to allocate for each secondary core stack.
const SECONDARY_STACK_PAGE_COUNT: usize = 4;

/// A pointer to a stack allocated for a secondary CPU.
///
/// This must not be dropped as long as the secondary CPU is running.
struct SecondaryStack {
    stack: Box<Stack<SECONDARY_STACK_PAGE_COUNT>>,
}

impl SecondaryStack {
    fn ptr(&mut self) -> *mut Stack<SECONDARY_STACK_PAGE_COUNT> {
        &raw mut *self.stack
    }
}

impl Default for SecondaryStack {
    fn default() -> Self {
        Self {
            stack: Box::new(Stack::<SECONDARY_STACK_PAGE_COUNT>::new()),
        }
    }
}

static SECONDARY_STACKS: SpinMutex<BTreeMap<u64, SecondaryStack>> = SpinMutex::new(BTreeMap::new());

fn get_secondary_stack(mpidr: u64) -> *mut Stack<SECONDARY_STACK_PAGE_COUNT> {
    SECONDARY_STACKS.lock().entry(mpidr).or_default().ptr()
}
