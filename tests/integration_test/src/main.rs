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
use arm_pl011_uart::{Uart, UniqueMmioPointer};
use arm_sysregs::read_esr_el1;
use core::arch::asm;
use core::fmt::Write;
use core::panic::PanicInfo;
use core::ptr::NonNull;
use core::sync::atomic::{AtomicUsize, Ordering};
use smccc::{Hvc, hvc64, psci};
use spin::Once;
use spin::mutex::{SpinMutex, SpinMutexGuard};

const UART_BASE: usize = 0x0900_0000;
const RITM_BASE: usize = 0x4000_0000;

exception_handlers!(Exceptions);
entry!(main);

static UART: Once<SpinMutex<Uart>> = Once::new();
static TRAP_COUNT: AtomicUsize = AtomicUsize::new(0);

fn get_uart() -> SpinMutexGuard<'static, Uart<'static>> {
    UART.call_once(|| {
        SpinMutex::new(Uart::new(unsafe {
            UniqueMmioPointer::new(NonNull::new(UART_BASE as *mut _).unwrap())
        }))
    })
    .lock()
}

fn main(_arg0: u64, _arg1: u64, _arg2: u64, _arg3: u64) -> ! {
    writeln!(get_uart(), "TEST: Starting integration tests").unwrap();

    test_psci();
    test_dummy_hvc();
    test_unknown_hvc();
    test_memory_isolation();

    writeln!(get_uart(), "TEST: All tests passed!").unwrap();
    power_off();
}

fn test_psci() {
    let version = psci::version::<Hvc>();
    writeln!(get_uart(), "TEST: PSCI version: {:?}", version).unwrap();
    if version.is_err() {
        panic!("PSCI version call failed");
    }
}

fn test_dummy_hvc() {
    writeln!(get_uart(), "TEST: Attempting a dummy HVC call...").unwrap();
    let [res, ..] = hvc64(0xFF00_0000, [0; 17]);

    if res != 0x1234_5678_9ABC_DEF0 {
        panic!("Dummy HVC failed, x0={:#x}", res);
    }
    writeln!(get_uart(), "TEST: Dummy HVC succeeded").unwrap();
}

fn test_unknown_hvc() {
    writeln!(get_uart(), "TEST: Attempting an unknown HVC call...").unwrap();
    let [res, ..] = hvc64(0xFF00_0001, [0; 17]);
    writeln!(get_uart(), "TEST: Unknown HVC handled (returned {res:#x})",).unwrap();
}

fn test_memory_isolation() {
    writeln!(
        get_uart(),
        "TEST: Attempting to read protected memory at {:#x}",
        RITM_BASE,
    )
    .unwrap();

    let before = TRAP_COUNT.load(Ordering::SeqCst);

    // We expect this to trap
    let val = unsafe { core::ptr::read_volatile(RITM_BASE as *const u64) };

    let after = TRAP_COUNT.load(Ordering::SeqCst);
    if after != before + 1 {
        // If we reach here, the test failed
        panic!(
            "Isolation test failed! Read value {:#x} from protected memory without trap",
            val
        );
    }

    writeln!(
        get_uart(),
        "TEST: Caught expected Data Abort! Isolation test passed.",
    )
    .unwrap();
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    let _ = writeln!(get_uart(), "TEST: PANIC: {}", info);
    power_off();
}

struct Exceptions;
impl ExceptionHandlers for Exceptions {
    extern "C" fn sync_current(mut register_state: RegisterStateRef) {
        let esr = read_esr_el1();

        // Check for Data Abort (EC = 0x25 or 0x24 if injected verbatim)
        let ec = esr.ec();
        if ec == 0x25 || ec == 0x24 {
            TRAP_COUNT.fetch_add(1, Ordering::SeqCst);
            unsafe {
                let regs = register_state.get_mut();
                regs.elr += 4;
            }
        } else {
            panic!("Unexpected exception: ESR={:#x}", esr);
        }
    }
}

fn power_off() -> ! {
    let _ = psci::cpu_off::<Hvc>();
    // loop forever if the PSCI call failed
    loop {
        unsafe {
            asm!("wfi");
        }
    }
}
