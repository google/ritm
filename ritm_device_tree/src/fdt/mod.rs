// Copyright 2025 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! A read-only API for parsing and traversing a [Flattened Device Tree (FDT)].
//!
//! This module provides the [`Fdt`] struct, which is the entry point for
//! parsing and traversing an FDT blob. The API is designed to be safe and
//! efficient, performing no memory allocation and providing a zero-copy view
//! of the FDT data.
//! 
//! [Flattened Device Tree (FDT)]: https://devicetree-specification.readthedocs.io/en/latest/chapter5-flattened-format.html

use crate::error::{Error, ErrorKind};
mod node;
mod property;
use core::ffi::CStr;
use core::fmt;
use core::ptr;

use zerocopy::FromBytes;
use zerocopy::Immutable;
use zerocopy::IntoBytes;
use zerocopy::KnownLayout;
use zerocopy::Unaligned;
use zerocopy::byteorder::big_endian;

pub use node::FdtNode;
pub use property::FdtProperty;

/// Version of the FDT specification supported by this library.
const FDT_VERSION: u32 = 17;
pub(crate) const FDT_TAGSIZE: usize = core::mem::size_of::<u32>();
pub(crate) const FDT_MAGIC: u32 = 0xd00dfeed;
pub(crate) const FDT_BEGIN_NODE: u32 = 0x1;
pub(crate) const FDT_END_NODE: u32 = 0x2;
pub(crate) const FDT_END: u32 = 0x9;
pub(crate) const FDT_PROP: u32 = 0x3;
pub(crate) const FDT_NOP: u32 = 0x4;

#[repr(C, packed)]
#[derive(Debug, Copy, Clone, FromBytes, IntoBytes, Unaligned, Immutable, KnownLayout)]
pub(crate) struct FdtHeader {
    /// Magic number of the device tree.
    pub(crate) magic: big_endian::U32,
    /// Total size of the device tree.
    pub(crate) totalsize: big_endian::U32,
    /// Offset of the device tree structure.
    pub(crate) off_dt_struct: big_endian::U32,
    /// Offset of the device tree strings.
    pub(crate) off_dt_strings: big_endian::U32,
    /// Offset of the memory reservation map.
    pub(crate) off_mem_rsvmap: big_endian::U32,
    /// Version of the device tree.
    pub(crate) version: big_endian::U32,
    /// Last compatible version of the device tree.
    pub(crate) last_comp_version: big_endian::U32,
    /// Physical ID of the boot CPU.
    pub(crate) boot_cpuid_phys: big_endian::U32,
    /// Size of the device tree strings.
    pub(crate) size_dt_strings: big_endian::U32,
    /// Size of the device tree structure.
    pub(crate) size_dt_struct: big_endian::U32,
}

impl FdtHeader {
    pub(crate) fn magic(&self) -> u32 {
        self.magic.get()
    }

    pub(crate) fn totalsize(&self) -> u32 {
        self.totalsize.get()
    }

    pub(crate) fn off_dt_struct(&self) -> u32 {
        self.off_dt_struct.get()
    }

    pub(crate) fn off_dt_strings(&self) -> u32 {
        self.off_dt_strings.get()
    }

    pub(crate) fn off_mem_rsvmap(&self) -> u32 {
        self.off_mem_rsvmap.get()
    }

    pub(crate) fn version(&self) -> u32 {
        self.version.get()
    }

    pub(crate) fn last_comp_version(&self) -> u32 {
        self.last_comp_version.get()
    }

    pub(crate) fn boot_cpuid_phys(&self) -> u32 {
        self.boot_cpuid_phys.get()
    }

    pub(crate) fn size_dt_strings(&self) -> u32 {
        self.size_dt_strings.get()
    }

    pub(crate) fn size_dt_struct(&self) -> u32 {
        self.size_dt_struct.get()
    }
}

/// A flattened device tree.
pub struct Fdt<'a> {
    pub(crate) data: &'a [u8],
}

/// A token in the device tree structure.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum FdtToken {
    BeginNode,
    EndNode,
    Prop,
    Nop,
    End,
}

impl TryFrom<u32> for FdtToken {
    type Error = u32;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            FDT_BEGIN_NODE => Ok(FdtToken::BeginNode),
            FDT_END_NODE => Ok(FdtToken::EndNode),
            FDT_PROP => Ok(FdtToken::Prop),
            FDT_NOP => Ok(FdtToken::Nop),
            FDT_END => Ok(FdtToken::End),
            _ => Err(value),
        }
    }
}

impl<'a> Fdt<'a> {
    /// Creates a new `Fdt` from the given byte slice.
    ///
    /// # Examples
    ///
    /// ```
    /// # use ritm_device_tree::fdt::Fdt;
    /// # let dtb = include_bytes!("../../dtb/test.dtb");
    /// let fdt = Fdt::new(dtb).unwrap();
    /// ```
    pub fn new(data: &'a [u8]) -> Result<Self, Error> {
        if data.len() < core::mem::size_of::<FdtHeader>() {
            return Err(Error::new(ErrorKind::InvalidLength, 0));
        }

        let fdt = Fdt { data };
        let header = fdt.header();

        if header.magic() != FDT_MAGIC {
            return Err(Error::new(ErrorKind::InvalidMagic, 0));
        }
        if !(header.last_comp_version()..=header.version()).contains(&FDT_VERSION) {
            return Err(Error::new(
                ErrorKind::UnsupportedVersion(header.version()),
                20,
            ));
        }

        if header.totalsize() != data.len() as u32 {
            return Err(Error::new(ErrorKind::InvalidLength, 4));
        }

        Ok(fdt)
    }

    /// Creates a new `Fdt` from the given pointer.
    ///
    /// # Safety
    ///
    /// The `data` pointer must be a valid pointer to a Flattened Device Tree (FDT) blob.
    /// The memory region starting at `data` and spanning `totalsize` bytes (as specified in the FDT header)
    /// must be valid and accessible for reading.
    /// The FDT blob must be well-formed and adhere to the Device Tree Specification.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use ritm_device_tree::fdt::Fdt;
    /// # let dtb = include_bytes!("../../dtb/test.dtb");
    /// let ptr = dtb.as_ptr();
    /// let fdt = unsafe { Fdt::from_raw(ptr).unwrap() };
    /// ```
    pub unsafe fn from_raw(data: *const u8) -> Result<Self, Error> {
        // SAFETY: The caller guarantees that `data` is a valid pointer to a Flattened Device Tree (FDT) blob.
        // We are reading an `FdtHeader` from this pointer, which is a `#[repr(C, packed)]` struct.
        // The `totalsize` field of this header is then used to determine the total size of the FDT blob.
        // The caller must ensure that the memory at `data` is valid for at least `size_of::<FdtHeader>()` bytes.
        let header = unsafe { ptr::read_unaligned(data as *const FdtHeader) };
        let size = header.totalsize();
        // SAFETY: The caller must ensure that `data` is a valid pointer to a Flattened Device Tree (FDT) blob.
        // The caller must ensure the `data` spans `totalsize` bytes (as specified in the FDT header).
        let slice = unsafe { core::slice::from_raw_parts(data, size as usize) };
        Fdt::new(slice)
    }

    /// Returns the header of the device tree.
    pub(crate) fn header(&self) -> &FdtHeader {
        let (header, _remaining_bytes) = FdtHeader::ref_from_prefix(self.data)
            .expect("new() checks if the slice is at least as big as the header");
        header
    }

    /// Returns the root node of the device tree.
    ///
    /// # Examples
    ///
    /// ```
    /// # use ritm_device_tree::fdt::Fdt;
    /// # let dtb = include_bytes!("../../dtb/test.dtb");
    /// let fdt = Fdt::new(dtb).unwrap();
    /// let root = fdt.root().unwrap();
    /// assert_eq!(root.name().unwrap(), "");
    /// ```
    pub fn root(&self) -> Result<FdtNode<'_>, Error> {
        let offset = self.header().off_dt_struct() as usize;
        let token = self.read_token(offset)?;
        if token != FdtToken::BeginNode {
            return Err(Error::new(ErrorKind::BadToken(FDT_BEGIN_NODE), offset));
        }
        Ok(FdtNode { fdt: self, offset })
    }

    /// Finds a node by its path.
    ///
    /// # Performance
    ///
    /// This method traverses the device tree and its performance is linear in
    /// the number of nodes in the path. If you need to call this often,
    /// consider using [`DeviceTree::from_fdt`](crate::model::DeviceTree::from_fdt)
    /// first. [`DeviceTree`](crate::model::DeviceTree) stores the nodes in a
    /// hash map for constant-time lookup.
    ///
    /// # Examples
    ///
    /// ```
    /// # use ritm_device_tree::fdt::Fdt;
    /// # let dtb = include_bytes!("../../dtb/test_traversal.dtb");
    /// let fdt = Fdt::new(dtb).unwrap();
    /// let node = fdt.find_node("/a/b/c").unwrap().unwrap();
    /// assert_eq!(node.name().unwrap(), "c");
    /// ```
    pub fn find_node(&self, path: &str) -> Option<Result<FdtNode<'_>, Error>> {
        if !path.starts_with('/') {
            return None;
        }
        let mut current_node = match self.root() {
            Ok(node) => node,
            Err(e) => return Some(Err(e)),
        };
        if path == "/" {
            return Some(Ok(current_node));
        }
        for component in path.split('/').filter(|s| !s.is_empty()) {
            match current_node.children().find(|child| {
                child
                    .as_ref()
                    .is_ok_and(|c| c.name().is_ok_and(|n| n == component))
            }) {
                Some(Ok(node)) => current_node = node,
                Some(Err(e)) => return Some(Err(e)),
                None => return None,
            }
        }
        Some(Ok(current_node))
    }

    pub(crate) fn read_token(&self, offset: usize) -> Result<FdtToken, Error> {
        let val = big_endian::U32::ref_from_prefix(&self.data[offset..])
            .map(|(val, _)| val.get())
            .map_err(|_e| Error::new(ErrorKind::InvalidLength, offset))?;
        FdtToken::try_from(val).map_err(|t| Error::new(ErrorKind::BadToken(t), offset))
    }

    /// Return a string from the string block.
    pub(crate) fn string(&self, string_block_offset: usize) -> Result<&'a str, Error> {
        let header = self.header();
        let strings_start = header.off_dt_strings() as usize;
        let strings_size = header.size_dt_strings() as usize;
        let strings_end = strings_start + strings_size;
        let string_start = strings_start + string_block_offset;

        if string_start >= strings_end {
            return Err(Error::new(ErrorKind::InvalidLength, string_start));
        }

        self.string_at_offset(string_start, Some(strings_end))
    }

    /// Return a NUL-terminated string from a given offset.
    pub(crate) fn string_at_offset(
        &self,
        offset: usize,
        end: Option<usize>,
    ) -> Result<&'a str, Error> {
        let slice = match end {
            Some(end) => &self.data[offset..end],
            None => &self.data[offset..],
        };

        match CStr::from_bytes_until_nul(slice).map(|val| val.to_str()) {
            Ok(Ok(val)) => Ok(val),
            _ => Err(Error::new(ErrorKind::InvalidString, offset)),
        }
    }

    pub(crate) fn find_string_end(&self, start: usize) -> Result<usize, Error> {
        let mut offset = start;
        loop {
            match self.data.get(offset) {
                Some(0) => return Ok(offset + 1),
                Some(_) => {}
                None => return Err(Error::new(ErrorKind::InvalidString, start)),
            }
            offset += 1;
        }
    }

    pub(crate) fn next_sibling_offset(&self, mut offset: usize) -> crate::Result<usize> {
        offset += FDT_TAGSIZE; // Skip FDT_BEGIN_NODE

        // Skip node name
        offset = self.find_string_end(offset)?;
        offset = Self::align_tag_offset(offset);

        // Skip properties
        loop {
            let token = self.read_token(offset)?;
            match token {
                FdtToken::Prop => {
                    offset += FDT_TAGSIZE; // skip FDT_PROP
                    offset = self.next_property_offset(offset)?;
                }
                FdtToken::Nop => offset += FDT_TAGSIZE,
                _ => break,
            }
        }

        // Skip child nodes
        loop {
            let token = self.read_token(offset)?;
            match token {
                FdtToken::BeginNode => {
                    offset = self.next_sibling_offset(offset)?;
                }
                FdtToken::EndNode => {
                    offset += FDT_TAGSIZE;
                    break;
                }
                FdtToken::Nop => offset += FDT_TAGSIZE,
                _ => {}
            }
        }

        Ok(offset)
    }

    pub(crate) fn next_property_offset(&self, mut offset: usize) -> crate::Result<usize> {
        let len = big_endian::U32::ref_from_prefix(&self.data[offset..])
            .map(|(val, _)| val.get())
            .map_err(|_e| Error::new(ErrorKind::InvalidLength, offset))? as usize;
        offset += FDT_TAGSIZE; // skip value length
        offset += FDT_TAGSIZE; // skip name offset
        offset += len; // skip property value

        Ok(Self::align_tag_offset(offset))
    }

    pub(crate) fn align_tag_offset(offset: usize) -> usize {
        offset.next_multiple_of(FDT_TAGSIZE)
    }
}

impl<'a> fmt::Display for Fdt<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "/dts-v1/;")?;
        writeln!(f)?;
        let root = self.root().map_err(|_| fmt::Error)?;
        root.fmt_recursive(f, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;

    const FDT_HEADER_OK: &[u8] = &[
        0xd0, 0x0d, 0xfe, 0xed, // magic
        0x00, 0x00, 0x00, 0x3c, // totalsize = 60
        0x00, 0x00, 0x00, 0x38, // off_dt_struct = 56
        0x00, 0x00, 0x00, 0x3c, // off_dt_strings = 60
        0x00, 0x00, 0x00, 0x28, // off_mem_rsvmap = 40
        0x00, 0x00, 0x00, 0x11, // version = 17
        0x00, 0x00, 0x00, 0x10, // last_comp_version = 16
        0x00, 0x00, 0x00, 0x00, // boot_cpuid_phys = 0
        0x00, 0x00, 0x00, 0x00, // size_dt_strings = 0
        0x00, 0x00, 0x00, 0x04, // size_dt_struct = 4
        0x00, 0x00, 0x00, 0x00, // memory reservation
        0x00, 0x00, 0x00, 0x00, // ...
        0x00, 0x00, 0x00, 0x00, // ...
        0x00, 0x00, 0x00, 0x00, // ...
        0x00, 0x00, 0x00, 0x09, // dt struct
    ];

    #[test]
    fn header_is_parsed_correctly() {
        let fdt = Fdt::new(FDT_HEADER_OK).unwrap();
        let header = fdt.header();

        assert_eq!(header.totalsize(), 60);
        assert_eq!(header.off_dt_struct(), 56);
        assert_eq!(header.off_dt_strings(), 60);
        assert_eq!(header.off_mem_rsvmap(), 40);
        assert_eq!(header.version(), 17);
        assert_eq!(header.last_comp_version(), 16);
        assert_eq!(header.boot_cpuid_phys(), 0);
        assert_eq!(header.size_dt_strings(), 0);
        assert_eq!(header.size_dt_struct(), 4);
    }

    #[test]
    fn invalid_magic() {
        let mut header = FDT_HEADER_OK.to_vec();
        header[0] = 0x00;
        let result = Fdt::new(&header);
        assert!(matches!(result, Err(e) if matches!(e.kind, ErrorKind::InvalidMagic)));
    }

    #[test]
    fn invalid_length() {
        let header = &FDT_HEADER_OK[..10];
        let result = Fdt::new(header);
        assert!(matches!(result, Err(e) if matches!(e.kind, ErrorKind::InvalidLength)));
    }

    #[test]
    fn unsupported_version() {
        let mut header = FDT_HEADER_OK.to_vec();
        header[23] = 0x10;
        let result = Fdt::new(&header);
        assert!(matches!(result, Err(e) if matches!(e.kind, ErrorKind::UnsupportedVersion(16))));
    }
}
