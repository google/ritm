#!/usr/bin/env python3

# Copyright 2026 Google LLC
#
# Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
# https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
# <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
# option. This file may not be copied, modified, or distributed
# except according to those terms.

import subprocess
import sys
import os
import time
from pathlib import Path

PROJECT_ROOT = Path(__file__).parent.parent.resolve()
TEST_DIR = PROJECT_ROOT / "tests" / "isolation_test"
PAYLOAD_ELF = PROJECT_ROOT / "target" / "aarch64-unknown-none" / "release" / "isolation_test"
PAYLOAD_BIN = PROJECT_ROOT / "target" / "aarch64-unknown-none" / "release" / "isolation_test.bin"

def run_command(cmd, cwd=None, env=None, check=True):
    print(f"Running: {' '.join(str(c) for c in cmd)}")
    result = subprocess.run(cmd, cwd=cwd, env=env, text=True)
    if check and result.returncode != 0:
        print(f"Error running command: {cmd}")
        sys.exit(1)
    return result

def main():
    print("Building isolation_test payload...")
    env = os.environ.copy()
    run_command(
        ["cargo", "build", "--release", "--locked", "--target", "aarch64-unknown-none", "-p", "isolation_test"],
        cwd=TEST_DIR,
        env=env
    )

    print("Creating payload binary...")
    run_command(
        ["cargo", "objcopy", "--target", "aarch64-unknown-none", "-p", "isolation_test", "--", "-O", "binary", str(PAYLOAD_BIN)],
        cwd=PROJECT_ROOT
    )

    print("Building RITM with payload...")
    run_command(
        ["make", "build.qemu", f"PAYLOAD={PAYLOAD_BIN}"],
        cwd=PROJECT_ROOT
    )

    print("Running QEMU integration test...")
    cmd = ["make", "qemu", f"PAYLOAD={PAYLOAD_BIN}"]

    process = subprocess.Popen(
        cmd,
        cwd=PROJECT_ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        bufsize=1, # Line buffered
        env=env
    )

    success_string = "Caught expected Data Abort! Isolation test passed."
    failure_string = "FAILED"
    start_time = time.time()
    timeout = 30 # seconds

    passed = False

    try:
        while True:
            if process.poll() is not None:
                print("QEMU exited unexpectedly.")
                break

            line = process.stdout.readline()
            if line:
                print(f"QEMU: {line.strip()}")
                if success_string in line:
                    print("[PASS] integration_test")
                    passed = True
                    break
                if failure_string in line:
                    print("[FAIL] integration_test")
                    break

            if time.time() - start_time > timeout:
                print("Test timed out.")
                break

    finally:
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=1)
            except subprocess.TimeoutExpired:
                process.kill()

    if not passed:
        sys.exit(1)

if __name__ == "__main__":
    main()
