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

mod console;
mod exceptions;

use aarch64_paging::paging::Attributes;
use aarch64_rt::{InitialPagetable, entry, initial_pagetable};
use arm_pl011_uart::{PL011Registers, Uart, UniqueMmioPointer};
use buddy_system_allocator::LockedHeap;
use core::arch::naked_asm;
use core::ptr::NonNull;
use embedded_io::Write;

/// Base address of the primary PL011 UART.
const PL011_BASE_ADDRESS: NonNull<PL011Registers> = NonNull::new(0x900_0000 as _).unwrap();

/// Attributes to use for device memory in the initial identity map.
const DEVICE_ATTRIBUTES: Attributes = Attributes::VALID
    .union(Attributes::ATTRIBUTE_INDEX_0)
    .union(Attributes::ACCESSED)
    .union(Attributes::UXN);

/// Attributes to use for normal memory in the initial identity map.
const MEMORY_ATTRIBUTES: Attributes = Attributes::VALID
    .union(Attributes::ATTRIBUTE_INDEX_1)
    .union(Attributes::INNER_SHAREABLE)
    .union(Attributes::ACCESSED)
    .union(Attributes::NON_GLOBAL);

initial_pagetable!({
    let mut idmap = [0; 512];
    // 1 GiB of device memory.
    idmap[0] = DEVICE_ATTRIBUTES.bits();
    // 1 GiB of normal memory.
    idmap[1] = MEMORY_ATTRIBUTES.bits() | 0x40000000;
    // Another 1 GiB of device memory starting at 256 GiB.
    idmap[256] = DEVICE_ATTRIBUTES.bits() | 0x4000000000;
    InitialPagetable(idmap)
});

#[global_allocator]
static HEAP_ALLOCATOR: LockedHeap<32> = LockedHeap::new();

#[repr(align(2097152))]
struct AlignImage<T>(T);

// Payload path here
static NEXT_IMAGE: AlignImage<[u8; 38373888]> = AlignImage(*include_bytes!(
    "/usr/local/google/home/mmac/code/common-android16-6.12/common/arch/arm64/boot/Image"
));

entry!(main);
fn main(x0: u64, x1: u64, x2: u64, x3: u64) -> ! {
    // SAFETY: `PL011_BASE_ADDRESS` is the base address of a PL011 device, and
    // nothing else accesses that address range.
    let mut uart = unsafe { Uart::new(UniqueMmioPointer::new(PL011_BASE_ADDRESS)) };

    let mut console = console::init(uart);
    let mut shared = console.shared();

    writeln!(shared, "main({x0:#x}, {x1:#x}, {x2:#x}, {x3:#x})").unwrap();
    writeln!(shared, "### ritm running! ###").unwrap();

    // SAFETY: We assume there's a valid executable at `NEXT_IMAGE`
    unsafe { run_payload(x0, x1, x2, x3) }
}

#[unsafe(naked)]
unsafe extern "C" fn run_payload(x0: u64, x1: u64, x2: u64, x3: u64) -> ! {
    naked_asm!(
        "b {next_image}",
        next_image = sym crate::NEXT_IMAGE,
    )
}
