// Copyright 2025 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

const PLATFORM_DIR: &str = "src/platform";
const QEMU_PAYLOAD_SIZE_LIMIT: u64 = 64 * 1024 * 1024;
const PAYLOAD_SECTION_ALIGNMENT: u64 = 4;

fn main() {
    let platforms = discover_platforms();
    println!(
        "cargo::rustc-check-cfg=cfg(platform, values(\"{}\"))",
        platforms.join("\", \"")
    );
    println!("cargo:rerun-if-changed={PLATFORM_DIR}");

    let platform = env::var("CARGO_CFG_PLATFORM").expect("Missing platform name");
    assert!(
        platforms.iter().any(|known| known == &platform),
        "Unexpected platform name {platform:?}. Supported platforms: {platforms:?}",
    );

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR is expected to be set"));
    let platform_module = out_dir.join("platform.rs");
    fs::write(&platform_module, platform_module_source(&platforms))
        .unwrap_or_else(|e| panic!("Failed to write '{}': {e}", platform_module.display()));

    println!("cargo:rustc-link-arg=-Timage.ld");
    println!("cargo:rustc-link-arg=-Tlinker/{platform}.ld");
    println!("cargo:rerun-if-changed=linker/{platform}.ld");

    handle_payload(&platform);
}

fn discover_platforms() -> Vec<String> {
    let mut platforms = fs::read_dir(PLATFORM_DIR)
        .unwrap_or_else(|e| panic!("Failed to read {PLATFORM_DIR}: {e}"))
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
                return None;
            }
            Some(path.file_stem()?.to_str()?.to_owned())
        })
        .collect::<Vec<_>>();
    platforms.sort();
    platforms
}

fn platform_module_source(platforms: &[String]) -> String {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is expected");
    let mut output = String::new();
    for platform in platforms {
        let path = Path::new(&manifest_dir)
            .join(PLATFORM_DIR)
            .join(format!("{platform}.rs"));
        write!(
            output,
            r#"
#[cfg(platform = "{platform}")]
#[path = "{path}"]
mod {platform};

#[cfg(platform = "{platform}")]
pub use {platform}::PlatformImpl;
"#,
            path = path.display()
        )
        .expect("writing to String should not fail");
    }
    output
}

fn handle_payload(platform: &str) {
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
    let payload_size_aligned = payload_size.next_multiple_of(PAYLOAD_SECTION_ALIGNMENT);
    let payload_size_limit = payload_size_limit(platform);
    assert!(
        payload_size_aligned <= payload_size_limit,
        "Payload '{}' is too large for platform {platform:?}: {payload_size} bytes ({payload_size_aligned} bytes after section alignment) exceeds the {payload_size_limit} byte payload region. Please reduce RITM_PAYLOAD or increase the payload memory region in linker/{platform}.ld.",
        payload_canonical.display()
    );

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR is expected to be set in build.rs");
    let dest_path = Path::new(&out_dir).join("payload_constants.rs");

    fs::write(
        &dest_path,
        format!(
            r#"
        #[allow(clippy::unreadable_literal)]
        pub const PAYLOAD_SIZE: usize = {payload_size};
        #[allow(clippy::unreadable_literal)]
        pub const PAYLOAD_DATA: &[u8; PAYLOAD_SIZE] = include_bytes!(env!("RITM_PAYLOAD_PATH"));
        "#
        ),
    )
    .expect("Failed to write to {dest_path}");

    println!(
        "cargo:rustc-env=RITM_PAYLOAD_PATH={}",
        payload_canonical.display()
    );
}

fn payload_size_limit(platform: &str) -> u64 {
    match platform {
        "qemu" | "qemu_bl33" => QEMU_PAYLOAD_SIZE_LIMIT,
        _ => unreachable!("platform was already validated"),
    }
}
