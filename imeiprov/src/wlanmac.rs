// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! The factory Wi-Fi MAC from the `traceability` partition, as the WLAN
//! driver's MAC file.
//!
//! Stock `tctd` reads 6 bytes at offset 0x39 (right after the Bluetooth
//! address), least significant byte first, and sets `ro.vendor.trace.wifimac`.
//! Stock `setwlanmac.sh` (a late_start service) writes it to
//! `/mnt/vendor/persist/qca6750/wlan_mac.bin` as `Intf0MacAddress=<12 hex>`
//! and `END`, one per line (33 bytes). The qcacld driver
//! (`hdd_update_mac_config`, with `read_mac_addr_from_mac_file=1` in its
//! configuration) requests that file through the firmware loader as
//! `wlan/qca_cld/qca6750/wlan_mac.bin`, uses `Intf0` as the station's
//! hardware address and generates the addresses of its other interfaces.
//! Stock checks nothing.
//!
//! [`WlanMac`] has no `Display` and a redacting `Debug`; the only way out is
//! [`WlanMac::mac_file`], which goes straight into the file.

use crate::eui48;
use core::fmt;

/// Offset of the MAC in the traceability partition (right after the
/// Bluetooth address at 0x33..0x39).
pub const TRACE_OFFSET: u64 = 0x39;
/// Bytes of the MAC.
pub const LEN: usize = eui48::LEN;
/// Length of the MAC file as stock writes it: `Intf0MacAddress=` (16), 12 hex
/// digits, a newline, then `END` and a newline.
pub const FILE_LEN: usize = 33;

/// Why a MAC is rejected (the EUI-48 checks). Carries no address bytes.
pub use crate::eui48::Fault as Error;

/// A validated, universally administered unicast MAC, most significant byte
/// first.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct WlanMac {
    bytes: [u8; LEN],
}

impl fmt::Debug for WlanMac {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WlanMac(<redacted>)")
    }
}

impl WlanMac {
    /// Decode the 6 bytes as stored in traceability (least significant first)
    /// and validate them: not zero, not all 0xff (broadcast or erased), not a
    /// group (multicast) address, not locally administered.
    pub fn from_trace(raw: &[u8]) -> Result<Self, Error> {
        let bytes = eui48::from_stored(raw)?;
        eui48::check(&bytes)?;
        Ok(Self { bytes })
    }

    /// The driver's MAC file, byte for byte as stock writes it: lowercase hex,
    /// no separators, most significant byte first.
    pub fn mac_file(&self) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut out = String::with_capacity(FILE_LEN);
        out.push_str("Intf0MacAddress=");
        for b in self.bytes {
            out.push(HEX[usize::from(b >> 4)] as char);
            out.push(HEX[usize::from(b & 0x0f)] as char);
        }
        out.push_str("\nEND\n");
        out
    }
}
