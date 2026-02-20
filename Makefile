# Copyright 2025 Google LLC
#
# Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
# https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
# <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
# option. This file may not be copied, modified, or distributed
# except according to those terms.

TARGET := --target aarch64-unknown-none

PAYLOAD ?= payload.bin

QEMU_BIN := target/ritm.qemu.bin
QEMU_RUSTFLAGS := "--cfg platform=\"qemu\""

.PHONY: all build.qemu clean clippy qemu test

all: $(QEMU_BIN)

clippy:
	RITM_PAYLOAD=$(PAYLOAD) RUSTFLAGS=$(QEMU_RUSTFLAGS) cargo clippy $(TARGET)

build.qemu:
	RITM_PAYLOAD=$(PAYLOAD) RUSTFLAGS=$(QEMU_RUSTFLAGS) cargo build $(TARGET)

$(QEMU_BIN): build.qemu
	RITM_PAYLOAD=$(PAYLOAD) RUSTFLAGS=$(QEMU_RUSTFLAGS) cargo objcopy $(TARGET) -- -O binary $@

qemu: $(QEMU_BIN)
	qemu-system-aarch64 -machine virt,virtualization=on,gic-version=3 -cpu cortex-a57 -display none -kernel $< -s \
	  -smp 4 -serial mon:stdio \
	  -global virtio-mmio.force-legacy=false \
	  -drive file=/dev/null,if=none,format=raw,id=x0 \
	  -device virtio-blk-device,drive=x0 \
	  -device virtio-serial,id=virtio-serial0 \
	  -chardev socket,path=/tmp/qemu-console,server=on,wait=off,id=char0,mux=on \
	  -device virtconsole,chardev=char0 \
	  -device vhost-vsock-device,id=virtiosocket0,guest-cid=102

test:
	tests/isolation_test.py

clean:
	cargo clean
	rm -f target/*.bin
