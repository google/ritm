// Copyright 2025 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use crate::{
    arch,
    platform::{Platform, PlatformImpl},
    simple_map::SimpleMap,
};
use aarch64_paging::descriptor::Stage2Attributes;
use aarch64_paging::idmap::IdMap;
use aarch64_rt::{RegisterStateRef, Stack};
use arm_sysregs::{
    CnthctlEl2, CntvoffEl2, ElrEl1, ElrEl2, EsrEl1, FarEl1, HcrEl2, MpidrEl1, SpsrEl1, SpsrEl2,
    VtcrEl2, read_cnthctl_el2, read_esr_el2, read_far_el2, read_hcr_el2, read_mpidr_el1,
    read_spsr_el2, read_vbar_el1, write_cnthctl_el2, write_cntvoff_el2, write_elr_el1,
    write_elr_el2, write_esr_el1, write_far_el1, write_hcr_el2, write_spsr_el1, write_spsr_el2,
    write_vtcr_el2,
};
use core::arch::naked_asm;
use log::debug;
use spin::Once;
use spin::mutex::SpinMutex;

static STAGE2_MAP: Once<SpinMutex<IdMap<Stage2Attributes>>> = Once::new();

const SPSR_EL1H: u8 = 5;
const T0SZ_MAX_SIZE: u8 = 64;

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
    setup_stage2();
    // Setup EL1
    let mut hcr = read_hcr_el2();
    hcr |= HcrEl2::RW;
    hcr |= HcrEl2::TSC;
    hcr |= HcrEl2::VM;
    hcr -= HcrEl2::IMO;
    // SAFETY: We are configuring HCR_EL2 to allow EL1 execution.
    unsafe {
        write_hcr_el2(hcr);
    }

    // Reset the timer offset.
    write_cntvoff_el2(CntvoffEl2::default());

    // Allow access to timers
    let mut cnthctl = read_cnthctl_el2();
    cnthctl |= CnthctlEl2::EL0PCTEN;
    cnthctl |= CnthctlEl2::EL1PCEN;
    write_cnthctl_el2(cnthctl);

    let mut spsr = read_spsr_el2();
    // Setup SPSR_EL2 to enter EL1h
    spsr.set_m_3_0(SPSR_EL1H);
    // Mask debug, SError, IRQ, and FIQ
    spsr |= SpsrEl2::D;
    spsr |= SpsrEl2::A;
    spsr |= SpsrEl2::I;
    spsr |= SpsrEl2::F;
    // SAFETY: Configuring SPSR_EL2 for the return to EL1.
    unsafe {
        write_spsr_el2(spsr);
    }

    // Set ELR_EL2 to the kernel entry point
    let mut elr = ElrEl2::default();
    elr.set_addr(entry_point);
    // SAFETY: We trust the caller the entry point is valid.
    unsafe {
        write_elr_el2(elr);
    }

    // SAFETY: The caller ensures that the provided arguments are valid and that this is called
    // from EL2. We've set the `elr_el2` system register right before calling this, and the caller
    // ensured that the value we've set is a valid address for EL1 execution that never returns.
    unsafe {
        eret_to_el1(arg0, arg1, arg2, arg3);
    }
}

fn setup_stage2() {
    debug!("Setting up stage 2 page table");
    let mut idmap = STAGE2_MAP
        .call_once(|| SpinMutex::new(PlatformImpl::make_stage2_pagetable()))
        .lock();

    let root_pa = idmap.root_address().0;
    debug!("Root PA: {root_pa:#x}");

    // Activate the page table
    // SAFETY: We are initializing the Stage 2 translation. The guest is not running yet.
    let ttbr = unsafe { idmap.activate() };
    debug!("idmap.activate() returned ttbr={ttbr:#x}");

    let mut vtcr = VtcrEl2::default();
    vtcr.set_ps(2); // 40 bit physical address size
    vtcr.set_tg0(0); // 4kB granule size
    vtcr.set_sh0(3); // Inner shareable memory
    vtcr.set_orgn0(1); // Outer Write-Back Read-Allocate Write-Allocate Cacheable
    vtcr.set_irgn0(1); // Inner Write-Back Read-Allocate Write-Allocate Cacheable
    vtcr.set_sl0(2); // L0 starting level
    vtcr.set_t0sz(T0SZ_MAX_SIZE - 40); // 40 bit size offset
    debug!("Writing VTCR_EL2={vtcr:#x}...");
    // SAFETY: We are initializing the Stage 2 translation. The guest is not running yet.
    unsafe {
        write_vtcr_el2(vtcr);
    }
    arch::tlbi_vmalls12e1();
    arch::dsb();
    arch::isb();
    debug!("Stage 2 activation complete.");
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
        // overwrite the registers to avoid leaking data from RITM
        "mov x4, 0",
        "mov x5, 0",
        "mov x6, 0",
        "mov x7, 0",
        "mov x8, 0",
        "mov x9, 0",
        "mov x10, 0",
        "mov x11, 0",
        "mov x12, 0",
        "mov x13, 0",
        "mov x14, 0",
        "mov x15, 0",
        "mov x16, 0",
        "mov x17, 0",
        "mov x18, 0",
        "mov x19, 0",
        "mov x20, 0",
        "mov x21, 0",
        "mov x22, 0",
        "mov x23, 0",
        "mov x24, 0",
        "mov x25, 0",
        "mov x26, 0",
        "mov x27, 0",
        "mov x28, 0",
        "mov x29, 0",
        "mov x30, 0",
        "movi v0.2d, #0",
        "movi v1.2d, #0",
        "movi v2.2d, #0",
        "movi v3.2d, #0",
        "movi v4.2d, #0",
        "movi v5.2d, #0",
        "movi v6.2d, #0",
        "movi v7.2d, #0",
        "movi v8.2d, #0",
        "movi v9.2d, #0",
        "movi v10.2d, #0",
        "movi v11.2d, #0",
        "movi v12.2d, #0",
        "movi v13.2d, #0",
        "movi v14.2d, #0",
        "movi v15.2d, #0",
        "movi v16.2d, #0",
        "movi v17.2d, #0",
        "movi v18.2d, #0",
        "movi v19.2d, #0",
        "movi v20.2d, #0",
        "movi v21.2d, #0",
        "movi v22.2d, #0",
        "movi v23.2d, #0",
        "movi v24.2d, #0",
        "movi v25.2d, #0",
        "movi v26.2d, #0",
        "movi v27.2d, #0",
        "movi v28.2d, #0",
        "movi v29.2d, #0",
        "movi v30.2d, #0",
        "movi v31.2d, #0",
        "eret",
    );
}

pub fn handle_sync_lower(mut register_state: RegisterStateRef) {
    let esr_el2 = read_esr_el2();
    let ec = ExceptionClass::from(esr_el2.ec());

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
        ExceptionClass::DataAbortLowerEL => {
            inject_data_abort(&mut register_state);
        }
        ExceptionClass::Unknown(val) => {
            panic!(
                "Unexpected sync_lower, esr={esr_el2:#x}, ec={val:#x}, far={:#x}, register_state={register_state:?}",
                read_far_el2(),
            );
        }
    }
}

fn inject_data_abort(register_state: &mut RegisterStateRef) {
    // SAFETY: We are modifying the saved register state to redirect execution.
    let regs = unsafe { register_state.get_mut() };
    let fault_addr = read_far_el2();
    let esr = read_esr_el2();

    debug!("Injecting data abort to guest: fault_addr={fault_addr:#x}, esr={esr:#x}");

    // Read guest VBAR
    let vbar = read_vbar_el1().bits();
    assert_ne!(
        vbar, 0,
        "Guest VBAR_EL1 is 0, cannot inject data abort. Fault addr: {fault_addr:#x}"
    );
    let handler = vbar + 0x200; // Current EL with SPx Sync

    // Save current context to guest EL1 regs
    // SAFETY: We are accessing EL1 system registers to inject exception.
    unsafe {
        write_elr_el1(ElrEl1::from_bits_retain(regs.elr as u64));
        write_spsr_el1(SpsrEl1::from_bits_retain(regs.spsr));
        write_far_el1(FarEl1::from_bits_retain(fault_addr.va()));
    }
    write_esr_el1(EsrEl1::from_bits_retain(esr.bits()));

    // Redirect execution
    #[expect(
        clippy::cast_possible_truncation,
        reason = "only 64-bit target is supported"
    )]
    {
        regs.elr = handler as usize;
    }
    // Mask all interrupts (DAIF) and set mode to EL1h
    let mut spsr = SpsrEl1::default();
    spsr.set_m_3_0(SPSR_EL1H);
    spsr |= SpsrEl1::D;
    spsr |= SpsrEl1::A;
    spsr |= SpsrEl1::I;
    spsr |= SpsrEl1::F;
    regs.spsr = spsr.bits();
}

const AARCH64_INSTRUCTION_LENGTH: usize = 4;

fn try_handle_psci(register_state: &mut RegisterStateRef) -> Result<(), arm_psci::Error> {
    let [fn_id, arg0, arg1, arg2, ..] = register_state.registers;
    debug!(
        "Forwarding the PSCI call: fn_id={fn_id:#x}, arg0={arg0:#x}, arg1={arg1:#x}, arg2={arg2:#x}"
    );

    let out = handle_psci(fn_id, arg0, arg1, arg2)?;
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
fn handle_psci(fn_id: u64, arg0: u64, arg1: u64, arg2: u64) -> Result<u64, arm_psci::Error> {
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
            Ok(psci_result_to_u64(result))
        }
        CpuSuspend { state, entry } => {
            let result = psci_cpu_suspend(state, entry);
            Ok(psci_result_to_u64(result))
        }
    }
}

fn psci_cpu_on(
    fn_id: u64,
    mpidr: arm_psci::Mpidr,
    entry: arm_psci::EntryPoint,
) -> Result<(), smccc::psci::Error> {
    let mpidr_u64 = mpidr.into();
    let mpidr = MpidrEl1::from_bits_retain(mpidr_u64);
    let stack = get_secondary_stack(mpidr);

    // SAFETY: aarch64_rt::start_core is safe to call with a valid stack.
    unsafe {
        aarch64_rt::start_core::<smccc::Smc, _, _>(mpidr_u64, stack, move || {
            let entry_ptr = entry.entry_point_address();
            let arg = entry.context_id();
            debug!(
                "Started core with fn_id={fn_id:#x}, mpidr={mpidr:#x}, entry_ptr={entry_ptr:#x}, arg={arg}"
            );

            entry_point_el1(arg, 0, 0, 0, entry_ptr);
        })
    }
}

fn psci_cpu_suspend(
    power_state: arm_psci::PowerState,
    entry: arm_psci::EntryPoint,
) -> Result<(), smccc::psci::Error> {
    let mpidr = read_mpidr_el1();

    let context = SuspendCoreData {
        entry_point: entry.entry_point_address(),
        context_id: entry.context_id(),
    };
    SUSPEND_CONTEXTS.lock().insert(mpidr, context);

    // SAFETY: We treat CPU_SUSPEND as CPU_ON, resetting the stack pointer to the bottom of the stack
    // and not assuming anything is there.
    let result = unsafe {
        aarch64_rt::suspend_core::<smccc::Smc>(
            power_state.into(),
            // The stack grows downwards on aarch64, so get a pointer to the end of the stack.
            get_secondary_stack(mpidr).wrapping_add(1) as usize as *mut u64,
            restore_from_suspend,
            0,
        )
    };

    // If we return here, suspend failed or was not a power down.
    SUSPEND_CONTEXTS.lock().remove(&mpidr);

    result
}

fn psci_result_to_u64(result: Result<(), smccc::psci::Error>) -> u64 {
    match result {
        Ok(()) => u64::from(i32::from(arm_psci::ReturnCode::Success).cast_unsigned()),
        Err(err) => i64::from(err).cast_unsigned(),
    }
}

#[derive(Debug, Clone, Copy)]
struct SuspendCoreData {
    entry_point: u64,
    context_id: u64,
}

/// Restores the environment after a suspend.
///
/// # Safety
///
/// The caller must ensure the entry pointer and `context_id` for given mpidr saved in
/// [`SUSPEND_CONTEXTS`] is valid.
extern "C" fn restore_from_suspend(_data: u64) -> ! {
    let mpidr = read_mpidr_el1();
    let context = SUSPEND_CONTEXTS
        .lock()
        .remove(&mpidr)
        .expect("context not found for resuming CPU");
    debug!(
        "Restoring from suspend: entry={:#x}, ctx={:#x}",
        context.entry_point, context.context_id
    );

    // SAFETY: We are restoring the execution of the guest, assuming the entry point and
    // context_id we saved earlier from the guest is valid.
    unsafe {
        entry_point_el1(context.context_id, 0, 0, 0, context.entry_point);
    }
}

const MAX_CORES: usize = <PlatformImpl as Platform>::MAX_CORES;
static SUSPEND_CONTEXTS: SpinMutex<SimpleMap<MpidrEl1, SuspendCoreData, MAX_CORES>> =
    SpinMutex::new(SimpleMap::new());

/// The class of an exception.
#[derive(Debug)]
enum ExceptionClass {
    /// HVC instruction execution in `AArch64` state.
    HvcTrappedInAArch64,
    /// SMC instruction execution in `AArch64` state.
    SmcTrappedInAArch64,
    /// Data Abort taken without a change in Exception Level.
    DataAbortLowerEL,
    #[allow(unused)]
    /// Unknown exception class.
    Unknown(u8),
}

impl From<u8> for ExceptionClass {
    fn from(value: u8) -> Self {
        match value {
            0x16 => Self::HvcTrappedInAArch64,
            0x17 => Self::SmcTrappedInAArch64,
            0x24 => Self::DataAbortLowerEL,
            _ => Self::Unknown(value),
        }
    }
}

/// The number of pages to allocate for each secondary core stack.
const SECONDARY_STACK_PAGE_COUNT: usize = 4;
static SECONDARY_STACKS: SpinMutex<
    SimpleMap<MpidrEl1, Stack<SECONDARY_STACK_PAGE_COUNT>, MAX_CORES>,
> = SpinMutex::new(SimpleMap::new());

/// Returns a pointer to a stack for a given CPU.
///
/// # Safety
///
/// The pointers are safe to read/write as long as the stack never exceeds
/// `SECONDARY_STACK_PAGE_COUNT` in size.
///
/// The caller must ensure that pointers obtained for a specific CPU are only written
/// by this CPU.
fn get_secondary_stack(mpidr: MpidrEl1) -> *mut Stack<SECONDARY_STACK_PAGE_COUNT> {
    let mut stack_map = SECONDARY_STACKS.lock();
    // We never remove items from the map, so each CPU always gets its own stack which never
    // gets invalidated.
    if let Some(stack) = stack_map.get_mut(&mpidr) {
        &raw mut *stack
    } else {
        &raw mut *stack_map.insert(mpidr, Stack::default())
    }
}
