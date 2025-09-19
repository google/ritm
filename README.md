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

## License

This software is distributed under the terms of both the MIT license and the
Apache License (Version 2.0).

See LICENSE for details.

## Contributing

If you want to contribute to the project, see details of
[how we accept contributions](CONTRIBUTING.md).
