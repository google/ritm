// Copyright 2026 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use arm_gic::IntId;
use arm_sysregs::{CntpCtlEl0, CntpTvalEl0, write_cntp_ctl_el0, write_cntp_tval_el0};

pub const INTERRUPT_ID: IntId = IntId::ppi(14);

pub fn set(ticks: u32) {
    write_cntp_tval_el0(CntpTvalEl0::empty().with_timervalue(ticks));
    write_cntp_ctl_el0(CntpCtlEl0::ENABLE);
}

pub fn stop() {
    write_cntp_ctl_el0(CntpCtlEl0::empty());
}
