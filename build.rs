// Copyright 2025 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::env;
use std::fs;
use std::path::Path;

const PLATFORMS: [&str; 1] = ["qemu"];

fn main() {
    println!(
        "cargo::rustc-check-cfg=cfg(platform, values(\"{}\"))",
        PLATFORMS.join("\", \"")
    );

    let platform = env::var("CARGO_CFG_PLATFORM").expect("Missing platform name");
    assert!(
        PLATFORMS.contains(&platform.as_str()),
        "Unexpected platform name {platform:?}. Supported platforms: {PLATFORMS:?}",
    );

    println!("cargo:rustc-link-arg=-Timage.ld");
    println!("cargo:rustc-link-arg=-Tlinker/{platform}.ld");
    println!("cargo:rerun-if-changed=linker/{platform}.ld");

    handle_payload();
}

fn handle_payload() {
    let payload_path_str = env::var("RITM_PAYLOAD").expect(
        "RITM_PAYLOAD environment variable not set. Please set it to the path of the kernel image.",
    );
    let payload_path = Path::new(&payload_path_str);

    println!("cargo:rerun-if-env-changed=RITM_PAYLOAD");
    println!("cargo:rerun-if-changed={}", payload_path.display());

    let payload_canonical = fs::canonicalize(payload_path).unwrap_or_else(|e| {
        panic!(
            "Failed to canonicalize path '{}': {}. Make sure the RITM_PAYLOAD environment variable points to a valid payload file.",
            payload_path.display(),
            e
        )
    });

    let metadata = fs::metadata(&payload_canonical).unwrap_or_else(|e| {
        panic!(
            "Failed to read metadata for '{}': {}. Make sure the RITM_PAYLOAD environment variable points to a valid payload file.",
            payload_canonical.display(),
            e
        )
    });

    let payload_size = metadata.len();
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("payload_constants.rs");

    fs::write(
        &dest_path,
        format!(
            r#"
        pub const PAYLOAD_SIZE: usize = {size};
        pub const PAYLOAD_DATA: &[u8; {size}] = include_bytes!(env!("RITM_PAYLOAD_PATH"));
        "#,
            size = payload_size
        ),
    )
    .unwrap();

    println!(
        "cargo:rustc-env=RITM_PAYLOAD_PATH={}",
        payload_canonical.display()
    );
}
