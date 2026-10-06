// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! The factory Bluetooth address from the `traceability` partition.
//!
//! Stock `tctd` (0xb3cc-0xb4ac) reads 6 bytes at offset 0x33, least
//! significant byte first, prints them in reverse as `%02x:%02x:...` for
//! `ro.vendor.trace.btmac`, and its rc copies that to
//! `ro.vendor.bt.boot.macaddr`, the property the Qualcomm HCI implementation
//! reads before its generated-address fallback. Stock checks nothing.
//!
//! [`BdAddr`] has no `Display` and a redacting `Debug`; the only way out is
//! [`BdAddr::property_value`], which goes straight into the property.

use crate::eui48::{self, Fault};
use core::fmt;

/// Offset of the address in the traceability partition (right after the slot 1
/// IMEI at 0x24..0x33).
pub const TRACE_OFFSET: u64 = 0x33;
/// Bytes of a Bluetooth device address.
pub const LEN: usize = eui48::LEN;
/// Length of the property value, `xx:xx:xx:xx:xx:xx`. The HCI implementation
/// accepts only exactly this length.
pub const PROPERTY_LEN: usize = 17;

/// A validated, universally administered unicast address, most significant
/// byte first.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct BdAddr {
    bytes: [u8; LEN],
}

/// Why an address is rejected. Carries no address bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Not exactly 6 bytes.
    Length,
    /// All zero: never written.
    Zero,
    /// All 0xff: erased flash, or the broadcast address.
    Erased,
    /// Group (multicast) bit set: never a device address.
    Group,
    /// Locally administered bit set: not a factory address. Also rejects
    /// the HCI implementation's own generated prefix (22:22).
    Local,
    /// Lower 24 bits (the LAP) in 0x9E8B00..=0x9E8B3F, which the Bluetooth
    /// Core specification (Vol 2, Part B, 1.2) reserves for inquiry access
    /// codes and forbids in a device address.
    ReservedLap,
}

impl fmt::Debug for BdAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("BdAddr(<redacted>)")
    }
}

impl BdAddr {
    /// Decode the 6 bytes as stored in traceability (least significant first)
    /// and validate them.
    pub fn from_trace(raw: &[u8]) -> Result<Self, Error> {
        let bytes = eui48::from_stored(raw)?;
        validate(&bytes)?;
        Ok(Self { bytes })
    }

    /// The property value as stock writes it: lowercase hex, colon-separated,
    /// most significant byte first.
    pub fn property_value(&self) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut out = String::with_capacity(PROPERTY_LEN);
        for (i, b) in self.bytes.iter().enumerate() {
            if i > 0 {
                out.push(':');
            }
            out.push(HEX[usize::from(b >> 4)] as char);
            out.push(HEX[usize::from(b & 0x0f)] as char);
        }
        out
    }
}

impl From<Fault> for Error {
    fn from(fault: Fault) -> Self {
        match fault {
            Fault::Length => Error::Length,
            Fault::Zero => Error::Zero,
            Fault::Erased => Error::Erased,
            Fault::Group => Error::Group,
            Fault::Local => Error::Local,
        }
    }
}

/// Checks an address given most significant byte first. Stock does no checks;
/// these reject only values that cannot be a factory-assigned device address:
/// the EUI-48 checks shared with the Wi-Fi MAC (`eui48::check`) plus the
/// reserved inquiry LAPs.
pub fn validate(bytes: &[u8; LEN]) -> Result<(), Error> {
    eui48::check(bytes)?;
    let lap = u32::from_be_bytes([0, bytes[3], bytes[4], bytes[5]]);
    if (0x9E_8B00..=0x9E_8B3F).contains(&lap) {
        return Err(Error::ReservedLap);
    }
    Ok(())
}
