// Copyright 2026 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/// A simple map that stores the items in a fixed-capacity array
/// and looks up the items linearly.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct SimpleMap<K, V, const CAPACITY: usize> {
    items: [Option<Item<K, V>>; CAPACITY],
}

impl<K: PartialEq, V, const CAPACITY: usize> SimpleMap<K, V, CAPACITY> {
    pub const fn new() -> Self {
        Self {
            items: [const { None }; CAPACITY],
        }
    }

    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        for item in &mut self.items {
            if let Some(inner) = item
                && inner.key == *key
            {
                return Some(&mut inner.value);
            }
        }
        None
    }

    pub fn insert(&mut self, key: K, value: V) -> &mut V {
        self.remove(&key);
        for item in &mut self.items {
            if item.is_none() {
                *item = Some(Item { key, value });
                return &mut item.as_mut().expect("the value was just set").value;
            }
        }
        panic!("maximum capacity exceeded");
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        for item in &mut self.items {
            if let Some(inner) = item
                && inner.key == *key
            {
                return item.take().map(|item| item.value);
            }
        }
        None
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
struct Item<K, V> {
    key: K,
    value: V,
}
