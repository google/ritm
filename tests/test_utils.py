#!/usr/bin/env python3

# Copyright 2026 Google LLC
#
# Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
# https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
# <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
# option. This file may not be copied, modified, or distributed
# except according to those terms.

import subprocess
import threading
from pathlib import Path

PROJECT_ROOT = Path(__file__).parent.parent.resolve()

def run_command(cmd, cwd=None, env=None, check=True):
    print(f"Running: {' '.join(str(c) for c in cmd)}")
    return subprocess.run(cmd, cwd=cwd, env=env, check=check, text=True)


def run_qemu_test(cmd, cwd, success_string, failure_string, timeout=30, env=None):
    """
    Run a QEMU test and look for success/failure strings in its output.
    """
    process = subprocess.Popen(
        cmd,
        cwd=cwd,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        bufsize=1,
        env=env
    )

    passed = False

    def read_output():
        nonlocal passed
        for line in process.stdout:
            print(f"QEMU: {line.strip()}")

            if failure_string in line:
                print(f"[FAIL] Found failure string: {failure_string}")
                return

            if success_string in line:
                print("[PASS] Test completed successfully.")
                passed = True
                return


    thread = threading.Thread(target=read_output)
    thread.daemon = True
    thread.start()

    thread.join(timeout)

    if thread.is_alive():
        print("Test timed out.")
    elif not passed and process.poll() is not None:
        print("QEMU exited unexpectedly before test completion.")

    if process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=1)
        except subprocess.TimeoutExpired:
            process.kill()

    return passed
