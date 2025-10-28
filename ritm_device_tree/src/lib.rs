// Copyright 2025 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg(feature = "alloc")]
extern crate alloc;

pub type Result<T> = core::result::Result<T, Error>;

pub mod error;
pub mod fdt;
#[cfg(feature = "write")]
pub mod ir;
pub mod node;
#[cfg(feature = "write")]
pub mod overlay;
pub mod property;
#[cfg(feature = "write")]
pub mod writer;

pub use error::Error;
pub use fdt::Fdt;
#[cfg(feature = "write")]
pub use ir::{DeviceTree, DeviceTreeNode, DeviceTreeNodeBuilder, DeviceTreeProperty};
pub use node::FdtNode;
pub use property::FdtProperty;
