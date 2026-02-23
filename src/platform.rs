// Copyright 2025 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#[cfg(platform = "qemu")]
mod qemu;

use aarch64_paging::descriptor::Stage2Attributes;
use aarch64_paging::idmap::IdMap;
use aarch64_paging::paging::PAGE_SIZE;
use dtoolkit::fdt::Fdt;
use embedded_io::{Write, WriteReady};
#[cfg(platform = "qemu")]
pub use qemu::Qemu as PlatformImpl;

pub type ConsoleImpl = <PlatformImpl as Platform>::Console;

/// Platform-specific code.
pub trait Platform {
    type Console: Send + Write + WriteReady;

    /// The maximum number of cores supported by the platform.
    const MAX_CORES: usize;

    /// The size of the heap shared between the host and the guest (e.g. to provide
    /// the modified DTB).
    const SHARED_HEAP_SIZE: usize = 16 * PAGE_SIZE;

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

    /// Returns the intended boot mode for current device configuration.
    fn boot_mode(&self) -> BootMode;

    /// Modify the Device Tree if needed to adjust for the platform's needs. That might include
    /// reserving memory for RITM, or changing the PSCI method.
    fn modify_dt(&self, fdt: Fdt<'static>) -> Fdt<'static> {
        fdt
    }

    /// Create stage-2 page table for use by the guest for use when booting the payload at EL1.
    ///
    /// The page table should typically unmap the part of the memory where RITM resides, so that
    /// the guest cannot interact with it in any way.
    fn make_stage2_pagetable() -> IdMap<Stage2Attributes>;
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[allow(unused)]
pub enum BootMode {
    /// Booting in EL1.
    El1,
    /// Booting in EL2.
    El2,
}

/// The drivers provided by each platform.
pub struct PlatformParts<Console> {
    /// The primary console.
    pub console: Console,
}
