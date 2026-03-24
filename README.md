# ritm

A Google-delivered open-source reference implementation for enabling
hypervisor policy enforcement on Android devices with unlocked bootloaders.

This is not an officially supported Google product. This project is not
eligible for the [Google Open Source Software Vulnerability Rewards
Program](https://bughunters.google.com/open-source-security).

## Overview

This project provides an "EL2 shim" designed to address the challenge
of enforcing vendor hypervisor policies when a device's bootloader 
is unlocked. On such devices, the standard Protected KVM (pKVM) hypervisor
might be replaced by the user, potentially bypassing security policies.

This shim operates at EL2 (a higher privilege level than the Android OS 
kernel) and adapts its behavior based on the bootloader's lock state:

* **Bootloader Locked:** The shim disables stage-2 protection and enters 
  the kernel at EL2, handing over control to the standard hypervisor 
  (e.g., pKVM).
* **Bootloader Unlocked:** The shim **keeps** stage-2 memory protection 
  enabled and enters the kernel at EL1. This enforces basic hypervisor
  memory protection policies even when the device is unlocked.

## Who is this for?

This reference implementation is intended for **System on a Chip (SOC) vendors**
and **Original Equipment Manufacturers (OEMs)** to incorporate into the
bootloader flow of their devices.

## Building

To build the project for QEMU, you need to provide the path to the kernel
image payload (e.g., a Linux kernel `Image`).

```bash
make build.qemu PAYLOAD=/path/to/your/linux/Image
```

If you do not specify `PAYLOAD`, it defaults to looking for a file named
`payload.bin` in the root directory.

## Platforms

### Supported

#### QEMU

The reference environment is **QEMU (aarch64 `virt` machine)**
(`src/platform/qemu.rs`). It uses a PL011 UART console, relies on a static
early-boot memory map, and patches the Device Tree (FDT) to hide `ritm`'s
memory from the payload.

To choose between booting the payload in EL1 and EL2, the `ritm.boot_mode`
command line option can be used:

```bash
qemu-system-aarch64 -append "ritm.boot_mode=el1" ...
# or:
qemu-system-aarch64 -append "ritm.boot_mode=el2" ...
```

By default, the payload is booted in EL1.

### Customization

#### Modifying an existing platform

Adjust device addresses, pagetables, or FDT logic in the platform's source
file and update its linker script. See `src/platform/qemu.rs` and
`linker/qemu.ld` for the reference example.

### How to add a new platform

1. Implement the `Platform` trait (`src/platform.rs`) in a new module
   to provide the console, early pagetable, and boot configuration.
2. Conditionally export it via `#[cfg(platform = "...")]`.
3. Provide a memory layout linker script in `linker/`.
4. Register the new platform in `build.rs` and the `Makefile`.

## License

This software is distributed under the terms of both the MIT license and the
Apache License (Version 2.0).

See LICENSE for details.

## Contributing

If you want to contribute to the project, see details of
[how we accept contributions](CONTRIBUTING.md).
