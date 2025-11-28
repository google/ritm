// Copyright 2025 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use crate::platform::ConsoleImpl;
use core::panic::PanicInfo;
use embedded_io::{ErrorType, Write};
use percore::{ExceptionLock, exception_free};
use smccc::{Smc, psci::system_off};
use spin::{Once, mutex::SpinMutex};

static CONSOLE: Once<Console<ConsoleImpl>> = Once::new();

/// A console guarded by a spin mutex so that it may be shared between threads.
pub struct Console<T: Send> {
    pub console: ExceptionLock<SpinMutex<T>>,
}

impl<T: ErrorType + Send> ErrorType for &Console<T> {
    type Error = T::Error;
}

impl<T: ErrorType + Send + Write> Write for &Console<T> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        exception_free(|token| self.console.borrow(token).lock().write(buf))
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        exception_free(|token| self.console.borrow(token).lock().flush())
    }
}

impl<T: ErrorType + Send + 'static> ErrorType for Console<T> {
    type Error = T::Error;
}

/// Initialises the shared console.
pub fn init(console: ConsoleImpl) -> &'static Console<ConsoleImpl> {
    CONSOLE.call_once(|| Console {
        console: ExceptionLock::new(SpinMutex::new(console)),
    })
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    if let Some(console) = CONSOLE.get() {
        exception_free(|token| {
            // Ignore any errors writing to the console, to avoid panicking recursively.
            let _ = writeln!(console.console.borrow(token).lock(), "{info}");
        });
    }
    system_off::<Smc>().expect("system_off failed");

    loop {}
}
