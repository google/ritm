#![no_std]
#![no_main]

use aarch64_rt::{ExceptionHandlers, RegisterStateRef, entry, exception_handlers};
use arm_pl011_uart::{Uart, UniqueMmioPointer};
use core::arch::asm;
use core::fmt::Write;
use core::panic::PanicInfo;
use core::ptr::NonNull;

const UART_BASE: usize = 0x0900_0000;
const RITM_BASE: usize = 0x4000_0000;

exception_handlers!(Exceptions);
entry!(main);

fn main(_arg0: u64, _arg1: u64, _arg2: u64, _arg3: u64) -> ! {
    let mut uart =
        Uart::new(unsafe { UniqueMmioPointer::new(NonNull::new(UART_BASE as *mut _).unwrap()) });
    writeln!(uart, "TEST: Starting isolation test").unwrap();

    writeln!(
        uart,
        "TEST: Attempting to read protected memory at {:#x}",
        RITM_BASE,
    )
    .unwrap();

    // We expect this to trap
    let val = unsafe { core::ptr::read_volatile(RITM_BASE as *const u64) };

    writeln!(uart, "TEST: FAILED: Read successful: {:#x}", val).unwrap();
    loop_forever();
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    let mut uart =
        Uart::new(unsafe { UniqueMmioPointer::new(NonNull::new(UART_BASE as *mut _).unwrap()) });
    writeln!(uart, "TEST: PANIC: {}", info).unwrap();
    loop_forever();
}

struct Exceptions;
impl ExceptionHandlers for Exceptions {
    extern "C" fn sync_current(_register_state: RegisterStateRef) {
        let esr: u64;
        unsafe {
            asm!("mrs {0}, esr_el1", out(reg) esr);
        }

        // Check for Data Abort (EC = 0x25 or 0x24 if injected verbatim)
        let ec = (esr >> 26) & 0x3f;
        if ec == 0x25 || ec == 0x24 {
            let mut uart = Uart::new(unsafe {
                UniqueMmioPointer::new(NonNull::new(UART_BASE as *mut _).unwrap())
            });
            writeln!(
                uart,
                "TEST: Caught expected Data Abort! Isolation test passed.",
            )
            .unwrap();
            loop_forever();
        } else {
            panic!("Unexpected exception: ESR={:#x}", esr);
        }
    }
}

fn loop_forever() -> ! {
    loop {
        unsafe {
            asm!("wfi");
        }
    }
}
