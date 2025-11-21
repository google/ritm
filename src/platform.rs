// Copyright 2025 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#[cfg(platform = "qemu")]
mod qemu;

use embedded_io::{Read, ReadReady, Write, WriteReady};
#[cfg(platform = "qemu")]
pub use qemu::Qemu as PlatformImpl;

pub type ConsoleImpl = <PlatformImpl as Platform>::Console;

/// Platform-specific code.
pub trait Platform {
    type Console: Read + ReadReady + Send + Write + WriteReady;

    /// Creates an instance of the platform.
    ///
    /// # Safety
    ///
    /// This method must only be called once. Calling it multiple times would result in unsound
    /// mutable aliasing.
    unsafe fn create() -> Self;

    /// Returns the drivers provided by the platform.
    ///
    /// This should return `Some` the first time it is called, but may return `None` on subsequent
    /// calls.
    fn parts(&mut self) -> Option<PlatformParts<Self::Console>>;
}

/// The drivers provided by each platform.
pub struct PlatformParts<Console> {
    /// The primary console.
    pub console: Console,
}
