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
mod logger;
mod pagetable;
mod platform;

mod payload_constants {
    include!(concat!(env!("OUT_DIR"), "/payload_constants.rs"));
}

use aarch64_paging::paging::PAGE_SIZE;
use aarch64_rt::entry;
use buddy_system_allocator::{Heap, LockedHeap};
use core::arch::naked_asm;
use core::ops::DerefMut;
use log::{LevelFilter, info};
use spin::mutex::{SpinMutex, SpinMutexGuard};

use crate::{
    arch::disable_mmu_and_caches,
    platform::{Platform, PlatformImpl},
};

const LOG_LEVEL: LevelFilter = LevelFilter::Info;

const HEAP_SIZE: usize = 40 * PAGE_SIZE;
static HEAP: SpinMutex<[u8; HEAP_SIZE]> = SpinMutex::new([0; HEAP_SIZE]);

#[global_allocator]
static HEAP_ALLOCATOR: LockedHeap<32> = LockedHeap::new();

#[repr(align(0x200000))] // Linux requires 2MB alignment
struct AlignImage<T>(T);

static NEXT_IMAGE: AlignImage<[u8; payload_constants::PAYLOAD_SIZE]> =
    AlignImage(*payload_constants::PAYLOAD_DATA);

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

    // SAFETY: We assume that the payload at `NEXT_IMAGE` is a valid executable piece of code.
    unsafe {
        run_payload_el2(x0, x1, x2, x3);
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
        disable_mmu_and_caches = sym disable_mmu_and_caches,
        next_image = sym NEXT_IMAGE,
    );
}
