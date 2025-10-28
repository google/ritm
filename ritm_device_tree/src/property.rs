// Copyright 2025 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use crate::{
    error::{Error, ErrorKind},
    fdt::{FDT_TAGSIZE, Fdt},
};
use core::ffi::CStr;
use core::fmt;
use zerocopy::{FromBytes, big_endian};

/// A property of a device tree node.
#[derive(Debug, PartialEq)]
pub struct FdtProperty<'a> {
    /// The name of the property.
    pub name: &'a str,
    /// The value of the property.
    pub value: &'a [u8],
    value_offset: usize,
}

impl<'a> FdtProperty<'a> {
    /// Returns the value of this property as a `u32`.
    pub fn as_u32(&self) -> Result<u32, Error> {
        big_endian::U32::ref_from_bytes(self.value)
            .map(|val| val.get())
            .map_err(|_e| Error::new(ErrorKind::InvalidLength, self.value_offset))
    }

    /// Returns the value of this property as a `u64`.
    pub fn as_u64(&self) -> Result<u64, Error> {
        big_endian::U64::ref_from_bytes(self.value)
            .map(|val| val.get())
            .map_err(|_e| Error::new(ErrorKind::InvalidLength, self.value_offset))
    }

    /// Returns the value of this property as a string.
    pub fn as_str(&self) -> Result<&'a str, Error> {
        let cstr = CStr::from_bytes_with_nul(self.value)
            .map_err(|_| Error::new(ErrorKind::InvalidString, self.value_offset))?;
        cstr.to_str()
            .map_err(|_| Error::new(ErrorKind::InvalidString, self.value_offset))
    }

    /// Returns an iterator over the strings in this property.
    pub fn as_str_list(&self) -> impl Iterator<Item = &'a str> {
        FdtStringListIterator { value: self.value }
    }

    pub(crate) fn fmt(&self, f: &mut fmt::Formatter<'_>, indent: usize) -> fmt::Result {
        write!(f, "{:indent$}{}", "", self.name, indent = indent)?;

        if self.value.is_empty() {
            writeln!(f, ";")?;
            return Ok(());
        }

        let is_printable = self
            .value
            .iter()
            .all(|&ch| ch.is_ascii_graphic() || ch == b' ' || ch == 0);
        let has_empty = self.value.windows(2).all(|window| window == [0, 0]);
        if is_printable && self.value.ends_with(&[0]) && !has_empty {
            let mut strings = self.as_str_list();
            if let Some(first) = strings.next() {
                write!(f, " = \"{}\"", first)?;
                for s in strings {
                    write!(f, ", \"{}\"", s)?;
                }
                writeln!(f, ";")?;
                return Ok(());
            }
        }

        if self.value.len() % 4 == 0 {
            write!(f, " = <")?;
            for (i, chunk) in self.value.chunks_exact(4).enumerate() {
                if i > 0 {
                    write!(f, " ")?;
                }
                let val = u32::from_be_bytes(chunk.try_into().unwrap());
                write!(f, "0x{:02x}", val)?;
            }
            writeln!(f, ">;")?;
        } else {
            write!(f, " = [")?;
            for (i, byte) in self.value.iter().enumerate() {
                if i > 0 {
                    write!(f, " ")?;
                }
                write!(f, "{:02x}", byte)?;
            }
            writeln!(f, "];")?;
        }

        Ok(())
    }
}

/// An iterator over the properties of a device tree node.
pub(crate) enum FdtPropIter<'a> {
    Start { fdt: &'a Fdt<'a>, offset: usize },
    Running { fdt: &'a Fdt<'a>, offset: usize },
    Error,
}

impl<'a> Iterator for FdtPropIter<'a> {
    type Item = crate::Result<FdtProperty<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Start { fdt, offset } => {
                let mut offset = *offset;
                offset += FDT_TAGSIZE; // Skip FDT_BEGIN_NODE
                offset = match fdt.find_string_end(offset) {
                    Ok(offset) => offset,
                    Err(e) => {
                        *self = Self::Error;
                        return Some(Err(e));
                    }
                };
                offset = Fdt::align_tag_offset(offset);
                *self = Self::Running { fdt, offset };
                self.next()
            }
            Self::Running { fdt, offset } => match Self::try_next(fdt, offset) {
                Some(Ok(val)) => Some(Ok(val)),
                Some(Err(e)) => {
                    *self = Self::Error;
                    Some(Err(e))
                }
                None => None,
            },
            Self::Error => None,
        }
    }
}

impl<'a> FdtPropIter<'a> {
    fn try_next(fdt: &'a Fdt<'a>, offset: &mut usize) -> Option<crate::Result<FdtProperty<'a>>> {
        loop {
            let token = match fdt.read_u32(*offset) {
                Ok(token) => token,
                Err(e) => return Some(Err(e)),
            };
            match token {
                crate::fdt::FDT_PROP => {
                    let len = match fdt.read_u32(*offset + FDT_TAGSIZE) {
                        Ok(len) => len as usize,
                        Err(e) => return Some(Err(e)),
                    };
                    let nameoff = match fdt.read_u32(*offset + 2 * FDT_TAGSIZE) {
                        Ok(nameoff) => nameoff as usize,
                        Err(e) => return Some(Err(e)),
                    };
                    let prop_offset = *offset + 3 * FDT_TAGSIZE;
                    *offset = Fdt::align_tag_offset(prop_offset + len);
                    let name = match fdt.string(nameoff) {
                        Ok(name) => name,
                        Err(e) => return Some(Err(e)),
                    };
                    let value = fdt.data.get(prop_offset..prop_offset + len)?;
                    return Some(Ok(FdtProperty {
                        name,
                        value,
                        value_offset: prop_offset,
                    }));
                }
                crate::fdt::FDT_NOP => *offset += FDT_TAGSIZE,
                _ => return None,
            }
        }
    }
}

struct FdtStringListIterator<'a> {
    value: &'a [u8],
}

impl<'a> Iterator for FdtStringListIterator<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        if self.value.is_empty() {
            return None;
        }
        let cstr = CStr::from_bytes_until_nul(self.value).ok()?;
        let s = cstr.to_str().ok()?;
        self.value = &self.value[s.len() + 1..];
        Some(s)
    }
}
