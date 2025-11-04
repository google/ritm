// Copyright 2025 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use crate::{
    error::Error,
    fdt::{FDT_TAGSIZE, Fdt, FdtToken},
    property::FdtPropIter,
    FdtProperty,
};
use core::fmt;

/// A node in a flattened device tree.
#[derive(Clone, Copy)]
pub struct FdtNode<'a> {
    pub(crate) fdt: &'a Fdt<'a>,
    pub(crate) offset: usize,
}

impl<'a> FdtNode<'a> {
    /// Returns the name of this node.
    pub fn name(&self) -> Result<&'a str, Error> {
        let name_offset = self.offset + FDT_TAGSIZE;
        self.fdt.string_at_offset(name_offset, None)
    }

    /// Returns a property by its name.
    pub fn property(&self, name: &str) -> crate::Result<Option<FdtProperty<'a>>> {
        for property in self.properties() {
            let property = property?;
            if property.name() == name {
                return Ok(Some(property));
            }
        }
        Ok(None)
    }

    /// Returns an iterator over the properties of this node.
    pub fn properties(&self) -> impl Iterator<Item = crate::Result<FdtProperty<'a>>> + use<'a> {
        FdtPropIter::Start {
            fdt: self.fdt,
            offset: self.offset,
        }
    }

    /// Returns a child node by its name.
    pub fn child(&self, name: &str) -> crate::Result<Option<FdtNode<'a>>> {
        for child in self.children() {
            let child = child?;
            if child.name()? == name {
                return Ok(Some(child));
            }
        }
        Ok(None)
    }

    /// Returns an iterator over the children of this node.
    pub fn children(&self) -> impl Iterator<Item = crate::Result<FdtNode<'a>>> + use<'a> {
        FdtChildIter::Start {
            fdt: self.fdt,
            offset: self.offset,
        }
    }

    pub(crate) fn fmt_recursive(&self, f: &mut fmt::Formatter<'_>, indent: usize) -> fmt::Result {
        let name = self.name().map_err(|_| fmt::Error)?;
        if name.is_empty() {
            writeln!(f, "{:indent$}/ {{", "", indent = indent)?;
        } else {
            writeln!(f, "{:indent$}{} {{", "", name, indent = indent)?;
        }

        for prop in self.properties() {
            match prop {
                Ok(prop) => prop.fmt(f, indent + 4)?,
                Err(_e) => {
                    writeln!(f, "<Error reading property>")?;
                }
            }
        }

        for child in self.children() {
            writeln!(f)?;
            match child {
                Ok(child) => child.fmt_recursive(f, indent + 4)?,
                Err(_e) => {
                    writeln!(f, "<Error reading child node>")?;
                }
            }
        }

        writeln!(f, "{:indent$}}};", "", indent = indent)
    }
}

/// An iterator over the children of a device tree node.
enum FdtChildIter<'a> {
    Start { fdt: &'a Fdt<'a>, offset: usize },
    Running { fdt: &'a Fdt<'a>, offset: usize },
    Error,
}

impl<'a> Iterator for FdtChildIter<'a> {
    type Item = crate::Result<FdtNode<'a>>;

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

impl<'a> FdtChildIter<'a> {
    fn try_next(fdt: &'a Fdt<'a>, offset: &mut usize) -> Option<crate::Result<FdtNode<'a>>> {
        loop {
            let token = match fdt.read_token(*offset) {
                Ok(token) => token,
                Err(e) => return Some(Err(e)),
            };
            match token {
                FdtToken::BeginNode => {
                    let node_offset = *offset;
                    *offset = match fdt.next_sibling_offset(*offset) {
                        Ok(offset) => offset,
                        Err(e) => return Some(Err(e)),
                    };
                    return Some(Ok(FdtNode {
                        fdt,
                        offset: node_offset,
                    }));
                }
                FdtToken::EndNode => return None,
                FdtToken::Prop => {
                    *offset = match fdt.next_property_offset(*offset + FDT_TAGSIZE) {
                        Ok(offset) => offset,
                        Err(e) => return Some(Err(e)),
                    };
                }
                FdtToken::Nop => *offset += FDT_TAGSIZE,
                _ => return None,
            }
        }
    }
}
