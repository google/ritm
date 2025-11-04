// Copyright 2025 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! A library for parsing and manipulating Flattened Device Tree (FDT) blobs.
//!
//! This library provides a comprehensive API for working with FDTs, including:
//!
//! - A read-only API for parsing and traversing FDTs without memory allocation.
//! - A read-write API for creating and modifying FDTs in memory.
//! - Support for applying device tree overlays.
//!
//! ## Read-Only API
//!
//! The read-only API is centered around the [`Fdt`] struct, which provides a
//! safe, zero-copy view of an FDT blob. You can use this API to traverse the
//! device tree, inspect nodes and properties, and read property values.
//!
//! ## Read-Write API
//!
//! The read-write API is centered around the [`DeviceTree`] struct, which
//! provides a mutable, in-memory representation of a device tree. You can use
//! this API to create new device trees from scratch, modify existing ones, and
//! serialize them back to an FDT blob.
//!
//! ## Device Tree Overlays
//!
//! This library also provides support for applying device tree overlays. See
//! the [`overlay`] module for more information.

#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg(feature = "alloc")]
extern crate alloc;

pub type Result<T> = core::result::Result<T, Error>;

pub mod error;
pub mod fdt;
#[cfg(feature = "write")]
#[cfg_attr(docsrs, doc(cfg(feature = "write")))]
pub mod ir;
pub mod node;
#[cfg(feature = "write")]
#[cfg_attr(docsrs, doc(cfg(feature = "write")))]
pub mod overlay;
pub mod property;
#[cfg(feature = "write")]
mod writer;

pub use error::Error;
pub use fdt::Fdt;
#[cfg(feature = "write")]
pub use ir::{DeviceTree, DeviceTreeNode, DeviceTreeNodeBuilder, DeviceTreeProperty};
pub use node::FdtNode;
pub use property::FdtProperty;
