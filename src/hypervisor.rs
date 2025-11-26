// Copyright 2025 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use core::arch::naked_asm;

use aarch64_rt::{RegisterStateRef, Stack, SuspendContext, warm_boot_entry};
use alloc::boxed::Box;
use log::debug;
use spin::mutex::SpinMutex;

use crate::{
    arch::{self, esr, far},
    platform::{Platform, PlatformImpl},
    simple_map::SimpleMap,
};

/// Entry point for EL1 execution.
///
/// This function configures the environment for EL1 execution and then jumps to the EL1 entry point.
///
/// # Safety
///
/// This function is unsafe because it modifies system registers and performs a context switch to EL1.
/// The caller must ensure that the provided arguments are valid and that the entry point is a valid
/// address for EL1 execution that never returns.
/// This function must be called in EL2.
pub unsafe fn entry_point_el1(arg0: u64, arg1: u64, arg2: u64, arg3: u64, entry_point: u64) -> ! {
    // Setup EL1
    // SAFETY: We are configuring HCR_EL2 to allow EL1 execution.
    unsafe {
        let mut hcr = arch::hcr_el2::read();
        hcr |= arch::hcr_el2::RW;
        hcr |= arch::hcr_el2::TSC;
        hcr &= !arch::hcr_el2::IMO;
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

    // SAFETY: The caller ensures that the provided arguments are valid and that this is called
    // from EL2. We've set the `elr_el2` system register right before calling this, and the caller
    // ensured that the value we've set is a valid address for EL1 execution that never returns.
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
/// The caller must ensure that the provided arguments are valid and that the `elr_el2` system register
/// contains a valid address for EL1 execution that never returns.
/// This function must be called in EL2.
#[unsafe(naked)]
pub unsafe extern "C" fn eret_to_el1(x0: u64, x1: u64, x2: u64, x3: u64) -> ! {
    naked_asm!(
        "mov x19, x0",
        "bl {disable_mmu_and_caches}",
        "mov x0, x19",
        "eret",
        disable_mmu_and_caches = sym arch::disable_mmu_and_caches,
    );
}

pub fn handle_sync_lower(mut register_state: RegisterStateRef) {
    let esr = esr();
    let ec = u8::try_from((esr >> 26) & 0x3f).expect("`& 0x3f` guarantees the value fits in u8");
    let ec = ExceptionClass::new(ec);

    match ec {
        ExceptionClass::HvcTrappedInAArch64 | ExceptionClass::SmcTrappedInAArch64 => {
            let function_id = register_state.registers[0];

            match function_id {
                0x8400_0000..=0x8400_001F | 0xC400_0000..=0xC400_001F => {
                    try_handle_psci(&mut register_state)
                        .expect("Unknown PSCI call: {register_state:?}");
                }
                _ => {
                    panic!("Unknown HVC/SMC call: {register_state:?}");
                }
            }
        }
        ExceptionClass::Unknown(_) => {
            panic!(
                "Unexpected sync_lower, far={:#x}, register_state={register_state:?}",
                far(),
            );
        }
    }
}

const AARCH64_INSTRUCTION_LENGTH: usize = 4;

fn try_handle_psci(register_state: &mut RegisterStateRef) -> Result<(), arm_psci::Error> {
    let [fn_id, arg0, arg1, arg2, ..] = register_state.registers;
    debug!(
        "Forwarding the PSCI call: fn_id={fn_id:#x}, arg0={arg0:#x}, arg1={arg1:#x}, arg2={arg2:#x}"
    );

    // SAFETY: We are handling a trapped HVC or SMC instruction, which is likely a PSCI call.
    // The arguments are passed from the guest.
    let out = unsafe { handle_psci(fn_id, arg0, arg1, arg2)? };
    debug!("PSCI call output: out={out:#x}");

    // SAFETY: This is an answer to the guest calling HVC/SMC, so it expects x0..3 will
    // get overwritten. The HVC/SMC call needs to be skipped so that after returning back
    // to the guest, it will not be executed again.
    unsafe {
        let regs = register_state.get_mut();
        regs.registers[0] = out;
        regs.registers[1] = 0;
        regs.registers[2] = 0;
        regs.registers[3] = 0;
        regs.elr += AARCH64_INSTRUCTION_LENGTH; // move to the next instruction to avoid looping
    }

    Ok(())
}

/// Handles a PSCI call.
///
/// # Errors
///
/// Returns an error when an unknown PSCI function has been called.
///
/// # Safety
///
/// This function is unsafe because it may call into the secure monitor via SMC or perform other
/// privileged operations.
unsafe fn handle_psci(fn_id: u64, arg0: u64, arg1: u64, arg2: u64) -> Result<u64, arm_psci::Error> {
    #[allow(clippy::enum_glob_use)]
    use arm_psci::Function::*;

    let psci_fn = arm_psci::Function::try_from(&[fn_id, arg0, arg1, arg2])?;
    match psci_fn {
        Version
        | CpuOff
        | AffinityInfo { .. }
        | Migrate { .. }
        | MigrateInfoType
        | MigrateInfoUpCpu { .. }
        | SystemOff
        | SystemOff2 { .. }
        | SystemReset
        | SystemReset2 { .. }
        | MemProtect { .. }
        | MemProtectCheckRange { .. }
        | Features { .. }
        | CpuFreeze
        | CpuDefaultSuspend { .. }
        | NodeHwState { .. }
        | SystemSuspend { .. }
        | SetSuspendMode { .. }
        | StatResidency { .. }
        | StatCount { .. } => {
            // forward the PSCI call
            let mut smc_args = [0; 17];
            smc_args[0] = arg0;
            smc_args[1] = arg1;
            smc_args[2] = arg2;
            #[expect(
                clippy::cast_possible_truncation,
                reason = "the fn_id is a u32 per specification, so can be truncated"
            )]
            let result = smccc::smc64(fn_id as u32, smc_args);
            Ok(result[0])
        }
        CpuOn { target_cpu, entry } => {
            let result = psci_cpu_on(fn_id, target_cpu, entry);
            Ok(u64::from(i32::from(result).cast_unsigned()))
        }
        CpuSuspend { state, entry } => {
            let result = psci_cpu_suspend(state, entry);
            Ok(result)
        }
    }
}

fn psci_cpu_on(
    fn_id: u64,
    mpidr: arm_psci::Mpidr,
    entry: arm_psci::EntryPoint,
) -> arm_psci::ReturnCode {
    let mpidr: u64 = mpidr.into();
    let stack = get_secondary_stack(mpidr);

    // SAFETY: aarch64_rt::start_core is safe to call with a valid stack.
    unsafe {
        aarch64_rt::start_core::<smccc::Smc, _, _>(mpidr, stack, move || {
            let entry_ptr = entry.entry_point_address();
            let arg = entry.context_id();
            debug!("Started core with fn_id={fn_id:#x}, mpidr={mpidr:#x}, entry_ptr={entry_ptr:#x}, arg={arg}");

            entry_point_el1(arg, 0, 0, 0, entry_ptr);
        }).expect("Failed to start core");
    }

    arm_psci::ReturnCode::Success
}

fn psci_cpu_suspend(power_state: arm_psci::PowerState, entry: arm_psci::EntryPoint) -> u64 {
    // SAFETY: Reading MPIDR_EL1 is safe.
    let mpidr = arch::mpidr_el1::read();
    let context = SuspendContext {
        stack_ptr: get_secondary_stack(mpidr).wrapping_add(1) as usize as u64,
        entry: restore_from_suspend,
        data: SuspendCoreData {
            mpidr,
            entry_point: entry.entry_point_address(),
            context_id: entry.context_id(),
        },
    };

    let context_ptr = core::ptr::from_mut(SUSPEND_CONTEXTS.lock().insert(mpidr, context));

    let result = smccc::psci::cpu_suspend::<smccc::Smc>(
        power_state.into(),
        warm_boot_entry::<SuspendCoreData> as usize as u64,
        context_ptr as u64,
    );

    // If we return here, the suspend failed or was not a power down.
    SUSPEND_CONTEXTS.lock().remove(&mpidr);

    match result {
        Ok(()) => u64::from(i32::from(arm_psci::ReturnCode::Success).cast_unsigned()),
        Err(err) => i64::from(err).cast_unsigned(),
    }
}

#[derive(Debug, Clone, Copy)]
struct SuspendCoreData {
    mpidr: u64,
    entry_point: u64,
    context_id: u64,
}

/// Restores the environment after a suspend.
///
/// # Safety
///
/// The caller must ensure the entry pointer and `context_id` for given mpidr saved in
/// [`SUSPEND_CONTEXTS`] is valid.
extern "C" fn restore_from_suspend(context: &mut SuspendContext<SuspendCoreData>) -> ! {
    let context = SUSPEND_CONTEXTS
        .lock()
        .remove(&context.data.mpidr)
        .expect("context not found for resuming CPU");
    debug!(
        "Restoring from suspend: entry={:#x}, ctx={:#x}",
        context.data.entry_point, context.data.context_id
    );

    // SAFETY: We are restoring the execution of the guest, assuming the entry point and
    // context_id we saved earlier from the guest is valid.
    unsafe {
        entry_point_el1(context.data.context_id, 0, 0, 0, context.data.entry_point);
    }
}

const MAX_CORES: usize = <PlatformImpl as Platform>::MAX_CORES;
static SUSPEND_CONTEXTS: SpinMutex<SimpleMap<u64, SuspendContext<SuspendCoreData>, MAX_CORES>> =
    SpinMutex::new(SimpleMap::new());

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
static SECONDARY_STACKS: SpinMutex<
    SimpleMap<u64, Box<Stack<SECONDARY_STACK_PAGE_COUNT>>, MAX_CORES>,
> = SpinMutex::new(SimpleMap::new());

fn get_secondary_stack(mpidr: u64) -> *mut Stack<SECONDARY_STACK_PAGE_COUNT> {
    let mut stack_map = SECONDARY_STACKS.lock();
    if let Some(stack) = stack_map.get_mut(&mpidr) {
        &raw mut **stack
    } else {
        &raw mut **stack_map.insert(mpidr, Box::default())
    }
}
