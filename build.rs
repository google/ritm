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

#[path = "build_support/platform_config.rs"]
mod platform_config;

use platform_config::platform_config;

const BUILD_SUPPORT_PLATFORM_CONFIG: &str = "build_support/platform_config.rs";
const PLATFORM_CONFIG_DIR: &str = "platforms";
const PLATFORM_DIR: &str = "src/platform";
const RITM_IMAGE_ADDRESS_KEY: &str = "ritm_image_address";
const PAYLOAD_ADDRESS_KEY: &str = "payload_address";
const PAYLOAD_SIZE_LIMIT: u64 = 64 * 1024 * 1024;
const PAYLOAD_SECTION_ALIGNMENT: u64 = 4;

fn main() {
    let platforms = discover_platforms();
    println!(
        "cargo::rustc-check-cfg=cfg(platform, values(\"{}\"))",
        platforms.join("\", \"")
    );
    println!("cargo:rerun-if-changed={BUILD_SUPPORT_PLATFORM_CONFIG}");
    println!("cargo:rerun-if-changed={PLATFORM_DIR}");
    println!("cargo:rerun-if-changed={PLATFORM_CONFIG_DIR}");

    let platform = env::var("CARGO_CFG_PLATFORM").expect("Missing platform name");
    assert!(
        platforms.iter().any(|known| known == &platform),
        "Unexpected platform name {platform:?}. Supported platforms: {platforms:?}",
    );

    let config_file = platform_config_file(&platform);
    println!("cargo:rerun-if-changed={}", config_file.display());

    let config = platform_config(&config_file);
    let image_address = config_value(&config, RITM_IMAGE_ADDRESS_KEY, &config_file);
    let payload_address = config_value(&config, PAYLOAD_ADDRESS_KEY, &config_file);
    let payload = payload_config(payload_address);

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR is expected to be set"));
    let platform_linker_script = out_dir.join("platform.ld");
    let platform_module = out_dir.join("platform.rs");
    let payload_source = out_dir.join("payload.rs");

    fs::write(
        &platform_linker_script,
        linker_script(image_address, payload.as_ref()),
    )
    .unwrap_or_else(|e| {
        panic!(
            "Failed to write '{}': {e}",
            platform_linker_script.display()
        )
    });
    fs::write(
        &platform_module,
        platform_module_source(&platforms, image_address, payload_address),
    )
    .unwrap_or_else(|e| panic!("Failed to write '{}': {e}", platform_module.display()));
    fs::write(&payload_source, payload_source_code(payload.as_ref()))
        .unwrap_or_else(|e| panic!("Failed to write '{}': {e}", payload_source.display()));

    println!("cargo:rustc-link-arg=-Timage.ld");
    println!(
        "cargo:rustc-link-arg=-T{}",
        platform_linker_script.display()
    );
}

struct PayloadConfig {
    address: u64,
    path: PathBuf,
    size: u64,
}

fn discover_platforms() -> Vec<String> {
    let mut platforms = fs::read_dir(PLATFORM_CONFIG_DIR)
        .unwrap_or_else(|e| panic!("Failed to read {PLATFORM_CONFIG_DIR}: {e}"))
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("conf") {
                return None;
            }
            Some(path.file_stem()?.to_str()?.to_owned())
        })
        .collect::<Vec<_>>();
    platforms.sort();
    platforms
}

fn platform_config_file(platform: &str) -> PathBuf {
    Path::new(PLATFORM_CONFIG_DIR).join(format!("{platform}.conf"))
}

fn config_value(config: &std::collections::HashMap<String, u64>, key: &str, path: &Path) -> u64 {
    *config
        .get(key)
        .unwrap_or_else(|| panic!("Could not find {key:?} in {}", path.display()))
}

fn payload_config(address: u64) -> Option<PayloadConfig> {
    println!("cargo:rerun-if-env-changed=RITM_PAYLOAD");
    let payload_path = match env::var("RITM_PAYLOAD") {
        Ok(path) if !path.is_empty() => PathBuf::from(path),
        _ => return None,
    };

    println!("cargo:rerun-if-changed={}", payload_path.display());
    let payload_canonical = fs::canonicalize(&payload_path).unwrap_or_else(|e| {
        panic!(
            "Failed to canonicalize payload path '{}': {e}",
            payload_path.display()
        )
    });
    let metadata = fs::metadata(&payload_canonical).unwrap_or_else(|e| {
        panic!(
            "Failed to read metadata for payload '{}': {e}",
            payload_canonical.display()
        )
    });

    let size = metadata.len();
    let size_aligned = size.next_multiple_of(PAYLOAD_SECTION_ALIGNMENT);
    assert!(
        size_aligned <= PAYLOAD_SIZE_LIMIT,
        "Payload '{}' is too large: {size} bytes ({size_aligned} bytes after section alignment) exceeds the {PAYLOAD_SIZE_LIMIT} byte payload region.",
        payload_canonical.display()
    );

    Some(PayloadConfig {
        address,
        path: payload_canonical,
        size,
    })
}

fn linker_script(image_address: u64, payload: Option<&PayloadConfig>) -> String {
    let payload_memory = payload
        .map(|payload| {
            format!(
                "	payload (rwx) : ORIGIN = {:#x}, LENGTH = {}M\n",
                payload.address,
                PAYLOAD_SIZE_LIMIT / 1024 / 1024
            )
        })
        .unwrap_or_default();
    let payload_sections = payload
        .map(|_| {
            "
SECTIONS {
	.payload :
	{
		KEEP(*(.payload));
		. = ALIGN(4);
	} > payload
} INSERT AFTER .text;
"
        })
        .unwrap_or_default();

    format!(
        "RITM_IMAGE_ADDRESS = {image_address:#x};

MEMORY
{{
	image : ORIGIN = RITM_IMAGE_ADDRESS, LENGTH = 4M
{payload_memory}}}
{payload_sections}"
    )
}

fn platform_module_source(
    platforms: &[String],
    image_address: u64,
    payload_address: u64,
) -> String {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is expected");
    let mut output = format!(
        "\
#[allow(clippy::unreadable_literal)]
pub const RITM_IMAGE_ADDRESS: usize = {image_address:#x};
#[allow(clippy::unreadable_literal)]
pub const PAYLOAD_ADDRESS: u64 = {payload_address:#x};
"
    );
    for platform in platforms {
        let path = platform_source_path(&manifest_dir, platform);
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

fn platform_source_path(manifest_dir: &str, platform: &str) -> PathBuf {
    let platform_dir = Path::new(manifest_dir).join(PLATFORM_DIR);
    let direct_path = platform_dir.join(format!("{platform}.rs"));
    if direct_path.exists() {
        direct_path
    } else if platform == "qemu_bl33" {
        platform_dir.join("qemu.rs")
    } else {
        panic!(
            "Platform config {platform:?} has no matching source file '{}'",
            direct_path.display()
        );
    }
}

fn payload_source_code(payload: Option<&PayloadConfig>) -> String {
    if let Some(payload) = payload {
        format!(
            r#"
#[used]
#[unsafe(link_section = ".payload")]
static EMBEDDED_PAYLOAD: [u8; {size}] = *include_bytes!("{path}");
"#,
            size = payload.size,
            path = payload.path.display(),
        )
    } else {
        String::new()
    }
}
