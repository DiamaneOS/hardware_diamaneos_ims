// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! Structural observations only, without subscriber data or measurement identity.
//! Not a profile registry, quality result, or modem operation.
use diamaneos_ims_dcm::protocol::{Frame, Kind};

const INITIALIZE: u16 = 0x45;
const SELECT: u16 = 0x3f;
const KNOWN_SELECTION_BITS: u64 = 0xffff_ffff_0001_03f8;

/// Non-identifying facts from one validated notification. There is deliberately
/// no measurement key: observations cannot establish which profiles are active.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notice {
    Initialized(u8),
    Selected(Selection),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    known: u64,
    unmapped: bool,
}

impl Selection {
    /// A requested type in this message, not proof that it was initialized or
    /// accepted. Different opaque measurement contexts are intentionally erased.
    pub fn contains(self, profile: u8) -> bool {
        let bit = match profile {
            4..=35 => profile + 28,
            36..=42 => profile - 33,
            43 => 16,
            _ => return false,
        };
        self.known & (1_u64 << bit) != 0
    }

    /// Other bits have no established meaning. Do not infer profile names from them.
    pub fn has_unmapped_bits(self) -> bool {
        self.unmapped
    }
}

impl Notice {
    /// Caller must validate the current QRTR peer and bound subscription first.
    /// Only type/selection facts survive this call. Optional SIM bytes, thresholds
    /// and opaque measurement IDs are checked for wire shape, never copied,
    /// decoded or retained. This does not validate their semantic values.
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        // Check the largest stock message bound before parsing TLVs.
        if bytes.len() > 69 {
            return None;
        }
        let frame = Frame::parse(bytes).ok()?;
        if frame.kind != Kind::Indication {
            return None;
        }
        let required = frame.tlv(1)?;
        match frame.id {
            INITIALIZE if required.len() == 4 => {
                if !fields_valid(bytes, INITIALIZE) {
                    return None;
                }
                let profile = u32::from_le_bytes(required.try_into().ok()?);
                (4..=43)
                    .contains(&profile)
                    .then_some(Self::Initialized(profile as u8))
            }
            SELECT if bytes.len() <= 43 && required.len() == 8 => {
                if !fields_valid(bytes, SELECT) {
                    return None;
                }
                let mask = u64::from_le_bytes(required.try_into().ok()?);
                Some(Self::Selected(Selection {
                    known: mask & KNOWN_SELECTION_BITS,
                    unmapped: mask & !KNOWN_SELECTION_BITS != 0,
                }))
            }
            _ => None,
        }
    }
}

// Frame::parse already validated lengths/duplicates. Walk its bounded body once
// to reject unknown fields and wrong widths without allocating or retaining data.
fn fields_valid(bytes: &[u8], message: u16) -> bool {
    let mut rest = &bytes[7..];
    while !rest.is_empty() {
        let tag = rest[0];
        let length = usize::from(u16::from_le_bytes([rest[1], rest[2]]));
        let value = &rest[3..3 + length];
        let valid = match (message, tag) {
            (INITIALIZE, 1) => length == 4,
            (INITIALIZE, 0x10 | 0x11 | 0x14..=0x17) => length == 2,
            (INITIALIZE, 0x13) | (SELECT, 0x11) => length == 8,
            (SELECT, 1) => length == 8,
            (INITIALIZE, 0x12) | (SELECT, 0x10) => value
                .first()
                .is_some_and(|n| *n <= 10 && usize::from(*n) + 1 == length),
            _ => false,
        };
        if !valid {
            return false;
        }
        rest = &rest[3 + length..];
    }
    true
}
