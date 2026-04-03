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
from pathlib import Path

sys.path.append(str(Path(__file__).parent))
import test_utils

TEST_DIR = test_utils.PROJECT_ROOT / "tests" / "integration_test"
PAYLOAD_BIN = test_utils.PROJECT_ROOT / "target" / "aarch64-unknown-none" / "release" / "integration_test.bin"

def main():
    print("Building integration_test payload...")
    env = os.environ.copy()
    test_utils.run_command(
        ["cargo", "build", "--release", "--locked", "--target", "aarch64-unknown-none", "-p", "integration_test"],
        cwd=TEST_DIR,
        env=env
    )

    print("Creating payload binary...")
    test_utils.run_command(
        ["cargo", "objcopy", "--target", "aarch64-unknown-none", "-p", "integration_test", "--", "-O", "binary", str(PAYLOAD_BIN)],
        cwd=test_utils.PROJECT_ROOT
    )

    print("Building RITM with payload...")
    test_utils.run_command(
        ["make", "build.qemu", f"PAYLOAD={PAYLOAD_BIN}"],
        cwd=test_utils.PROJECT_ROOT
    )

    print("Running QEMU integration test...")
    cmd = ["make", "qemu", f"PAYLOAD={PAYLOAD_BIN}"]

    passed = test_utils.run_qemu_test(
        cmd=cmd,
        cwd=test_utils.PROJECT_ROOT,
        success_string="TEST: All tests passed!",
        failure_string="PANIC",
        timeout=30,
        env=env
    )

    if not passed:
        sys.exit(1)

if __name__ == "__main__":
    main()
