// Copyright 2026 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use crate::exceptions::GuestRegisterStateRef;
use core::fmt::{Debug, Formatter};
use log::debug;
use smccc::arch::Error::NotSupported;

/// The result of an HVC call handled by the platform.
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum HvcResponse {
    /// The HVC call was handled, and returns the provided values in x0-x3.
    /// x4-x17 are preserved.
    Success([u64; 4]),
    /// The HVC call was handled, and returns the provided values in x0-x17.
    SuccessLarge([u64; 18]),
}

impl Debug for HvcResponse {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        let regs = match self {
            HvcResponse::Success(regs) => regs.as_slice(),
            HvcResponse::SuccessLarge(regs) => regs.as_slice(),
        };

        let mut d = f.debug_tuple("HvcResponse");
        for reg in regs {
            d.field(&format_args!("0x{reg:x}"));
        }
        d.finish()
    }
}

/// The result of an HVC call.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum HvcResult {
    /// The HVC call was not handled.
    Unhandled,
    /// The HVC call was handled, and either succeeded or failed with an error code.
    Handled(Result<HvcResponse, smccc::arch::Error>),
}

impl From<u64> for HvcResponse {
    fn from(value: u64) -> Self {
        HvcResponse::Success([value, 0, 0, 0])
    }
}

impl From<[u64; 4]> for HvcResponse {
    fn from(value: [u64; 4]) -> Self {
        HvcResponse::Success(value)
    }
}

impl From<[u64; 18]> for HvcResponse {
    fn from(value: [u64; 18]) -> Self {
        HvcResponse::SuccessLarge(value)
    }
}

impl HvcResult {
    pub(crate) fn modify_register_state(self, register_state: &mut GuestRegisterStateRef) {
        match self {
            HvcResult::Handled(Ok(HvcResponse::Success(results))) => {
                write_response_registers(register_state, &results);
            }
            HvcResult::Handled(Ok(HvcResponse::SuccessLarge(results))) => {
                write_response_registers(register_state, &results);
            }
            HvcResult::Handled(Err(error)) => {
                // SAFETY: x0 is the SMCCC return value register.
                unsafe {
                    register_state
                        .write_gpr(0, error_to_u64(error))
                        .expect("x0 is a valid guest register");
                }
            }
            HvcResult::Unhandled => {
                debug!("HVC call not handled, returning NOT_SUPPORTED");
                // SAFETY: x0 is the SMCCC return value register.
                unsafe {
                    register_state
                        .write_gpr(0, error_to_u64(NotSupported))
                        .expect("x0 is a valid guest register");
                }
            }
        }
    }
}

fn write_response_registers(register_state: &mut GuestRegisterStateRef, results: &[u64]) {
    for (index, value) in results.iter().copied().enumerate() {
        // SAFETY: SMCCC responses return values in x0-x17, and callers only pass slices from
        // fixed-size x0-x3 or x0-x17 response arrays.
        unsafe {
            register_state
                .write_gpr(index, value)
                .expect("SMCCC response register index should be valid");
        }
    }
}

#[must_use]
fn error_to_u64(error: smccc::arch::Error) -> u64 {
    i64::from(i32::from(error)).cast_unsigned()
}
