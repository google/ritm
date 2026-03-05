// Copyright 2025 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use crate::platform::PlatformImpl;
use aarch64_paging::descriptor::{El23Attributes, Stage2Attributes};
use aarch64_rt::initial_pagetable;

/// Attributes to use for device memory in the initial identity map.
pub const DEVICE_ATTRIBUTES: El23Attributes = El23Attributes::VALID
    .union(El23Attributes::ATTRIBUTE_INDEX_0)
    .union(El23Attributes::ACCESSED)
    .union(El23Attributes::XN);

/// Attributes to use for normal memory in the initial identity map.
pub const MEMORY_ATTRIBUTES: El23Attributes = El23Attributes::VALID
    .union(El23Attributes::ATTRIBUTE_INDEX_1)
    .union(El23Attributes::INNER_SHAREABLE)
    .union(El23Attributes::ACCESSED)
    .union(El23Attributes::NON_GLOBAL);

// The initial hardcoded page table used before the Rust code starts and activates the main page
// table.
initial_pagetable!(PlatformImpl::initial_idmap());

/// Attributes to use for device memory in the stage-2 page tables.
pub const STAGE2_DEVICE_ATTRIBUTES: Stage2Attributes = Stage2Attributes::VALID
    .union(Stage2Attributes::MEMATTR_DEVICE_nGnRnE)
    .union(Stage2Attributes::S2AP_ACCESS_RW)
    .union(Stage2Attributes::ACCESS_FLAG)
    .union(Stage2Attributes::SH_NONE);

/// Attributes to use for normal memory in the stage-2 page tables.
pub const STAGE2_MEMORY_ATTRIBUTES: Stage2Attributes = Stage2Attributes::VALID
    .union(Stage2Attributes::MEMATTR_NORMAL_OUTER_WB)
    .union(Stage2Attributes::MEMATTR_NORMAL_INNER_WB)
    .union(Stage2Attributes::S2AP_ACCESS_RW)
    .union(Stage2Attributes::ACCESS_FLAG)
    .union(Stage2Attributes::SH_INNER);
