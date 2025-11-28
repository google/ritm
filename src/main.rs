// Copyright 2025 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![no_main]
#![no_std]
#![deny(clippy::undocumented_unsafe_blocks)]
#![deny(unsafe_op_in_unsafe_fn)]

extern crate alloc;

mod console;
mod exceptions;
mod logger;
mod pagetable;
mod platform;

use aarch64_paging::paging::PAGE_SIZE;
use aarch64_rt::entry;
use buddy_system_allocator::{Heap, LockedHeap};
use core::ops::DerefMut;
use log::{LevelFilter, info};
use spin::mutex::{SpinMutex, SpinMutexGuard};

use crate::platform::{Platform, PlatformImpl};

const LOG_LEVEL: LevelFilter = LevelFilter::Info;

const HEAP_SIZE: usize = 40 * PAGE_SIZE;
static HEAP: SpinMutex<[u8; HEAP_SIZE]> = SpinMutex::new([0; HEAP_SIZE]);

#[global_allocator]
static HEAP_ALLOCATOR: LockedHeap<32> = LockedHeap::new();

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

    todo!();
}

/// Adds the given memory range to the given heap.
fn add_to_heap<const ORDER: usize>(heap: &mut Heap<ORDER>, range: &'static mut [u8]) {
    // SAFETY: The range we pass is valid because it comes from a mutable static reference, which it
    // effectively takes ownership of.
    unsafe {
        heap.init(range.as_mut_ptr() as usize, range.len());
    }
}
