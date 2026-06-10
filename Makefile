# Copyright 2025 Google LLC
#
# Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
# https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
# <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
# option. This file may not be copied, modified, or distributed
# except according to those terms.

TARGET := --target aarch64-unknown-none
PLATFORM ?= qemu
PAYLOAD ?=

BIN := target/ritm.$(PLATFORM).bin
ELF := target/aarch64-unknown-none/debug/ritm
RUSTFLAGS_PLATFORM := --cfg platform="$(PLATFORM)"
BUILD_ENV := RUSTFLAGS='$(RUSTFLAGS_PLATFORM)'

ifneq ($(strip $(PAYLOAD)),)
BUILD_ENV += RITM_PAYLOAD='$(abspath $(PAYLOAD))'
endif

.PHONY: all build clean clippy clippy-fix qemu test

all: $(BIN)

clippy:
	$(BUILD_ENV) cargo clippy $(TARGET)

clippy-fix:
	$(BUILD_ENV) cargo clippy --fix $(TARGET)

build:
	$(BUILD_ENV) cargo build $(TARGET)

$(BIN): build
	$(BUILD_ENV) cargo objcopy $(TARGET) -- -O binary $@

# RITM does not configure SVE or pointer authentication state for EL1 guests yet.
qemu: build
	@test -n "$(strip $(PAYLOAD))" || { echo "PAYLOAD is required for make qemu"; exit 2; }
	qemu-system-aarch64 -machine virt,virtualization=on,gic-version=3 -cpu cortex-a57 -display none -net none -kernel $(ELF) -s \
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
