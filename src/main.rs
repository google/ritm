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

mod arch;
mod console;
mod exceptions;
mod logger;
mod pagetable;
mod platform;

use aarch64_paging::paging::PAGE_SIZE;
use aarch64_rt::entry;
use buddy_system_allocator::{Heap, LockedHeap};
use core::arch::naked_asm;
use core::ops::DerefMut;
use log::{LevelFilter, info};
use ritm_device_tree::fdt::Fdt;
use spin::mutex::{SpinMutex, SpinMutexGuard};

use crate::platform::{Platform, PlatformImpl};

const LOG_LEVEL: LevelFilter = LevelFilter::Info;
const BOOT_KERNEL_AT_EL1: bool = false;

const HEAP_SIZE: usize = 40 * PAGE_SIZE;
#[unsafe(no_mangle)]
#[unsafe(link_section = ".heap")]
static HEAP: SpinMutex<[u8; HEAP_SIZE]> = SpinMutex::new([0; HEAP_SIZE]);

#[global_allocator]
static HEAP_ALLOCATOR: LockedHeap<32> = LockedHeap::new();

#[repr(align(4096))]
struct AlignImage<T>(T);

// Payload path here
#[unsafe(link_section = ".payload")]
// static NEXT_IMAGE: AlignImage<[u8; 38373888]> = AlignImage(*include_bytes!(
//     "/usr/local/google/home/mmac/code/common-android16-6.12/common/arch/arm64/boot/Image"
// ));
static NEXT_IMAGE: AlignImage<[u8; 18_815_488]> = AlignImage(*include_bytes!(
    "/usr/local/google/home/mmac/code/linux/arch/arm64/boot/Image"
));

entry!(main);
fn main(x0: u64, x1: u64, x2: u64, x3: u64) -> ! {
    // SAFETY: We only call `PlatformImpl::create` here, once on boot.
    let mut platform = unsafe { PlatformImpl::create() };
    let parts = platform.parts().expect("could not get platform parts");

    let console = console::init(parts.console);
    logger::init(console.shared(), LOG_LEVEL).expect("failed to init logger");

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
    let fdt: Fdt<'_> = unsafe { Fdt::from_raw(fdt_address).expect("fdt_address is not a valid fdt") };
    info!("FDT: {fdt}");

    let new_fdt = platform.modify_dt(fdt);
    let dtb_ptr = new_fdt.data().as_ptr() as u64;

    // SAFETY: We assume there's a valid executable at `NEXT_IMAGE`
    unsafe {
        if BOOT_KERNEL_AT_EL1 {
            run_payload_el1(dtb_ptr, x1, x2, x3)
        } else {
            run_payload_el2(dtb_ptr, x1, x2, x3)
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

fn disable_mmu_and_caches() {
    // Disable MMU and caches
    let mut sctlr = arch::sctlr_el2::read();
    sctlr &= !arch::sctlr_el2::M;
    sctlr &= !arch::sctlr_el2::C;
    sctlr &= !arch::sctlr_el2::I;
    arch::sctlr_el2::write(sctlr);
    arch::isb();
    arch::dsb();

    arch::invalidate_dcache();

    arch::dsb();
    arch::isb();

    // Invalidate I-cache
    arch::ic_iallu();
    arch::tlbi_alle2is();

    // Final synchronization
    arch::dsb_ish();
    arch::isb();
}

#[unsafe(naked)]
unsafe extern "C" fn eret_to_el1(x0: u64, x1: u64, x2: u64, x3: u64) -> ! {
    naked_asm!("eret");
}

/// Run the payload at EL1.
///
/// # Safety
///
/// `NEXT_IMAGE` must point to a valid executable piece of code which never returns.
unsafe fn run_payload_el1(x0: u64, x1: u64, x2: u64, x3: u64) -> ! {
    disable_mmu_and_caches();

    // Setup EL1
    // EL1 is AArch64
    let mut hcr = arch::hcr_el2::read();
    hcr |= arch::hcr_el2::RW;
    hcr |= arch::hcr_el2::TID1;
    hcr &= !arch::hcr_el2::AMO;
    arch::hcr_el2::write(hcr);

    //todo
    arch::cntvoff_el2::write(0);

    // Allow access to timers
    arch::cnthctl_el2::write(3);

    // Setup SPSR_EL2 to enter EL1h
    // Mask debug, SError, IRQ, and FIQ
    let mut spsr = arch::spsr_el2::read();
    spsr |= arch::spsr_el2::MASK_ALL;
    spsr |= arch::spsr_el2::EL1H;
    arch::spsr_el2::write(spsr);

    // Set ELR_EL2 to the kernel entry point
    arch::elr_el2::write(NEXT_IMAGE.0.as_ptr() as u64);

    // Set stack pointer for EL1
    arch::sp_el1::write(arch::sp());

    info!("Exiting to EL1.");

    // SAFETY: This is a call to the hypervisor, which is safe.
    unsafe {
        eret_to_el1(x0, x1, x2, x3);
    }
}

#[unsafe(naked)]
unsafe extern "C" fn jump_to_payload(x0: u64, x1: u64, x2: u64, x3: u64) -> ! {
    naked_asm!(
        "b {next_image}",
        next_image = sym NEXT_IMAGE,
    );
}

/// Run the payload at EL2.
///
/// # Safety
///
/// `NEXT_IMAGE` must point to a valid executable piece of code which never returns.
unsafe fn run_payload_el2(x0: u64, x1: u64, x2: u64, x3: u64) -> ! {
    disable_mmu_and_caches();
    // SAFETY: This is a call to the hypervisor, which is safe.
    unsafe {
        jump_to_payload(x0, x1, x2, x3);
    }
}
