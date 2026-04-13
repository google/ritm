// Copyright 2026 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use crate::{CORE_COUNT, timer_helper};
use arm_gic::gicv3::{GicCpuInterface, GicV3, Group, HIGHEST_NS_PRIORITY};
use arm_gic::{IntId, InterruptGroup, Trigger};
use core::ptr::NonNull;
use spin::Lazy;
use spin::mutex::SpinMutex;

const GICD_BASE: usize = 0x0800_0000;
const GICR_BASE: usize = 0x080a_0000;
static GIC: Lazy<SpinMutex<GicV3>> = Lazy::new(create_gic);

fn create_gic() -> SpinMutex<GicV3<'static>> {
    let mut gic = unsafe {
        GicV3::new(
            arm_gic::UniqueMmioPointer::new(NonNull::new(GICD_BASE as *mut _).unwrap()),
            NonNull::new(GICR_BASE as *mut _).unwrap(),
            CORE_COUNT,
            false,
        )
    };

    gic.distributor().enable_group1_non_secure(true);
    SpinMutex::new(gic)
}

pub fn init_core() {
    GicCpuInterface::enable_system_register_el1();
    GicCpuInterface::enable_group1(true);
    arm_gic::irq_enable();
}

pub fn setup_timer_irq() {
    let mut gic = GIC.lock();

    let cpu = Some(0);
    let intid = timer_helper::INTERRUPT_ID;

    gic.set_interrupt_priority(intid, cpu, HIGHEST_NS_PRIORITY)
        .unwrap();
    gic.set_group(intid, cpu, Group::Group1NS).unwrap();
    gic.set_trigger(intid, cpu, Trigger::Level).unwrap();
    gic.enable_interrupt(intid, cpu, true).unwrap();
}

#[derive(Debug)]
pub struct GicInterrupt {
    pub id: IntId,
}

impl GicInterrupt {
    pub fn ack_interrupt() -> Self {
        Self {
            id: GicCpuInterface::get_and_acknowledge_interrupt(InterruptGroup::Group1)
                .expect("Failed to ack interrupt"),
        }
    }
}

impl Drop for GicInterrupt {
    fn drop(&mut self) {
        GicCpuInterface::end_interrupt(self.id, InterruptGroup::Group1);
    }
}
