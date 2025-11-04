// Copyright 2025 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Error types for the `ritm_device_tree` crate.

use core::fmt;

/// An error that can occur when parsing a device tree.
#[derive(Debug)]
#[non_exhaustive]
pub struct Error {
    offset: usize,
    pub kind: ErrorKind,
}

impl Error {
    pub(crate) fn new(kind: ErrorKind, offset: usize) -> Self {
        Self { offset, kind }
    }
}

/// The kind of an error that can occur when parsing a device tree.
#[derive(Debug)]
#[non_exhaustive]
pub enum ErrorKind {
    /// The magic number of the device tree is invalid.
    InvalidMagic,
    /// The Device Tree version is not supported by this library.
    UnsupportedVersion(u32),
    /// The length of the device tree is invalid.
    InvalidLength,
    /// An invalid token was encountered.
    BadToken(u32),
    /// An invalid string was encountered.
    InvalidString,
    /// An error occurred while applying an overlay.
    OverlayError,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at offset {}", self.kind, self.offset)
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorKind::InvalidMagic => write!(f, "invalid FDT magic number"),
            ErrorKind::UnsupportedVersion(version) => {
                write!(f, "the FDT version {} is not supported", version)
            }
            ErrorKind::InvalidLength => write!(f, "invalid FDT length"),
            ErrorKind::BadToken(token) => write!(f, "bad FDT token: 0x{:x}", token),
            ErrorKind::InvalidString => write!(f, "invalid string in FDT"),
            ErrorKind::OverlayError => write!(f, "invalid overlay"),
        }
    }
}

impl core::error::Error for Error {}
