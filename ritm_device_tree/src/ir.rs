// Copyright 2025 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use core::fmt::Display;

use crate::{error::Error, fdt::Fdt, node::FdtNode, property::FdtProperty, writer};
use alloc::{
    borrow::ToOwned, string::{String, ToString}, vec::Vec
};
use indexmap::IndexMap;
use twox_hash::xxhash64;

/// A mutable, in-memory representation of a device tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceTree {
    root: DeviceTreeNode,
}

impl DeviceTree {
    /// Creates a new `DeviceTree` with the given root node.
    pub fn new(root: DeviceTreeNode) -> Self {
        Self { root }
    }

    /// Creates a new `DeviceTree` from a `Fdt`.
    pub fn from_fdt(fdt: &Fdt) -> Result<Self, Error> {
        let root = DeviceTreeNode::try_from(fdt.root()?)?;
        Ok(DeviceTree { root })
    }

    /// Serializes the `DeviceTree` to a flattened device tree blob.
    pub fn to_dtb(&self) -> Vec<u8> {
        writer::to_bytes(self)
    }

    /// Returns a reference to the root node of the device tree.
    pub fn root(&self) -> &DeviceTreeNode {
        &self.root
    }

    /// Returns a mutable reference to the root node of the device tree.
    pub fn root_mut(&mut self) -> &mut DeviceTreeNode {
        &mut self.root
    }

    /// Finds a node by its path and returns a mutable reference to it.
    pub fn find_node_mut(&mut self, path: &str) -> Option<&mut DeviceTreeNode> {
        if !path.starts_with('/') {
            return None;
        }
        let mut current_node = &mut self.root;
        if path == "/" {
            return Some(current_node);
        }
        for component in path.split('/').filter(|s| !s.is_empty()) {
            match current_node.child_mut(component) {
                Some(node) => current_node = node,
                None => return None,
            }
        }
        Some(current_node)
    }
}

/// A mutable, in-memory representation of a device tree node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceTreeNode {
    name: String,
    properties: IndexMap<String, DeviceTreeProperty, xxhash64::State>,
    children: IndexMap<String, DeviceTreeNode, xxhash64::State>,
}

impl Default for DeviceTreeNode {
    fn default() -> Self {
        Self {
            name: String::new(),
            properties: IndexMap::with_hasher(xxhash64::State::with_seed(0xdead_cafe)),
            children: IndexMap::with_hasher(xxhash64::State::with_seed(0xdead_cafe)),
        }
    }
}

impl DeviceTreeNode {
    /// Creates a new `DeviceTreeNode` with the given name.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }

    /// Creates a new `DeviceTreeNodeBuilder` with the given name.
    #[must_use]
    pub fn builder(name: impl Into<String>) -> DeviceTreeNodeBuilder {
        DeviceTreeNodeBuilder::new(name)
    }

    /// Returns the name of this node.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns an iterator over the properties of this node.
    #[must_use]
    pub fn properties(&self) -> impl Iterator<Item = &DeviceTreeProperty> {
        self.properties.values()
    }

    /// Returns a mutable iterator over the properties of this node.
    #[must_use]
    pub fn properties_mut(&mut self) -> impl Iterator<Item = &mut DeviceTreeProperty> {
        self.properties.values_mut()
    }

    /// Finds a property by its name and returns a reference to it.
    #[must_use]
    pub fn property(&self, name: &str) -> Option<&DeviceTreeProperty> {
        self.properties.get(name)
    }

    /// Finds a property by its name and returns a mutable reference to it.
    #[must_use]
    pub fn property_mut(&mut self, name: &str) -> Option<&mut DeviceTreeProperty> {
        self.properties.get_mut(name)
    }

    /// Adds a property to this node.
    pub fn add_property(&mut self, property: DeviceTreeProperty) {
        self.properties.insert(property.name().to_owned(), property);
    }

    /// Removes a property from this node by its name.
    pub fn remove_property(&mut self, name: &str) -> Option<DeviceTreeProperty> {
        self.properties.shift_remove(name)
    }

    /// Returns an iterator over the children of this node.
    #[must_use]
    pub fn children(&self) -> impl Iterator<Item = &DeviceTreeNode> {
        self.children.values()
    }

    /// Returns a mutable iterator over the children of this node.
    #[must_use]
    pub fn children_mut(&mut self) -> impl Iterator<Item = &mut DeviceTreeNode> {
        self.children.values_mut()
    }

    /// Finds a child by its name and returns a mutable reference to it.
    #[must_use]
    pub fn child(&self, name: &str) -> Option<&DeviceTreeNode> {
        self.children.get(name)
    }

    /// Finds a child by its name and returns a mutable reference to it.
    #[must_use]
    pub fn child_mut(&mut self, name: &str) -> Option<&mut DeviceTreeNode> {
        self.children.get_mut(name)
    }

    /// Adds a child to this node.
    pub fn add_child(&mut self, child: DeviceTreeNode) {
        self.children.insert(child.name().to_owned(), child);
    }

    /// Removes a child from this node by its name.
    pub fn remove_child(&mut self, name: &str) -> Option<DeviceTreeNode> {
        self.children.shift_remove(name)
    }
}

impl Display for DeviceTree {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        Fdt::new(&self.to_dtb()).unwrap().fmt(f)
    }
}

impl<'a> TryFrom<FdtNode<'a>> for DeviceTreeNode {
    type Error = Error;

    fn try_from(node: FdtNode<'a>) -> Result<Self, Self::Error> {
        let name = node.name()?.to_string();
        let properties = node
            .properties()
            .map(|property| property?.try_into())
            .collect::<Result<Vec<DeviceTreeProperty>, _>>()?;
        let mut property_map = IndexMap::with_capacity_and_hasher(properties.len(), xxhash64::State::with_seed(0xdead_cafe));
        for property in properties {
            property_map.insert(property.name().to_owned(), property);
        }

        let children_vec: Vec<DeviceTreeNode> = node
            .children()
            .map(|child| child?.try_into())
            .collect::<Result<Vec<_>, _>>()?;
        let mut children =
            IndexMap::with_capacity_and_hasher(children_vec.len(), xxhash64::State::with_seed(0xdead_cafe));
        for child in children_vec {
            children.insert(child.name().to_owned(), child);
        }

        Ok(DeviceTreeNode {
            name,
            properties: property_map,
            children,
        })
    }
}

/// A mutable, in-memory representation of a device tree property.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceTreeProperty {
    name: String,
    value: Vec<u8>,
}

impl DeviceTreeProperty {
    /// Creates a new `DeviceTreeProperty` with the given name and value.
    #[must_use]
    pub fn new(name: impl Into<String>, value: impl Into<Vec<u8>>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }

    /// Returns the name of this property.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the value of this property.
    #[must_use]
    pub fn value(&self) -> &[u8] {
        &self.value
    }

    /// Sets the value of this property.
    pub fn set_value(&mut self, value: impl Into<Vec<u8>>) {
        self.value = value.into();
    }

    /// Returns the value of this property as a `u32`.
    pub fn as_u32(&self) -> Result<u32, ()> {
        self.value
            .as_slice()
            .try_into()
            .map(u32::from_be_bytes)
            .map_err(|_| ())
    }

    /// Returns the value of this property as a string.
    pub fn as_str(&self) -> Result<&str, ()> {
        core::str::from_utf8(&self.value)
            .map(|s| s.trim_end_matches('\0'))
            .map_err(|_| ())
    }
}

impl<'a> TryFrom<FdtProperty<'a>> for DeviceTreeProperty {
    type Error = Error;

    fn try_from(prop: FdtProperty<'a>) -> Result<Self, Self::Error> {
        let name = prop.name.to_string();
        let value = prop.value.to_vec();
        Ok(DeviceTreeProperty { name, value })
    }
}

/// A builder for creating `DeviceTreeNode`s.
#[derive(Debug, Default)]
pub struct DeviceTreeNodeBuilder {
    node: DeviceTreeNode,
}

impl DeviceTreeNodeBuilder {
    fn new(name: impl Into<String>) -> Self {
        Self {
            node: DeviceTreeNode::new(name),
        }
    }

    /// Adds a property to the node.
    #[must_use]
    pub fn property(mut self, property: DeviceTreeProperty) -> Self {
        self.node.add_property(property);
        self
    }

    /// Adds a child to the node.
    #[must_use]
    pub fn child(mut self, child: DeviceTreeNode) -> Self {
        self.node.add_child(child);
        self
    }

    /// Builds the `DeviceTreeNode`.
    #[must_use]
    pub fn build(self) -> DeviceTreeNode {
        self.node
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_and_get_child() {
        let mut root = DeviceTreeNode::new("root");
        let child1 = DeviceTreeNode::new("child1");
        let child2 = DeviceTreeNode::new("child2");

        root.add_child(child1);
        root.add_child(child2);

        assert!(root.child("child1").is_some());
        assert!(root.child("child2").is_some());
        assert!(root.child("nonexistent").is_none());
    }

    #[test]
    fn test_remove_child() {
        let mut root = DeviceTreeNode::new("root");
        let child1 = DeviceTreeNode::new("child1");
        root.add_child(child1);

        assert!(root.child("child1").is_some());
        let removed_child = root.remove_child("child1");
        assert!(removed_child.is_some());
        assert!(root.child("child1").is_none());
    }

    #[test]
    fn test_find_node_mut_with_children_map() {
        let mut root = DeviceTreeNode::new("root");
        let mut child1 = DeviceTreeNode::new("child1");
        child1.add_child(DeviceTreeNode::new("grandchild1"));
        root.add_child(child1);

        let mut tree = DeviceTree::new(root);

        let node = tree.find_node_mut("/child1").unwrap();
        assert_eq!(node.name(), "child1");

        let grandchild = tree.find_node_mut("/child1/grandchild1").unwrap();
        assert_eq!(grandchild.name(), "grandchild1");

        assert!(tree.find_node_mut("/child1/nonexistent").is_none());
        assert!(tree.find_node_mut("/nonexistent").is_none());
    }
}