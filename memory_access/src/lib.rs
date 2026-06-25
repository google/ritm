// Copyright 2026 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![no_std]

/// Width of a trapped guest memory access.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryAccessWidth {
    /// 8-bit access.
    U8,
    /// 16-bit access.
    U16,
    /// 32-bit access.
    U32,
    /// 64-bit access.
    U64,
}

impl MemoryAccessWidth {
    /// Returns the width in bits.
    #[must_use]
    pub const fn bits(self) -> u32 {
        match self {
            Self::U8 => 8,
            Self::U16 => 16,
            Self::U32 => 32,
            Self::U64 => 64,
        }
    }

    /// Returns the value mask for this access width.
    #[must_use]
    pub const fn mask(self) -> u64 {
        match self {
            Self::U8 => u8::MAX as u64,
            Self::U16 => u16::MAX as u64,
            Self::U32 => u32::MAX as u64,
            Self::U64 => u64::MAX,
        }
    }
}

/// Decoded representation of a trapped guest memory access.
pub struct DecodedMemoryAccess {
    /// The faulting intermediate physical address.
    pub ipa: u64,
    /// The width of the guest access.
    pub width: MemoryAccessWidth,
    /// Whether the access was a read or write.
    pub kind: DecodedMemoryAccessKind,
    /// The general-purpose register encoded in the syndrome.
    pub register_index: usize,
    /// Whether the read result should be sign-extended.
    pub sign_extend: bool,
    /// Whether the target register is 64-bit wide.
    pub register_width_64: bool,
}

impl DecodedMemoryAccess {
    /// Extends an emulated read value according to the decoded access and target register width.
    #[must_use]
    pub fn extend_read_result(&self, value: u64) -> u64 {
        extend_read_result(value, self.width, self.sign_extend, self.register_width_64)
    }
}

/// Decoded read or write direction for a trapped memory access.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecodedMemoryAccessKind {
    /// Guest read from memory into a register.
    Read,
    /// Guest write from a register to memory.
    Write {
        /// Value written by the guest, masked to the access width.
        value: u64,
    },
}

// ESR_EL2.ISS fields for a Data Abort exception with a valid instruction syndrome.
//
// See the Arm ESR_EL2 register documentation:
// https://developer.arm.com/documentation/ddi0601/latest/AArch64-Registers/ESR-EL2--Exception-Syndrome-Register--EL2-
const DATA_ABORT_ISS_ISV: u32 = 1 << 24;
const DATA_ABORT_ISS_SAS_SHIFT: u32 = 22;
const DATA_ABORT_ISS_SAS_MASK: u32 = 0b11;
const DATA_ABORT_ISS_SSE: u32 = 1 << 21;
const DATA_ABORT_ISS_SRT_SHIFT: u32 = 16;
const DATA_ABORT_ISS_SRT_MASK: u32 = 0b1_1111;
const DATA_ABORT_ISS_SF: u32 = 1 << 15;
const DATA_ABORT_ISS_WNR: u32 = 1 << 6;

// HPFAR_EL2.FIPA contains the faulting IPA above the 4 KiB page offset. The low
// offset bits come from FAR_EL2.
//
// See the Arm HPFAR_EL2 and FAR_EL2 register documentation:
// https://developer.arm.com/documentation/ddi0601/latest/AArch64-Registers/HPFAR-EL2--Hypervisor-IPA-Fault-Address-Register
// https://developer.arm.com/documentation/ddi0601/latest/AArch64-Registers/FAR-EL2--Fault-Address-Register--EL2-
const FAULT_IPA_PAGE_SHIFT: u64 = 12;
const FAULT_IPA_PAGE_OFFSET_MASK: u64 = (1 << FAULT_IPA_PAGE_SHIFT) - 1;

/// Decodes a Data Abort instruction syndrome into a guest memory access.
///
/// Returns `None` when the syndrome does not include enough information to emulate the access or
/// when a write access needs a saved register value that `read_register` cannot provide.
pub fn decode_memory_access(
    iss: u32,
    hpfar_fipa: u64,
    far_va: u64,
    mut read_register: impl FnMut(usize) -> Option<u64>,
) -> Option<DecodedMemoryAccess> {
    // Keep emulation syndrome-only: without ISV, ISS does not describe a GPR transfer well
    // enough to handle the abort without decoding the trapped instruction.
    if !decode_valid_instruction_syndrome(iss) {
        return None;
    }

    let width = decode_memory_access_width(iss);
    let register_index = decode_memory_access_register_index(iss);
    let kind = decode_memory_access_kind(iss, register_index, width, &mut read_register)?;

    Some(DecodedMemoryAccess {
        ipa: decode_fault_ipa(hpfar_fipa, far_va),
        width,
        kind,
        register_index,
        sign_extend: decode_memory_access_sign_extend(iss),
        register_width_64: decode_memory_access_register_width_64(iss),
    })
}

#[must_use]
fn extend_read_result(
    value: u64,
    width: MemoryAccessWidth,
    sign_extend: bool,
    register_width_64: bool,
) -> u64 {
    if sign_extend && width != MemoryAccessWidth::U64 {
        let shift = 64 - width.bits();
        let extended = ((value & width.mask()) << shift).cast_signed() >> shift;
        if register_width_64 {
            extended.cast_unsigned()
        } else {
            extended.cast_unsigned() & u64::from(u32::MAX)
        }
    } else if register_width_64 {
        value & width.mask()
    } else {
        value & width.mask() & u64::from(u32::MAX)
    }
}

fn decode_valid_instruction_syndrome(iss: u32) -> bool {
    (iss & DATA_ABORT_ISS_ISV) != 0
}

fn decode_memory_access_width(iss: u32) -> MemoryAccessWidth {
    match (iss >> DATA_ABORT_ISS_SAS_SHIFT) & DATA_ABORT_ISS_SAS_MASK {
        0 => MemoryAccessWidth::U8,
        1 => MemoryAccessWidth::U16,
        2 => MemoryAccessWidth::U32,
        3 => MemoryAccessWidth::U64,
        _ => unreachable!(),
    }
}

fn decode_memory_access_register_index(iss: u32) -> usize {
    ((iss >> DATA_ABORT_ISS_SRT_SHIFT) & DATA_ABORT_ISS_SRT_MASK) as usize
}

fn decode_memory_access_kind(
    iss: u32,
    register_index: usize,
    width: MemoryAccessWidth,
    read_register: &mut impl FnMut(usize) -> Option<u64>,
) -> Option<DecodedMemoryAccessKind> {
    if decode_memory_access_is_write(iss) {
        let value = read_register(register_index)? & width.mask();
        Some(DecodedMemoryAccessKind::Write { value })
    } else {
        Some(DecodedMemoryAccessKind::Read)
    }
}

fn decode_memory_access_is_write(iss: u32) -> bool {
    (iss & DATA_ABORT_ISS_WNR) != 0
}

fn decode_memory_access_sign_extend(iss: u32) -> bool {
    (iss & DATA_ABORT_ISS_SSE) != 0
}

fn decode_memory_access_register_width_64(iss: u32) -> bool {
    (iss & DATA_ABORT_ISS_SF) != 0
}

fn decode_fault_ipa(hpfar_fipa: u64, far_va: u64) -> u64 {
    (hpfar_fipa << FAULT_IPA_PAGE_SHIFT) | (far_va & FAULT_IPA_PAGE_OFFSET_MASK)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn iss(width: MemoryAccessWidth, register_index: u32, flags: u32) -> u32 {
        let sas = match width {
            MemoryAccessWidth::U8 => 0,
            MemoryAccessWidth::U16 => 1,
            MemoryAccessWidth::U32 => 2,
            MemoryAccessWidth::U64 => 3,
        };
        DATA_ABORT_ISS_ISV
            | (sas << DATA_ABORT_ISS_SAS_SHIFT)
            | ((register_index & DATA_ABORT_ISS_SRT_MASK) << DATA_ABORT_ISS_SRT_SHIFT)
            | flags
    }

    fn decode_with_register(iss: u32, register_value: u64) -> Option<DecodedMemoryAccess> {
        decode_memory_access(iss, 0x12345, 0xffff_0000_0000_0abc, |index| {
            assert_eq!(index, 7);
            Some(register_value)
        })
    }

    fn read_access(
        width: MemoryAccessWidth,
        sign_extend: bool,
        register_width_64: bool,
    ) -> DecodedMemoryAccess {
        DecodedMemoryAccess {
            ipa: 0,
            width,
            kind: DecodedMemoryAccessKind::Read,
            register_index: 0,
            sign_extend,
            register_width_64,
        }
    }

    #[test]
    fn decode_rejects_missing_instruction_syndrome() {
        assert!(decode_memory_access(0, 0x12345, 0xabc, |_| Some(0)).is_none());
    }

    #[test]
    fn decode_read_access_from_syndrome_fields() {
        let access = decode_memory_access(
            iss(
                MemoryAccessWidth::U32,
                9,
                DATA_ABORT_ISS_SSE | DATA_ABORT_ISS_SF,
            ),
            0x12345,
            0xffff_0000_0000_0abc,
            |_| panic!("read access should not read a saved register"),
        )
        .expect("access should decode");

        assert_eq!(access.ipa, 0x1234_5abc);
        assert_eq!(access.width, MemoryAccessWidth::U32);
        assert_eq!(access.kind, DecodedMemoryAccessKind::Read);
        assert_eq!(access.register_index, 9);
        assert!(access.sign_extend);
        assert!(access.register_width_64);
    }

    #[test]
    fn decode_write_access_masks_register_value_to_access_width() {
        let access = decode_with_register(
            iss(MemoryAccessWidth::U16, 7, DATA_ABORT_ISS_WNR),
            0xffff_ffff_ffff_1234,
        )
        .expect("access should decode");

        assert_eq!(
            access.kind,
            DecodedMemoryAccessKind::Write { value: 0x1234 }
        );
        assert_eq!(access.width, MemoryAccessWidth::U16);
        assert_eq!(access.register_index, 7);
    }

    #[test]
    fn decode_write_access_rejects_unavailable_register_value() {
        assert!(
            decode_memory_access(
                iss(MemoryAccessWidth::U64, 4, DATA_ABORT_ISS_WNR),
                0x12345,
                0xabc,
                |_| None,
            )
            .is_none()
        );
    }

    #[test]
    fn extend_read_result_zero_extends_to_32_bit_registers() {
        assert_eq!(
            read_access(MemoryAccessWidth::U32, false, false).extend_read_result(0x1_ffff_ffff),
            0xffff_ffff
        );
        assert_eq!(
            read_access(MemoryAccessWidth::U16, false, false).extend_read_result(0x1234),
            0x1234
        );
    }

    #[test]
    fn extend_read_result_sign_extends_to_requested_register_width() {
        assert_eq!(
            read_access(MemoryAccessWidth::U8, true, true).extend_read_result(0x80),
            0xffff_ffff_ffff_ff80
        );
        assert_eq!(
            read_access(MemoryAccessWidth::U8, true, false).extend_read_result(0x80),
            0xffff_ff80
        );
    }
}
