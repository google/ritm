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
mod logger;
mod pagetable;
mod platform;

use aarch64_rt::entry;
use buddy_system_allocator::LockedHeap;
use core::arch::naked_asm;
use log::{LevelFilter, info};
use flat_device_tree::Fdt;

use crate::platform::{Platform, PlatformImpl};

const LOG_LEVEL: LevelFilter = LevelFilter::Info;
const BOOT_KERNEL_AT_EL1: bool = false;

#[global_allocator]
static HEAP_ALLOCATOR: LockedHeap<32> = LockedHeap::new();

#[repr(align(0x200000))]
struct AlignImage<T>(T);

// Payload path here
static NEXT_IMAGE: AlignImage<[u8; 38373888]> = AlignImage(*include_bytes!(
    "/usr/local/google/home/mmac/code/common-android16-6.12/common/arch/arm64/boot/Image"
));

entry!(main);
fn main(x0: u64, x1: u64, x2: u64, x3: u64) -> ! {
    // SAFETY: We only call `PlatformImpl::create` here, once on boot.
    let mut platform = unsafe { PlatformImpl::create() };
    let parts = platform.parts().unwrap();

    let console = console::init(parts.console);
    logger::init(console.shared(), LOG_LEVEL).unwrap();

    info!("starting ritm");
    info!("main({x0:#x}, {x1:#x}, {x2:#x}, {x3:#x})");

    let fdt_address = x0 as *const u8;
    // SAFETY: We trust that the FDT pointer we were given is valid, and this is the only time we
    // use it.
    let fdt = unsafe { Fdt::from_ptr(fdt_address).unwrap() };
    info!("FDT size: {} bytes", fdt.total_size());
    info!("FDT: {fdt:?}");

    // SAFETY: We assume there's a valid executable at `NEXT_IMAGE`
    unsafe {
        if BOOT_KERNEL_AT_EL1 {
            run_payload_el1(x0, x1, x2, x3)
        } else {
            run_payload_el2(x0, x1, x2, x3)
        }
    }
}

#[unsafe(naked)]
#[rustfmt::skip]
unsafe extern "C" fn run_payload_el1(x0: u64, x1: u64, x2: u64, x3: u64) -> ! {
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

        // Setup EL1
        // EL1 is AArch64
        "mov x5, #(1 << 31)",
        "orr x5, x5, #(1 << 19)",
        "orr x5, x5, #(1 << 4)",
        "msr hcr_el2, x5",

        // Allow access to timers
        "mov x5, #3",
        "msr cnthctl_el2, x5",

        // Setup SPSR_EL2 to enter EL1h
        // Mask debug, SError, IRQ, and FIQ
        "mov x5, #(0b1111 << 6)",
        // EL1h
        "mov x6, #5",
        "orr x5, x5, x6",
        "msr spsr_el2, x5",

        // Set ELR_EL2 to the kernel entry point
        "ldr x5, ={next_image}",
        "msr elr_el2, x5",

        // Set stack pointer for EL1
        "mov x5, sp",
        "msr sp_el1, x5",

        "eret",
        next_image = sym crate::NEXT_IMAGE
    )
}

#[unsafe(naked)]
#[rustfmt::skip]
unsafe extern "C" fn run_payload_el2(x0: u64, x1: u64, x2: u64, x3: u64) -> ! {
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
