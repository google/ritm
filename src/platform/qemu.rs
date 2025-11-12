// Copyright 2025 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/// The QEMU aarch64 virt platform.
use super::{Platform, PlatformParts};
use crate::pagetable::{DEVICE_ATTRIBUTES, MEMORY_ATTRIBUTES};
use aarch64_rt::InitialPagetable;
use arm_pl011_uart::{Interrupts, PL011Registers, Uart, UniqueMmioPointer};
use core::ptr::NonNull;
use log::info;
use ritm_device_tree::{
    fdt::Fdt,
    model::{DeviceTree, DeviceTreeNode, DeviceTreeProperty},
};

/// Base address of the first PL011 UART.
const UART_BASE_ADDRESS: *mut PL011Registers = 0x900_0000 as _;
pub struct Qemu {
    parts: Option<PlatformParts<Uart<'static>>>,
}

impl Qemu {
    /// Returns the initial hard-coded page table to use before the Rust code starts.
    pub const fn initial_idmap() -> InitialPagetable {
        let mut idmap = [0; 512];
        // 1 GiB of device memory.
        idmap[0] = DEVICE_ATTRIBUTES.bits();
        // 1 GiB of normal memory.
        idmap[1] = MEMORY_ATTRIBUTES.bits() | 0x4000_0000;
        // 1 GiB of DRAM.
        idmap[2] = DEVICE_ATTRIBUTES.bits() | 0x8000_0000;
        InitialPagetable(idmap)
    }
}

impl Platform for Qemu {
    type Console = Uart<'static>;

    unsafe fn create() -> Self {
        let mut uart = Uart::new(
            // SAFETY: UART_BASE_ADDRESS is valid and mapped, and `create` is only called once so
            // there are no aliases
            // SAFETY: The address is a constant and thus not null.
            unsafe { UniqueMmioPointer::new(NonNull::new(UART_BASE_ADDRESS).expect("UART_BASE_ADDRESS should not be null")) },
        );
        uart.set_interrupt_masks(Interrupts::RXI);
        Self {
            parts: Some(PlatformParts { console: uart }),
        }
    }

    fn parts(&mut self) -> Option<PlatformParts<Uart<'static>>> {
        self.parts.take()
    }

    fn modify_dt(&self, fdt: Fdt<'static>) -> Fdt<'static> {
        let mut dt = DeviceTree::from_fdt(&fdt).expect("expected FDT to be valid");
        // let mut res = alloc::vec::Vec::<u8>::new();
        // res.extend_from_slice(&0x40080000u64.to_be_bytes());
        // res.extend_from_slice(&0x4000000u64.to_be_bytes()); // 64 MB
        // dt.root_mut().add_child(
        //     DeviceTreeNode::builder("reserved-memory")
        //         .property(DeviceTreeProperty::new(
        //             "#address-cells",
        //             &0x02u32.to_be_bytes(),
        //         ))
        //         .property(DeviceTreeProperty::new(
        //             "#size-cells",
        //             &0x02u32.to_be_bytes(),
        //         ))
        //         .property(DeviceTreeProperty::new("ranges", &[]))
        //         .child(
        //             DeviceTreeNode::builder("reservation-ritm@40080000")
        //                 .property(DeviceTreeProperty::new("reg", res))
        //                 .property(DeviceTreeProperty::new("no-map", &[]))
        //                 .build(),
        //         )
        //         .build(),
        // );

        let mut res = alloc::vec::Vec::<u8>::new();
        // res.extend_from_slice(&0x44080000u64.to_be_bytes());
        res.extend_from_slice(&0x4080_0000u64.to_be_bytes());
        res.extend_from_slice(&(120u64 * 1024 * 1024).to_be_bytes()); // 64 MB

        // res.extend_from_slice(&0x40000000u64.to_be_bytes());
        // res.extend_from_slice(&0x8000000u64.to_be_bytes()); // 64 MB

        dt.root_mut().remove_child("memory@40000000").expect("memory node not found");
        dt.root_mut().add_child(
            DeviceTreeNode::builder("memory@40800000")
                .property(DeviceTreeProperty::new("reg", res))
                .property(DeviceTreeProperty::new("device_type", b"memory\0"))
                .build(),
        );
        // dt.root_mut().child_mut("psci").unwrap().property_mut("method").unwrap().set_value(b"hvc\0");

        let new_dtb = dt.to_dtb().leak();
        let fdt_address = new_dtb.as_ptr();
        // SAFETY: fdt_address is a valid pointer to a device tree.
        let fdt: Fdt<'_> = unsafe { Fdt::from_raw(fdt_address).expect("fdt_address is not a valid fdt") };
        info!("FDT after mods: {fdt}");
        info!("FDT after mods ptr: {new_dtb:p}");
        // info!("FDT after mods ptr: {x0:x}");
        // info!("FDT after mods ptr: {:p}", HEAP.as_mut_ptr());

        fdt
    }
}
