// Copyright 2025 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use crate::platform::PlatformImpl;
use aarch64_paging::descriptor::Stage1Attributes;
use aarch64_rt::initial_pagetable;

/// Attributes to use for device memory in the initial identity map.
pub const DEVICE_ATTRIBUTES: Stage1Attributes = Stage1Attributes::VALID
    .union(Stage1Attributes::ATTRIBUTE_INDEX_0)
    .union(Stage1Attributes::ACCESSED)
    .union(Stage1Attributes::UXN);

/// Attributes to use for normal memory in the initial identity map.
pub const MEMORY_ATTRIBUTES: Stage1Attributes = Stage1Attributes::VALID
    .union(Stage1Attributes::ATTRIBUTE_INDEX_1)
    .union(Stage1Attributes::INNER_SHAREABLE)
    .union(Stage1Attributes::ACCESSED)
    .union(Stage1Attributes::NON_GLOBAL);

// The initial hardcoded page table used before the Rust code starts and activates the main page
// table.
initial_pagetable!(PlatformImpl::initial_idmap());
