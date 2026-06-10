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

mod platform_constants {
    include!(concat!(env!("OUT_DIR"), "/platform_constants.rs"));
}

const UART_BASE: usize = 0x0900_0000;
const FILTERED_MMIO_BASE: usize = 0x0f00_0000;
const FILTERED_MMIO_READ_VALUE: u64 = 0xfeed_face_cafe_beef;
const FILTERED_MMIO_WRITE_VALUE: u64 = 0x1234_5678_9abc_def0;
const FILTERED_MMIO_COUNTER_OFFSET: usize = 16;
const FILTERED_MMIO_OFFSET_VALUES: [(usize, u64); 3] = [
    (24, 0x1111_2222_3333_4444),
    (32, 0x5555_6666_7777_8888),
    (40, 0x9999_aaaa_bbbb_cccc),
];
const RITM_BASE: usize = platform_constants::RITM_IMAGE_ADDRESS;

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
    test_mmio_handler();
    test_mmio_handler_counter();
    test_mmio_handler_offsets();
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

fn test_mmio_handler() {
    writeln!(get_uart(), "TEST: Attempting filtered MMIO access...").unwrap();
    let before = TRAP_COUNT.load(Ordering::SeqCst);

    let mut value: u64;
    unsafe {
        asm!(
            "ldr x0, [x1]",
            in("x1") FILTERED_MMIO_BASE,
            lateout("x0") value,
            options(nostack, readonly),
        );
    }
    if value != FILTERED_MMIO_READ_VALUE {
        panic!("Filtered MMIO read returned {:#x}", value);
    }

    unsafe {
        asm!(
            "str x0, [x1]",
            in("x0") FILTERED_MMIO_WRITE_VALUE,
            in("x1") FILTERED_MMIO_BASE + 8,
            options(nostack),
        );
    }

    let after = TRAP_COUNT.load(Ordering::SeqCst);
    if after != before {
        panic!("Filtered MMIO access unexpectedly faulted");
    }
    writeln!(get_uart(), "TEST: Filtered MMIO access succeeded").unwrap();
}

fn test_mmio_handler_counter() {
    writeln!(
        get_uart(),
        "TEST: Attempting filtered MMIO counter reads..."
    )
    .unwrap();
    let before = TRAP_COUNT.load(Ordering::SeqCst);

    for expected in 0..4 {
        let mut value: u64;
        unsafe {
            asm!(
                "ldr x0, [x1]",
                in("x1") FILTERED_MMIO_BASE + FILTERED_MMIO_COUNTER_OFFSET,
                lateout("x0") value,
                options(nostack, readonly),
            );
        }
        if value != expected {
            panic!(
                "Filtered MMIO counter read returned {:#x}, expected {:#x}",
                value, expected
            );
        }
    }

    let after = TRAP_COUNT.load(Ordering::SeqCst);
    if after != before {
        panic!("Filtered MMIO counter access unexpectedly faulted");
    }
    writeln!(get_uart(), "TEST: Filtered MMIO counter reads succeeded").unwrap();
}

fn test_mmio_handler_offsets() {
    writeln!(get_uart(), "TEST: Attempting filtered MMIO offset reads...").unwrap();
    let before = TRAP_COUNT.load(Ordering::SeqCst);

    for (offset, expected) in FILTERED_MMIO_OFFSET_VALUES {
        let mut value: u64;
        unsafe {
            asm!(
                "ldr x0, [x1]",
                in("x1") FILTERED_MMIO_BASE + offset,
                lateout("x0") value,
                options(nostack, readonly),
            );
        }
        if value != expected {
            panic!(
                "Filtered MMIO read at offset {:#x} returned {:#x}, expected {:#x}",
                offset, value, expected
            );
        }
    }

    let after = TRAP_COUNT.load(Ordering::SeqCst);
    if after != before {
        panic!("Filtered MMIO offset access unexpectedly faulted");
    }
    writeln!(get_uart(), "TEST: Filtered MMIO offset reads succeeded").unwrap();
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
