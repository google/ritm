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
use arm_pl011_uart::Uart;
use arm_psci::PowerState;
use arm_sysregs::read_cntfrq_el0;
use core::fmt::Write;
use core::panic::PanicInfo;
use core::ptr::NonNull;
use core::sync::atomic::{AtomicBool, Ordering};
use percore::exception_free;
use smccc::psci::SuspendMode;
use smccc::{Hvc, psci};
use spin::Lazy;
use spin::mutex::SpinMutex;

mod gic_helper;
mod timer_helper;

const UART_BASE: usize = 0x0900_0000;

const CORE_COUNT: usize = 4;

static UART: Lazy<SpinMutex<Uart>> = Lazy::new(create_uart);
static IRQ_RECEIVED: AtomicBool = AtomicBool::new(false);

exception_handlers!(Exceptions);

struct Exceptions;
impl ExceptionHandlers for Exceptions {
    extern "C" fn irq_current(_register_state: RegisterStateRef) {
        println!("IRQ caught!");
        let interrupt = gic_helper::GicInterrupt::ack_interrupt();

        println!("IRQ ID: {:?}", interrupt.id);

        if interrupt.id == timer_helper::INTERRUPT_ID {
            println!("Timer IRQ received");
            timer_helper::stop();
            IRQ_RECEIVED.store(true, Ordering::SeqCst);
        } else {
            println!("Unexpected IRQ: {:?}", interrupt.id);
        }
    }
}

#[macro_export]
macro_rules! println {
    ($($arg:tt)*) => ({
        let mut uart = UART.lock();
        write!(uart, "TEST: ").unwrap();
        writeln!(uart, $($arg)*).unwrap();
    });
}

fn create_uart() -> SpinMutex<Uart<'static>> {
    SpinMutex::new(Uart::new(unsafe {
        arm_pl011_uart::UniqueMmioPointer::new(NonNull::new(UART_BASE as *mut _).unwrap())
    }))
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    let _ = writeln!(UART.lock(), "PANIC: {}", info);
    power_off();
}

entry!(main);
fn main(_arg0: u64, _arg1: u64, _arg2: u64, _arg3: u64) -> ! {
    println!("Starting PSCI OSI Test");

    gic_helper::init_core();

    psci::set_suspend_mode::<Hvc>(SuspendMode::OsInitiated)
        .expect("FAILED: set_suspend_mode(OsInitiated) failed");
    println!("OSI Mode Enabled");

    test_standby();
    test_powerdown();
}

fn test_standby() {
    println!("Testing Standby...");

    let duration = read_cntfrq_el0().clockfreq() / 100; // 10ms
    reset_irq_received();

    // QEMU Standby State ID
    let pstate = u32::from(PowerState::StandbyOrRetention(0x01));

    arm_gic::irq_disable();
    gic_helper::setup_timer_irq();
    timer_helper::set(duration);

    println!("Calling cpu_suspend (Standby)...");
    static mut DUMMY_STACK: aarch64_rt::Stack<1> = aarch64_rt::Stack::new();
    let stack_ptr = unsafe { (&raw mut DUMMY_STACK).add(1) as *mut u64 };

    unsafe {
        aarch64_rt::suspend_core::<Hvc>(pstate, stack_ptr, standby_unexpected_resume, 0)
            .expect("FAILED: Standby cpu_suspend returned error")
    }
    println!("Returned from cpu_suspend (Standby)");

    arm_gic::irq_enable();

    wait_for_irq();
    println!("Standby Passed");
}

extern "C" fn standby_unexpected_resume(_data: u64) -> ! {
    panic!("FAILED: Standby jumped to standby_unexpected_resume");
}

fn test_powerdown() -> ! {
    println!("Testing PowerDown...");

    let duration = read_cntfrq_el0().clockfreq() / 10; // 100ms
    reset_irq_received();

    // QEMU PowerDown State ID
    let pstate = u32::from(PowerState::PowerDown((1 << 12) | 0x002));

    arm_gic::irq_disable();
    gic_helper::setup_timer_irq();
    timer_helper::set(duration);

    println!("Suspending (PowerDown)...");
    static mut RESUME_STACK: aarch64_rt::Stack<4> = aarch64_rt::Stack::new();
    let stack_ptr = unsafe { (&raw mut RESUME_STACK).add(1) as *mut u64 };

    unsafe {
        aarch64_rt::suspend_core::<Hvc>(pstate, stack_ptr, powerdown_resume, 0)
            .expect("FAILED: PowerDown cpu_suspend returned unexpectedly");
    }
    unreachable!("PowerDown suspend should never return")
}

extern "C" fn powerdown_resume(_data: u64) -> ! {
    gic_helper::init_core();
    gic_helper::setup_timer_irq();
    arm_gic::irq_enable();

    wait_for_irq();
    println!("PowerDown Passed");
    println!("All tests passed!");
    power_off();
}

fn reset_irq_received() {
    IRQ_RECEIVED.store(false, Ordering::SeqCst);
}

fn wait_for_irq() {
    while !exception_free(|_token| {
        let received = IRQ_RECEIVED.load(Ordering::SeqCst);
        if !received {
            arm_gic::wfi();
        }
        received
    }) {}
}

fn power_off() -> ! {
    let _ = psci::cpu_off::<Hvc>();
    // loop forever if the PSCI call failed
    loop {
        arm_gic::wfi();
    }
}
