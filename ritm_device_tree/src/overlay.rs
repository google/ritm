// Copyright 2025 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use crate::{
    Error,
    ir::{DeviceTree, DeviceTreeNode},
};
use alloc::vec::Vec;

impl DeviceTree {
    /// Applies a device tree overlay to this device tree.
    pub fn apply_overlay(&mut self, overlay: &DeviceTree) -> Result<(), Error> {
        let overlay_root = overlay.root();
        let mut phandle_map = PhandleMap::new(self);

        for fragment in overlay_root.children() {
            let target_path = fragment
                .property("target-path")
                .ok_or_else(|| Error::new(crate::error::ErrorKind::OverlayError, 0))?
                .as_str()
                .map_err(|_err| Error::new(crate::error::ErrorKind::OverlayError, 0))?;
            let overlay_node = fragment
                .children()
                .find(|c| c.name() == "__overlay__")
                .ok_or_else(|| Error::new(crate::error::ErrorKind::OverlayError, 0))?;

            let target_node = self
                .find_node_mut(target_path)
                .ok_or_else(|| Error::new(crate::error::ErrorKind::OverlayError, 0))?;

            merge_nodes(&mut phandle_map, target_node, overlay_node.clone());
        }

        Ok(())
    }
}

fn merge_nodes(phandle_map: &mut PhandleMap, existing: &mut DeviceTreeNode, new: DeviceTreeNode) {
    for prop in new.properties() {
        let mut new_prop = prop.clone();
        phandle_map.fixup_property(&mut new_prop);

        if let Some(existing_prop) = existing.property_mut(prop.name()) {
            *existing_prop = prop.clone();
        } else {
            existing.add_property(prop.clone());
        }
    }

    for child in new.children() {
        let mut new_child = child.clone();
        phandle_map.fixup_node(&mut new_child);

        if let Some(existing_child) = existing.child_mut(child.name()) {
            merge_nodes(phandle_map, existing_child, child.clone());
        } else {
            existing.add_child(child.clone());
        }
    }
}

struct PhandleMap {
    next_phandle: u32,
    map: Vec<(u32, u32)>,
}

impl PhandleMap {
    fn new(base: &DeviceTree) -> Self {
        let mut max_phandle = 0;
        base.root().for_each_node(&mut |node| {
            if let Some(phandle) = node.property("phandle") {
                let phandle = phandle.as_u32().unwrap();
                if phandle > max_phandle {
                    max_phandle = phandle;
                }
            }
        });

        Self {
            next_phandle: max_phandle + 1,
            map: Vec::new(),
        }
    }

    fn fixup_node(&mut self, node: &mut DeviceTreeNode) {
        if let Some(phandle) = node.property("phandle") {
            let phandle = phandle.as_u32().unwrap();
            let new_phandle = self.next_phandle;
            self.next_phandle += 1;
            self.map.push((phandle, new_phandle));
            node.property_mut("phandle")
                .unwrap()
                .set_value(new_phandle.to_be_bytes());
        }

        for prop in node.properties_mut() {
            self.fixup_property(prop);
        }

        for child in node.children_mut() {
            self.fixup_node(child);
        }
    }

    fn fixup_property(&mut self, prop: &mut crate::ir::DeviceTreeProperty) {
        if prop.value().len() % 4 != 0 {
            return;
        }

        let mut new_value = prop.value().to_vec();
        for (old, new) in &self.map {
            for chunk in new_value.chunks_mut(4) {
                let value = u32::from_be_bytes(chunk.try_into().unwrap());
                if value == *old {
                    chunk.copy_from_slice(&new.to_be_bytes());
                }
            }
        }
        prop.set_value(new_value);
    }
}

impl DeviceTreeNode {
    fn for_each_node<F>(&self, f: &mut F)
    where
        F: FnMut(&DeviceTreeNode),
    {
        f(self);
        for child in self.children() {
            child.for_each_node(f);
        }
    }
}
