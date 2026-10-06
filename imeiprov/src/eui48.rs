// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! Checks shared by the factory Bluetooth address and Wi-Fi MAC.
//!
//! Both are IEEE 802 48-bit addresses (EUI-48) that the `traceability`
//! partition stores least significant byte first. These checks reject only
//! values that cannot be a factory-assigned address; stock checks nothing.

/// Bytes of an EUI-48.
pub const LEN: usize = 6;

/// Why an address is rejected. Carries no address bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    /// Not exactly 6 bytes.
    Length,
    /// All zero: never written.
    Zero,
    /// All 0xff: erased flash, or the broadcast address.
    Erased,
    /// Group (multicast) bit set: never a device address.
    Group,
    /// Locally administered bit set: not a factory address.
    Local,
}

/// The 6 bytes as stored (least significant first), in transmission order
/// (most significant first).
pub fn from_stored(raw: &[u8]) -> Result<[u8; LEN], Fault> {
    let mut bytes: [u8; LEN] = raw.try_into().map_err(|_| Fault::Length)?;
    bytes.reverse();
    Ok(bytes)
}

/// Accepts only a universally administered unicast address, given most
/// significant byte first.
pub fn check(bytes: &[u8; LEN]) -> Result<(), Fault> {
    if bytes.iter().all(|&b| b == 0) {
        return Err(Fault::Zero);
    }
    if bytes.iter().all(|&b| b == 0xff) {
        return Err(Fault::Erased);
    }
    if bytes[0] & 0x01 != 0 {
        return Err(Fault::Group);
    }
    if bytes[0] & 0x02 != 0 {
        return Err(Fault::Local);
    }
    Ok(())
}
