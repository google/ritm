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
use spin::Once;
use spin::mutex::{SpinMutex, SpinMutexGuard};

const UART_BASE: usize = 0x0900_0000;
const RITM_BASE: usize = 0x4000_0000;

exception_handlers!(Exceptions);
entry!(main);

static UART: Once<SpinMutex<Uart>> = Once::new();

fn get_uart() -> SpinMutexGuard<'static, Uart<'static>> {
    UART.call_once(|| {
        SpinMutex::new(Uart::new(unsafe {
            UniqueMmioPointer::new(NonNull::new(UART_BASE as *mut _).unwrap())
        }))
    })
    .lock()
}

fn main(_arg0: u64, _arg1: u64, _arg2: u64, _arg3: u64) -> ! {
    writeln!(get_uart(), "TEST: Starting isolation test").unwrap();

    writeln!(
        get_uart(),
        "TEST: Attempting to read protected memory at {:#x}",
        RITM_BASE,
    )
    .unwrap();

    // We expect this to trap
    let val = unsafe { core::ptr::read_volatile(RITM_BASE as *const u64) };

    writeln!(get_uart(), "TEST: FAILED: Read successful: {:#x}", val).unwrap();
    power_off();
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    writeln!(get_uart(), "TEST: PANIC: {}", info).unwrap();
    power_off();
}

struct Exceptions;
impl ExceptionHandlers for Exceptions {
    extern "C" fn sync_current(_register_state: RegisterStateRef) {
        let esr = read_esr_el1();

        // Check for Data Abort (EC = 0x25 or 0x24 if injected verbatim)
        let ec = esr.ec();
        if ec == 0x25 || ec == 0x24 {
            writeln!(
                get_uart(),
                "TEST: Caught expected Data Abort! Isolation test passed.",
            )
            .unwrap();
            power_off();
        } else {
            panic!("Unexpected exception: ESR={:#x}", esr);
        }
    }
}

fn power_off() -> ! {
    let _ = smccc::psci::cpu_off::<smccc::Hvc>();
    // loop forever if the PSCI call failed
    loop {
        unsafe {
            asm!("wfi");
        }
    }
}
