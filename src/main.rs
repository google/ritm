// Copyright 2025 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![no_main]
#![no_std]

extern crate alloc;

mod arch;
mod console;
mod exceptions;
mod hypervisor;
mod logger;
mod pagetable;
mod platform;
mod simple_map;

mod payload_constants {
    include!(concat!(env!("OUT_DIR"), "/payload_constants.rs"));
}

use aarch64_paging::paging::PAGE_SIZE;
use aarch64_rt::{entry, exception_handlers};
use buddy_system_allocator::{Heap, LockedHeap};
use core::arch::naked_asm;
use core::ops::DerefMut;
use dtoolkit::fdt::Fdt;
use log::{LevelFilter, info};
use spin::mutex::{SpinMutex, SpinMutexGuard};

use crate::{
    exceptions::Exceptions,
    platform::{BootMode, Platform, PlatformImpl},
};

const LOG_LEVEL: LevelFilter = LevelFilter::Info;

const HEAP_SIZE: usize = 40 * PAGE_SIZE;
static HEAP: SpinMutex<[u8; HEAP_SIZE]> = SpinMutex::new([0; HEAP_SIZE]);

#[global_allocator]
static HEAP_ALLOCATOR: LockedHeap<32> = LockedHeap::new();

#[repr(align(0x200000))] // Linux requires 2MB alignment
struct AlignImage<T>(T);

#[unsafe(link_section = ".payload")]
static NEXT_IMAGE: AlignImage<[u8; payload_constants::PAYLOAD_SIZE]> =
    AlignImage(*payload_constants::PAYLOAD_DATA);

exception_handlers!(Exceptions);
entry!(main);

fn main(x0: u64, x1: u64, x2: u64, x3: u64) -> ! {
    // SAFETY: We only call `PlatformImpl::create` here, once on boot.
    let mut platform = unsafe { PlatformImpl::create() };
    let parts = platform.parts().expect("could not get platform parts");

    let console = console::init(parts.console);
    logger::init(console, LOG_LEVEL).expect("failed to init logger");

    info!("starting ritm");
    info!("main({x0:#x}, {x1:#x}, {x2:#x}, {x3:#x})");

    // Give the allocator some memory to allocate.
    add_to_heap(
        HEAP_ALLOCATOR.lock().deref_mut(),
        SpinMutexGuard::leak(HEAP.try_lock().expect("failed to lock heap")).as_mut_slice(),
    );

    let fdt_address = x0 as *const u8;
    // SAFETY: We trust that the FDT pointer we were given is valid, and this is the only time we
    // use it.
    // SAFETY: fdt_address is a valid pointer to a device tree.
    let fdt: Fdt<'_> =
        unsafe { Fdt::from_raw(fdt_address).expect("fdt_address is not a valid fdt") };

    let new_fdt = platform.modify_dt(fdt);
    let dtb_ptr = new_fdt.data().as_ptr() as u64;

    let boot_mode = platform.boot_mode();
    info!("Booting in {boot_mode:?}");

    // SAFETY: We assume there's a valid executable at `NEXT_IMAGE`.
    unsafe {
        match platform.boot_mode() {
            BootMode::El1 => run_payload_el1(dtb_ptr, x1, x2, x3),
            BootMode::El2 => run_payload_el2(dtb_ptr, x1, x2, x3),
        }
    }
}

/// Adds the given memory range to the given heap.
fn add_to_heap<const ORDER: usize>(heap: &mut Heap<ORDER>, range: &'static mut [u8]) {
    // SAFETY: The range we pass is valid because it comes from a mutable static reference, which it
    // effectively takes ownership of.
    unsafe {
        heap.init(range.as_mut_ptr() as usize, range.len());
    }
}

/// Run the payload at EL2.
///
/// # Safety
///
/// `NEXT_IMAGE` must point to a valid executable piece of code which never returns.
#[unsafe(naked)]
unsafe extern "C" fn run_payload_el2(x0: u64, x1: u64, x2: u64, x3: u64) -> ! {
    naked_asm!(
        "mov x19, x0",
        "mov x20, x1",
        "mov x21, x2",
        "mov x22, x3",
        "bl {disable_mmu_and_caches}",
        "mov x0, x19",
        "mov x1, x20",
        "mov x2, x21",
        "mov x3, x22",
        "b {next_image}",
        disable_mmu_and_caches = sym arch::disable_mmu_and_caches,
        next_image = sym NEXT_IMAGE,
    );
}

/// Run the payload at EL1.
///
/// # Safety
///
/// `NEXT_IMAGE` must point to a valid executable piece of code which never returns.
unsafe fn run_payload_el1(x0: u64, x1: u64, x2: u64, x3: u64) -> ! {
    // SAFETY: The caller guarantees that `NEXT_IMAGE` points to a valid executable piece of code which never returns.
    unsafe {
        hypervisor::entry_point_el1(x0, x1, x2, x3, &raw const NEXT_IMAGE.0 as u64);
    }
}
