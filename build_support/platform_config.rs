// Copyright 2026 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub fn platform_config(path: &Path) -> HashMap<String, u64> {
    let source = fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("Failed to read '{}': {e}", path.display()));
    parse_platform_config(&source)
        .unwrap_or_else(|e| panic!("Failed to parse '{}': {e}", path.display()))
}

fn parse_platform_config(source: &str) -> Result<HashMap<String, u64>, String> {
    let mut config = HashMap::new();
    for (line_number, line) in source.lines().enumerate() {
        let line = line.split('#').next().unwrap_or(line).trim();
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(format!("line {}: expected key=value", line_number + 1));
        };
        let key = key.trim();
        if key.is_empty() {
            return Err(format!("line {}: empty key", line_number + 1));
        }
        let value = parse_int(value.trim())
            .ok_or_else(|| format!("line {}: invalid integer value", line_number + 1))?;
        config.insert(key.to_owned(), value);
    }
    Ok(config)
}

fn parse_int(value: &str) -> Option<u64> {
    let value = value.replace('_', "");
    if let Some(hex) = value.strip_prefix("0x") {
        u64::from_str_radix(hex, 16).ok()
    } else {
        value.parse().ok()
    }
}
