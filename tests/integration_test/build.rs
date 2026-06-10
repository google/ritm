// Copyright 2026 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::env;
use std::fs;
use std::path::Path;

#[path = "../../build_support/platform_config.rs"]
mod platform_config;

use platform_config::platform_config;

const RITM_IMAGE_ADDRESS_KEY: &str = "ritm_image_address";
const PAYLOAD_ADDRESS_KEY: &str = "payload_address";

fn main() {
    let dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let project_root = Path::new(&dir)
        .parent()
        .expect("integration test crate should be under tests/")
        .parent()
        .expect("tests/ should be under the project root");
    let config_file = project_root.join("platforms/qemu.conf");
    println!(
        "cargo:rerun-if-changed={}",
        project_root
            .join("build_support/platform_config.rs")
            .display()
    );
    println!("cargo:rerun-if-changed={}", config_file.display());

    let config = platform_config(&config_file);
    let ritm_image_address = config_value(&config, RITM_IMAGE_ADDRESS_KEY, &config_file);
    let payload_address = config_value(&config, PAYLOAD_ADDRESS_KEY, &config_file);

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR is expected to be set in build.rs");
    let out_dir = Path::new(&out_dir);
    let constants_path = out_dir.join("platform_constants.rs");
    fs::write(
        &constants_path,
        format!(
            r"
        #[allow(clippy::unreadable_literal)]
        pub const RITM_IMAGE_ADDRESS: usize = {ritm_image_address:#x};
        "
        ),
    )
    .expect("Failed to write platform constants");

    let layout_path = out_dir.join("layout.ld");
    fs::write(
        &layout_path,
        format!(
            r#"MEMORY
{{
	image : ORIGIN = {payload_address:#x}, LENGTH = 4M
}}
"#
        ),
    )
    .expect("Failed to write linker layout");

    println!("cargo:rustc-link-search={dir}");
    println!("cargo:rustc-link-arg=-Timage.ld");
    println!("cargo:rustc-link-arg=-T{}", layout_path.display());
}

fn config_value(config: &std::collections::HashMap<String, u64>, key: &str, path: &Path) -> u64 {
    *config
        .get(key)
        .unwrap_or_else(|| panic!("Could not find {key:?} in {}", path.display()))
}
