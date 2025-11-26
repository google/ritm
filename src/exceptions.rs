// Copyright 2025 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use aarch64_rt::{ExceptionHandlers, RegisterStateRef};

pub struct Exceptions;
impl ExceptionHandlers for Exceptions {
    extern "C" fn sync_lower(register_state: RegisterStateRef) {
        // sync_lower exception is most likely a HVC/SMC call, which we should
        // handle in the hypervisor.
        crate::hypervisor::handle_sync_lower(register_state);
    }
}
