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
mod hvc_response;
mod hypervisor;
mod logger;
mod pagetable;
mod platform;
mod simple_map;

include!(concat!(env!("OUT_DIR"), "/payload.rs"));

use aarch64_paging::paging::PAGE_SIZE;
use aarch64_rt::{entry, exception_handlers};
use arm_sysregs::read_currentel;
use buddy_system_allocator::{Heap, LockedHeap};
use core::alloc::Layout;
use core::arch::naked_asm;
use core::ops::DerefMut;
use dtoolkit::fdt::Fdt;
use log::{LevelFilter, info};
use spin::mutex::{SpinMutex, SpinMutexGuard};

use crate::arch::disable_mmu_and_caches;
use crate::{
    exceptions::Exceptions,
    platform::{BootMode, Platform, PlatformImpl},
};

const LOG_LEVEL: LevelFilter = LevelFilter::Info;

const HEAP_SIZE: usize = 40 * PAGE_SIZE;
static HEAP: SpinMutex<[u8; HEAP_SIZE]> = SpinMutex::new([0; HEAP_SIZE]);

static SHARED_HEAP: SpinMutex<[u8; PlatformImpl::SHARED_HEAP_SIZE]> =
    SpinMutex::new([0; PlatformImpl::SHARED_HEAP_SIZE]);

#[global_allocator]
static HEAP_ALLOCATOR: LockedHeap<32> = LockedHeap::new();

/// Heap allocator for data that needs to be shared between RITM and the guest running in EL1.
pub static SHARED_HEAP_ALLOCATOR: LockedHeap<32> = LockedHeap::new();

exception_handlers!(Exceptions);
entry!(main);

fn main(x0: u64, x1: u64, x2: u64, x3: u64) -> ! {
    // SAFETY: We only call `PlatformImpl::create` here, once on boot.
    let mut platform = unsafe { PlatformImpl::create() };
    let parts = platform.parts().expect("could not get platform parts");

    let console = console::init(parts.console);
    logger::init(console, LOG_LEVEL).expect("failed to init logger");

    info!("starting ritm");
    info!("current exception level: EL{}", read_currentel().el());
    info!("main({x0:#x}, {x1:#x}, {x2:#x}, {x3:#x})");

    // Give the allocator some memory to allocate.
    add_to_heap(
        HEAP_ALLOCATOR.lock().deref_mut(),
        SpinMutexGuard::leak(HEAP.try_lock().expect("failed to lock heap")).as_mut_slice(),
    );

    add_to_heap(
        SHARED_HEAP_ALLOCATOR.lock().deref_mut(),
        SpinMutexGuard::leak(SHARED_HEAP.try_lock().expect("failed to lock shared heap"))
            .as_mut_slice(),
    );

    let fdt_address = platform.fdt_address(x0) as *const u8;
    // SAFETY: We trust that the FDT pointer we were given is valid, and this is the only time we
    // use it.
    let fdt: Fdt<'_> =
        unsafe { Fdt::from_raw(fdt_address).expect("fdt_address is not a valid fdt") };

    let new_fdt = platform.modify_dt(fdt);
    let dtb_ptr = new_fdt.data().as_ptr() as u64;

    let boot_mode = platform.boot_mode(&new_fdt);
    let payload_address = platform.payload_address();
    info!("Booting in {boot_mode:?}");
    info!("Payload address: {payload_address:#x}");

    // SAFETY: We assume there's a valid executable at the platform payload address.
    unsafe {
        match boot_mode {
            BootMode::El1 => run_payload_el1(payload_address, dtb_ptr, x1, x2, x3),
            BootMode::El2 => run_payload_el2(payload_address, dtb_ptr, x1, x2, x3),
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
/// `entry_point` must point to a valid executable piece of code which never returns.
#[unsafe(naked)]
unsafe extern "C" fn run_payload_el2(entry_point: u64, x0: u64, x1: u64, x2: u64, x3: u64) -> ! {
    naked_asm!(
        "mov x19, x0",
        "mov x20, x1",
        "mov x21, x2",
        "mov x22, x3",
        "mov x23, x4",
        "bl {disable_mmu_and_caches}",
        "mov x0, x20",
        "mov x1, x21",
        "mov x2, x22",
        "mov x3, x23",
        "br x19",
        disable_mmu_and_caches = sym disable_mmu_and_caches,
    );
}

/// Run the payload at EL1.
///
/// # Safety
///
/// `entry_point` must point to a valid executable piece of code which never returns.
unsafe fn run_payload_el1(entry_point: u64, x0: u64, x1: u64, x2: u64, x3: u64) -> ! {
    // SAFETY: The caller guarantees that `entry_point` points to a valid executable piece of code which never returns.
    unsafe {
        hypervisor::entry_point_el1(x0, x1, x2, x3, entry_point);
    }
}

/// Allocates a buffer from the shared heap.
///
/// # Panics
///
/// Panics if the requested size is invalid or if the allocation fails.
pub fn shared_alloc(layout: Layout) -> &'static mut [u8] {
    let ptr = SHARED_HEAP_ALLOCATOR
        .lock()
        .alloc(layout)
        .expect("failed to allocate from shared heap");
    // SAFETY: The pointer is valid and represents the requested size.
    unsafe { core::slice::from_raw_parts_mut(ptr.as_ptr(), layout.size()) }
}
