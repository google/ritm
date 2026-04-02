#!/usr/bin/env python3

# Copyright 2026 Google LLC
#
# Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
# https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
# <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
# option. This file may not be copied, modified, or distributed
# except according to those terms.

import os
import sys
import shutil
from pathlib import Path

sys.path.append(str(Path(__file__).parent))
import test_utils

TEST_DIR = test_utils.PROJECT_ROOT / "tests" / "psci_test"
PAYLOAD_BIN = test_utils.PROJECT_ROOT / "target" / "aarch64-unknown-none" / "release" / "psci_test.bin"
QEMU_DIR = test_utils.PROJECT_ROOT / "target" / "qemu_run"

CACHE_DIR = test_utils.PROJECT_ROOT / ".test_cache"
TFA_DIR = CACHE_DIR / "trusted-firmware-a"
RFA_DIR = CACHE_DIR / "rusted-firmware-a"

TFA_REVISION = "8dae0862c502e08568a61a1050091fa9357f1240"
RFA_REVISION = "47ebfc5caa5fc49924ff51868b696099f7f889b6"


def main():
    env = os.environ.copy()
    test_utils.run_command(["cargo", "build", "--release", "--locked", "--target", "aarch64-unknown-none", "-p", "psci_test"], cwd=test_utils.PROJECT_ROOT, env=env)
    test_utils.run_command(["cargo", "objcopy", "--target", "aarch64-unknown-none", "-p", "psci_test", "--", "-O", "binary", str(PAYLOAD_BIN)], cwd=test_utils.PROJECT_ROOT)
    test_utils.run_command(["make", "target/ritm.qemu_bl33.bin", f"PAYLOAD={PAYLOAD_BIN}"], cwd=test_utils.PROJECT_ROOT)

    os.makedirs(test_utils.PROJECT_ROOT / "target" / "qemu_run", exist_ok=True)
    shutil.copy(test_utils.PROJECT_ROOT / "target" / "ritm.qemu_bl33.bin", test_utils.PROJECT_ROOT / "target" / "qemu_run" / "bl33.bin")

    clone_repos()
    build_firmware(env)

    cmd = [
        "qemu-system-aarch64",
        "-machine", "virt,gic-version=3,secure=on,virtualization=on",
        "-cpu", "max",
        "-m", "1024M",
        "-nographic",
        "-bios", "bl1.bin",
        "-smp", "4",
        "-semihosting-config", "enable=on,target=native"
    ]

    passed = test_utils.run_qemu_test(
        cmd=cmd,
        cwd=QEMU_DIR,
        success_string="TEST: All tests passed!",
        failure_string="PANIC",
        timeout=30,
        env=env
    )

    if not passed:
        sys.exit(1)


def clone_repos():
    CACHE_DIR.mkdir(exist_ok=True)
    if not TFA_DIR.exists():
        test_utils.run_command(["git", "clone", "--revision", TFA_REVISION, "https://review.trustedfirmware.org/TF-A/trusted-firmware-a", str(TFA_DIR)])
    if not RFA_DIR.exists():
        test_utils.run_command(["git", "clone", "--revision", RFA_REVISION, "https://review.trustedfirmware.org/RF-A/rusted-firmware-a", str(RFA_DIR)])


def build_firmware(env):
    # Patch RF-A to pass the Device Tree in x0
    rfa_platform_file = RFA_DIR / "src" / "platform" / "qemu.rs"
    with open(rfa_platform_file, "r") as f:
        content = f.read()

    old_handover = 'EntryPointInfo {\n            pc: 0x6000_0000,\n            args: Default::default(),\n        }'
    new_handover = 'EntryPointInfo {\n            pc: 0x6000_0000,\n            args: [0x4000_0000, 0, 0, 0, 0, 0, 0, 0],\n        }'
    if old_handover in content:
        content = content.replace(old_handover, new_handover)
        with open(rfa_platform_file, "w") as f:
            f.write(content)

    rfa_env = env.copy()
    rfa_env["TFA"] = str(TFA_DIR)
    rfa_env["CARGO"] = "cargo"

    # As per build-and-run.sh in RF-A:
    test_utils.run_command(
        [
            "make",
            "-C", str(TFA_DIR),
            "PLAT=qemu",
            "DEBUG=1",
            "CC=clang",
            "QEMU_USE_GIC_DRIVER=QEMU_GICV3",
            "NEED_BL32=yes",
            "NEED_BL31=no",
            "bl1", "bl2"
        ],
        cwd=RFA_DIR,
        env=rfa_env
    )
    test_utils.run_command(["make", "PLAT=qemu", "DEBUG=1", "CARGO=cargo", "all"], cwd=RFA_DIR, env=rfa_env)

    QEMU_DIR.mkdir(parents=True, exist_ok=True)

    shutil.copy(TFA_DIR / "build" / "qemu" / "debug" / "bl1.bin", QEMU_DIR / "bl1.bin")
    shutil.copy(TFA_DIR / "build" / "qemu" / "debug" / "bl2.bin", QEMU_DIR / "bl2.bin")
    shutil.copy(RFA_DIR / "target" / "bl31.bin", QEMU_DIR / "bl31.bin")
    shutil.copy(RFA_DIR / "target" / "bl32.bin", QEMU_DIR / "bl32.bin")
    shutil.copy(test_utils.PROJECT_ROOT / "target" / "ritm.qemu_bl33.bin", QEMU_DIR / "bl33.bin")


if __name__ == "__main__":
    main()
