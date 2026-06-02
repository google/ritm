# Copyright 2025 Google LLC
#
# Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
# https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
# <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
# option. This file may not be copied, modified, or distributed
# except according to those terms.

TARGET := --target aarch64-unknown-none
PLATFORM ?= qemu

PAYLOAD ?= payload.bin

BIN := target/ritm.$(PLATFORM).bin
RUSTFLAGS_PLATFORM := --cfg platform="$(PLATFORM)"
BUILD_ENV := RUSTFLAGS='$(RUSTFLAGS_PLATFORM)'

.PHONY: all build clean clippy clippy-fix qemu test

all: $(BIN)

clippy:
	RITM_PAYLOAD=/dev/null $(BUILD_ENV) cargo clippy $(TARGET)

clippy-fix:
	RITM_PAYLOAD=/dev/null $(BUILD_ENV) cargo clippy --fix $(TARGET)

build:
	RITM_PAYLOAD=$(PAYLOAD) $(BUILD_ENV) cargo build $(TARGET)

$(BIN): build
	RITM_PAYLOAD=$(PAYLOAD) $(BUILD_ENV) cargo objcopy $(TARGET) -- -O binary $@

qemu: $(BIN)
	qemu-system-aarch64 -machine virt,virtualization=on,gic-version=3 -cpu cortex-a57 -display none -kernel $< -s \
	  -smp 4 -serial mon:stdio \
	  -global virtio-mmio.force-legacy=false \
	  -drive file=/dev/null,if=none,format=raw,id=x0 \
	  -device virtio-blk-device,drive=x0 \
	  -device virtio-serial,id=virtio-serial0 \
	  -chardev socket,path=/tmp/qemu-console,server=on,wait=off,id=char0,mux=on \
	  -device virtconsole,chardev=char0 \
	  -device vhost-vsock-device,id=virtiosocket0,guest-cid=102 \
	  -append "ritm.boot_mode=el1"

test:
	tests/integration_test.py
	tests/psci_test.py

clean:
	cargo clean
	rm -f target/*.bin
