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
    let uart = unsafe { Uart::new(UniqueMmioPointer::new(PL011_BASE_ADDRESS)) };

    let console = console::init(uart);
    let mut shared = console.shared();

    writeln!(shared, "main({x0:#x}, {x1:#x}, {x2:#x}, {x3:#x})").unwrap();
    writeln!(shared, "### ritm running! ###").unwrap();

    // SAFETY: We assume there's a valid executable at `NEXT_IMAGE`
    unsafe { run_payload(x0, x1, x2, x3) }
}

#[unsafe(naked)]
#[rustfmt::skip]
unsafe extern "C" fn run_payload(x0: u64, x1: u64, x2: u64, x3: u64) -> ! {
    naked_asm!(
        // Disable MMU and caches
        "mrs x5, sctlr_el2",
        "bic x5, x5, #(1 << 0)",   // MMU disable
        "bic x5, x5, #(1 << 2)",   // Data Cache disable
        "bic x5, x5, #(1 << 12)",  // Instruction Cache Disable
        "msr sctlr_el2, x5",
        "isb",
        "dsb sy",

        // Invalidate D-cache by set/way to the point of coherency
        "mrs x5, clidr_el1",      // x5 = CLIDR
        "and x6, x5, #0x7000000", // x6 = LoC
        "lsr x6, x6, #23",
        "cbz x6, 2f",             // if LoC is 0, then no need to clean
        "mov x10, #0",            // x10 = cache level * 2

        // loop over cache levels
        "1:",
            "add x12, x10, x10, lsr #1", // x12 = level * 3
            "lsr x11, x5, x12",
            "and x11, x11, #7",          // x11 = cache type
            "cmp x11, #2",               // is it a data or unified cache?
            "b.lt 3f",                   // if not, skip to next level
            "msr csselr_el1, x10",       // select cache level
            "isb",                       // sync change of csselr
            "mrs x11, ccsidr_el1",       // x11 = ccsidr
            "and x12, x11, #7",          // x12 = log2(line size) - 4
            "add x12, x12, #4",          // x12 = log2(line size)
            "and x13, x11, #0x3ff",      // x13 = (number of ways - 1)
            "and x14, x11, #0x7fff000",  // x14 = (number of sets - 1)
            "lsr x14, x14, #13",
            "clz w15, w13",              // w15 = 31 - log2(ways)
            // loop over ways
            "4:",
                "mov x9, x14",           // x9 = set number
                // loop over sets
                "5:",
                    "lsl x7, x13, x15",
                    "lsl x8, x9, x12",
                    "orr x11, x10, x7",  // x11 = (level << 1) | (way << ...)
                    "orr x11, x11, x8",  // x11 |= (set << log2(line size))
                    "dc isw, x11",       // invalidate by set/way
                    "subs x9, x9, #1",
                    "b.ge 5b",
                    "subs x13, x13, #1",
                    "b.ge 4b",
            "3:",
                "add x10, x10, #2",      // next cache level
                "cmp x6, x10",
                "b.gt 1b",
    "2:",
        "dsb sy",
        "isb",

        // Invalidate I-cache
        "ic iallu",
        "tlbi alle2is",

        // Final synchronization
        "dsb ish",
        "isb",

        // Jump to payload
        "b {next_image}",
        next_image = sym crate::NEXT_IMAGE,
    )
}
