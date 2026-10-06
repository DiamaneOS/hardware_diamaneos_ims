// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! Host tests for the traceability Bluetooth address decode and validation.
//! All values are synthetic: the RFC 7042 documentation range 00:00:5e:00:53:xx
//! and made-up invalid patterns. No real address appears here or in any output.

use diamaneos_imeiprov::bdaddr::{validate, BdAddr, Error, LEN, PROPERTY_LEN, TRACE_OFFSET};

/// 00:00:5e:00:53:01 as traceability stores it, least significant byte first.
const DOC_STORED: [u8; LEN] = [0x01, 0x53, 0x00, 0x5e, 0x00, 0x00];

/// Store a most-significant-first address the way traceability does.
fn stored(display: [u8; LEN]) -> [u8; LEN] {
    let mut raw = display;
    raw.reverse();
    raw
}

#[test]
fn offset_follows_slot1_imei() {
    // Slot 1 IMEI: 15 ASCII digits at 0x24 (stock tctd), so 0x24 + 15 = 0x33.
    assert_eq!(TRACE_OFFSET, 0x24 + 15);
}

#[test]
fn stored_order_is_least_significant_first() {
    // The first stored byte (0x01) would be a group address if read in stored
    // order; reversed it is the last byte of a valid unicast address.
    let address = BdAddr::from_trace(&DOC_STORED).unwrap();
    assert_eq!(address.property_value(), "00:00:5e:00:53:01");
}

#[test]
fn property_value_is_stock_format() {
    let address = BdAddr::from_trace(&stored([0x00, 0x00, 0x5e, 0xab, 0xcd, 0xef])).unwrap();
    let value = address.property_value();
    assert_eq!(value.len(), PROPERTY_LEN);
    assert_eq!(value, "00:00:5e:ab:cd:ef");
}

#[test]
fn rejects_wrong_length() {
    assert_eq!(BdAddr::from_trace(&DOC_STORED[..5]), Err(Error::Length));
    assert_eq!(
        BdAddr::from_trace(&[0x01, 0x53, 0x00, 0x5e, 0x00, 0x00, 0x00]),
        Err(Error::Length)
    );
    assert_eq!(BdAddr::from_trace(&[]), Err(Error::Length));
}

#[test]
fn rejects_unwritten_and_erased() {
    assert_eq!(BdAddr::from_trace(&[0x00; LEN]), Err(Error::Zero));
    assert_eq!(BdAddr::from_trace(&[0xff; LEN]), Err(Error::Erased));
}

#[test]
fn rejects_group_addresses() {
    assert_eq!(
        validate(&[0x01, 0x00, 0x5e, 0x00, 0x53, 0x01]),
        Err(Error::Group)
    );
    assert_eq!(
        BdAddr::from_trace(&stored([0x03, 0x00, 0x5e, 0x00, 0x53, 0x01])),
        Err(Error::Group)
    );
}

#[test]
fn rejects_locally_administered_addresses() {
    // The HCI implementation's generated addresses start with 22:22.
    assert_eq!(
        BdAddr::from_trace(&stored([0x22, 0x22, 0x12, 0x34, 0x56, 0x78])),
        Err(Error::Local)
    );
    assert_eq!(
        validate(&[0x02, 0x00, 0x5e, 0x00, 0x53, 0x01]),
        Err(Error::Local)
    );
}

#[test]
fn rejects_reserved_inquiry_laps() {
    for lap in [[0x9e, 0x8b, 0x00], [0x9e, 0x8b, 0x33], [0x9e, 0x8b, 0x3f]] {
        let display = [0x00, 0x00, 0x5e, lap[0], lap[1], lap[2]];
        assert_eq!(validate(&display), Err(Error::ReservedLap));
    }
    assert_eq!(validate(&[0x00, 0x00, 0x5e, 0x9e, 0x8b, 0x40]), Ok(()));
    assert_eq!(validate(&[0x00, 0x00, 0x5e, 0x9e, 0x8a, 0xff]), Ok(()));
}

#[test]
fn accepts_universally_administered_unicast() {
    assert_eq!(validate(&[0x00, 0x00, 0x5e, 0x00, 0x53, 0x01]), Ok(()));
    // Group and local bits clear, other bits of the first byte set.
    assert_eq!(validate(&[0xfc, 0x00, 0x5e, 0x00, 0x53, 0x01]), Ok(()));
}

#[test]
fn debug_output_is_redacted() {
    let address = BdAddr::from_trace(&DOC_STORED).unwrap();
    let debug = format!("{address:?}");
    assert_eq!(debug, "BdAddr(<redacted>)");
    assert!(!debug.contains("5e") && !debug.contains("53"));
    // Errors carry no address bytes either.
    assert_eq!(format!("{:?}", Error::Local), "Local");
}
