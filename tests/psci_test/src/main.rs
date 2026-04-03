// Copyright 2026 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![no_std]
#![no_main]

use aarch64_rt::{ExceptionHandlers, RegisterStateRef, entry, exception_handlers};
use arm_gic::{
    IntId, Trigger,
    gicv3::{GicCpuInterface, GicV3, Group, HIGHEST_NS_PRIORITY, InterruptGroup},
};
use arm_pl011_uart::Uart;
use arm_psci::PowerState;
use arm_sysregs::{
    CntpCtlEl0, CntpTvalEl0, read_cntfrq_el0, write_cntp_ctl_el0, write_cntp_tval_el0,
};
use core::fmt::Write;
use core::panic::PanicInfo;
use core::ptr::NonNull;
use core::sync::atomic::{AtomicBool, Ordering};
use smccc::psci::SuspendMode;
use smccc::{Hvc, psci};
use spin::Once;
use spin::mutex::{SpinMutex, SpinMutexGuard};

const UART_BASE: usize = 0x0900_0000;
const GICD_BASE: usize = 0x0800_0000;
const GICR_BASE: usize = 0x080a_0000;

const CORE_COUNT: usize = 4;

static UART: Once<SpinMutex<Uart>> = Once::new();
static IRQ_RECEIVED: AtomicBool = AtomicBool::new(false);
static GIC: Once<SpinMutex<GicV3>> = Once::new();

exception_handlers!(Exceptions);

struct Exceptions;
impl ExceptionHandlers for Exceptions {
    extern "C" fn irq_current(_register_state: RegisterStateRef) {
        println!("TEST: IRQ caught!");
        let intid = GicCpuInterface::get_and_acknowledge_interrupt(InterruptGroup::Group1)
            .expect("Failed to ack interrupt");

        println!("TEST: IRQ ID: {:?}", intid);

        if intid == TimerHelper::INTERRUPT_ID {
            println!("TEST: Timer IRQ received");
            TimerHelper::stop();
            IRQ_RECEIVED.store(true, Ordering::SeqCst);
        } else {
            println!("Unexpected IRQ: {:?}", intid);
        }

        GicCpuInterface::end_interrupt(intid, InterruptGroup::Group1);
    }
}

#[macro_export]
macro_rules! println {
    ($($arg:tt)*) => ({
        writeln!(get_uart(), $($arg)*).unwrap();
    });
}

fn get_uart() -> SpinMutexGuard<'static, Uart<'static>> {
    UART.call_once(|| {
        SpinMutex::new(Uart::new(unsafe {
            arm_pl011_uart::UniqueMmioPointer::new(NonNull::new(UART_BASE as *mut _).unwrap())
        }))
    })
    .lock()
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    let _ = writeln!(get_uart(), "PANIC: {}", info);
    power_off();
}

pub struct TimerHelper;
impl TimerHelper {
    pub const INTERRUPT_ID: IntId = IntId::ppi(14);

    pub fn set(ticks: u32) {
        write_cntp_tval_el0(CntpTvalEl0::empty().with_timervalue(ticks));
        write_cntp_ctl_el0(CntpCtlEl0::ENABLE);
    }

    pub fn stop() {
        write_cntp_ctl_el0(CntpCtlEl0::empty());
    }
}

struct GicHelper;
impl GicHelper {
    fn get() -> SpinMutexGuard<'static, GicV3<'static>> {
        GIC.call_once(|| {
            let mut gic = unsafe {
                GicV3::new(
                    arm_gic::UniqueMmioPointer::new(NonNull::new(GICD_BASE as *mut _).unwrap()),
                    NonNull::new(GICR_BASE as *mut _).unwrap(),
                    CORE_COUNT,
                    false,
                )
            };

            gic.distributor().enable_group1_non_secure(true);
            SpinMutex::new(gic)
        })
        .lock()
    }

    fn init_core() {
        GicCpuInterface::enable_system_register_el1(true);
        GicCpuInterface::enable_group1(true);
        arm_gic::irq_enable();
    }

    fn setup_timer_irq() {
        let mut gic = Self::get();

        let cpu = Some(0);
        let intid = TimerHelper::INTERRUPT_ID;

        gic.set_interrupt_priority(intid, cpu, HIGHEST_NS_PRIORITY)
            .unwrap();
        gic.set_group(intid, cpu, Group::Group1NS).unwrap();
        gic.set_trigger(intid, cpu, Trigger::Level).unwrap();
        gic.enable_interrupt(intid, cpu, true).unwrap();
    }
}

entry!(main);
fn main(_arg0: u64, _arg1: u64, _arg2: u64, _arg3: u64) -> ! {
    println!("TEST: Starting PSCI OSI Test");

    GicHelper::init_core();

    psci::set_suspend_mode::<Hvc>(SuspendMode::OsInitiated)
        .expect("TEST: FAILED: set_suspend_mode(OsInitiated) failed");
    println!("TEST: OSI Mode Enabled");

    test_standby();
    test_powerdown();

    // PowerDown doesn't return here, it resumes via powerdown_resume.
    unreachable!();
}

fn test_standby() {
    println!("TEST: Testing Standby...");

    let duration = read_cntfrq_el0().clockfreq() / 100; // 10ms
    reset_irq_received();

    // QEMU Standby State ID
    let pstate = u32::from(PowerState::StandbyOrRetention(0x01));

    arm_gic::irq_disable();
    GicHelper::setup_timer_irq();
    TimerHelper::set(duration);

    println!("TEST: Calling cpu_suspend (Standby)...");
    static mut DUMMY_STACK: aarch64_rt::Stack<1> = aarch64_rt::Stack::new();
    let stack_ptr = unsafe { (&raw mut DUMMY_STACK).add(1) as *mut u64 };

    unsafe {
        aarch64_rt::suspend_core::<Hvc>(pstate, stack_ptr, standby_unexpected_resume, 0)
            .expect("TEST: FAILED: Standby cpu_suspend returned error")
    }
    println!("TEST: Returned from cpu_suspend (Standby)");

    arm_gic::irq_enable();

    wait_for_irq();
    println!("TEST: Standby Passed");
}

extern "C" fn standby_unexpected_resume(_data: u64) -> ! {
    panic!("TEST: FAILED: Standby jumped to standby_unexpected_resume");
}

fn test_powerdown() {
    println!("TEST: Testing PowerDown...");

    let duration = read_cntfrq_el0().clockfreq() / 10; // 100ms
    reset_irq_received();

    // QEMU PowerDown State ID
    let pstate = u32::from(PowerState::PowerDown((1 << 12) | 0x002));

    arm_gic::irq_disable();
    GicHelper::setup_timer_irq();
    TimerHelper::set(duration);

    println!("TEST: Suspending (PowerDown)...");
    static mut RESUME_STACK: aarch64_rt::Stack<4> = aarch64_rt::Stack::new();
    let stack_ptr = unsafe { (&raw mut RESUME_STACK).add(1) as *mut u64 };

    unsafe {
        aarch64_rt::suspend_core::<Hvc>(pstate, stack_ptr, powerdown_resume, 0)
            .expect("TEST: FAILED: PowerDown cpu_suspend returned unexpectedly")
    }
}

extern "C" fn powerdown_resume(_data: u64) -> ! {
    GicHelper::init_core();
    GicHelper::setup_timer_irq();
    arm_gic::irq_enable();

    wait_for_irq();
    println!("TEST: PowerDown Passed");
    println!("TEST: All tests passed!");
    power_off();
}

fn reset_irq_received() {
    IRQ_RECEIVED.store(false, Ordering::SeqCst);
}

fn wait_for_irq() {
    while !IRQ_RECEIVED.load(Ordering::SeqCst) {
        core::hint::spin_loop();
    }
}

fn power_off() -> ! {
    let _ = psci::cpu_off::<Hvc>();
    // loop forever if the PSCI call failed
    loop {
        arm_gic::wfi();
    }
}
