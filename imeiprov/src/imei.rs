// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! IMEI decode, Luhn validation and Qualcomm NV 550 (NV_UE_IMEI) BCD codec.
//!
//! Wire facts reverse-engineered from the stock `tctd` (`imei_build`,
//! `vendor/fairphone/source/system/tctd/trace_nv_read.c`): NV item 550, a
//! 9-byte value `[0x08][ (d1<<4)|0x0A ][ (d3<<4)|d2 ] ... [ (d15<<4)|d14 ]`.
//! The leading nibble 0x0A is the odd-length identity marker; byte 0 (0x08) is
//! the count of IMEI bytes that follow.
//!
//! `Imei` deliberately has no `Display` and a redacting `Debug`, so no code path
//! can print the digits. Construct only from trusted inputs (the traceability
//! partition, a decoded NV read, or synthetic test values).

use core::fmt;

pub const DIGITS: usize = 15;
/// NV item id for the UE IMEI (NV_UE_IMEI_I), confirmed at `imei_build` 0xb648.
pub const NV_ITEM_IMEI: u32 = 550;
/// Encoded NV 550 value length: count byte + 8 packed BCD bytes.
pub const NV550_LEN: usize = 9;
const COUNT_BYTE: u8 = 0x08;
const IDENTITY_NIBBLE: u8 = 0x0A;

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Imei {
    digits: [u8; DIGITS],
}

/// Reasons an IMEI is rejected. Carries no digits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Length,
    NotDigit,
    Luhn,
    Encoding,
}

impl fmt::Debug for Imei {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Never reveal the digits, even through {:?}.
        f.write_str("Imei(<redacted>)")
    }
}

impl Imei {
    /// 15 numeric digit values (0..=9). The only accessor; keep it off any
    /// logging path.
    pub fn digits(&self) -> [u8; DIGITS] {
        self.digits
    }

    /// Build from 15 numeric digit values. Checks range only; use
    /// [`Imei::is_luhn_valid`] for the check-digit test.
    pub fn from_digits(digits: [u8; DIGITS]) -> Result<Self, Error> {
        if digits.iter().any(|&d| d > 9) {
            return Err(Error::NotDigit);
        }
        Ok(Self { digits })
    }

    /// Decode 15 ASCII bytes ('0'..='9'). The traceability partition stores the
    /// IMEI as plain ASCII (stock `start` reads 15 bytes; `imei_build` validates
    /// each is a digit before packing).
    pub fn from_ascii(ascii: &[u8]) -> Result<Self, Error> {
        let bytes: [u8; DIGITS] = ascii.try_into().map_err(|_| Error::Length)?;
        let mut digits = [0u8; DIGITS];
        for (out, &b) in digits.iter_mut().zip(bytes.iter()) {
            if !b.is_ascii_digit() {
                return Err(Error::NotDigit);
            }
            *out = b - b'0';
        }
        Ok(Self { digits })
    }

    /// Luhn check over all 15 digits (the 15th is the check digit). The
    /// Qualcomm placeholder IMEI the unprovisioned modem reports is not
    /// Luhn-valid, so this check also rejects it; no placeholder constant is
    /// embedded (privacy).
    // `% 10 == 0` is kept over `is_multiple_of` for portability across the
    // build's Rust toolchain.
    #[allow(clippy::manual_is_multiple_of)]
    pub fn is_luhn_valid(&self) -> bool {
        let mut sum = 0u32;
        for (i, &d) in self.digits.iter().enumerate() {
            // Double every second digit counting from the right (indices 13,
            // 11, ... for a 15-digit number).
            let doubled = (DIGITS - 1 - i) % 2 == 1;
            let v = if doubled { d * 2 } else { d };
            sum += u32::from(if v > 9 { v - 9 } else { v });
        }
        sum % 10 == 0
    }

    /// Encode the NV 550 value: count byte 0x08, then the packed BCD bytes.
    pub fn to_nv550_bcd(&self) -> [u8; NV550_LEN] {
        let d = &self.digits;
        let pack = |hi: u8, lo: u8| (hi << 4) | (lo & 0x0F);
        [
            COUNT_BYTE,
            pack(d[0], IDENTITY_NIBBLE),
            pack(d[2], d[1]),
            pack(d[4], d[3]),
            pack(d[6], d[5]),
            pack(d[8], d[7]),
            pack(d[10], d[9]),
            pack(d[12], d[11]),
            pack(d[14], d[13]),
        ]
    }

    /// Decode an NV 550 value (the 9 bytes above, trailing padding ignored).
    pub fn from_nv550_bcd(value: &[u8]) -> Result<Self, Error> {
        if value.len() < NV550_LEN {
            return Err(Error::Length);
        }
        if value[0] != COUNT_BYTE || value[1] & 0x0F != IDENTITY_NIBBLE {
            return Err(Error::Encoding);
        }
        let mut digits = [0u8; DIGITS];
        digits[0] = value[1] >> 4;
        for (pair, &byte) in value[2..NV550_LEN].iter().enumerate() {
            digits[1 + pair * 2] = byte & 0x0F;
            digits[2 + pair * 2] = byte >> 4;
        }
        if digits.iter().any(|&d| d > 9) {
            return Err(Error::Encoding);
        }
        Ok(Self { digits })
    }
}

/// Outcome of validating the two traceability IMEIs before any modem write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PairCheck {
    pub slot1_valid: bool,
    pub slot2_valid: bool,
    pub distinct: bool,
}

impl PairCheck {
    /// Both slots pass structure + Luhn and the two IMEIs differ. Only then may
    /// the tool write anything.
    pub fn usable(&self) -> bool {
        self.slot1_valid && self.slot2_valid && self.distinct
    }
}

/// Validate a decoded pair. An IMEI is valid when it has 15 digits (guaranteed
/// by `Imei`) and a correct Luhn check digit; the pair must also be distinct.
pub fn check_pair(slot1: &Imei, slot2: &Imei) -> PairCheck {
    PairCheck {
        slot1_valid: slot1.is_luhn_valid(),
        slot2_valid: slot2.is_luhn_valid(),
        distinct: slot1 != slot2,
    }
}
