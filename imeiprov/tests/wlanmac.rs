// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! Host tests for the traceability Wi-Fi MAC decode, validation and MAC file.
//! All values are synthetic: the RFC 7042 documentation range 00:00:5e:00:53:xx
//! and made-up invalid patterns. No real address appears here or in any output.

use diamaneos_imeiprov::wlanmac::{Error, WlanMac, FILE_LEN, LEN, TRACE_OFFSET};
use diamaneos_imeiprov::{bdaddr, eui48};

/// 00:00:5e:00:53:02 as traceability stores it, least significant byte first.
const DOC_STORED: [u8; LEN] = [0x02, 0x53, 0x00, 0x5e, 0x00, 0x00];

/// Store a most-significant-first address the way traceability does.
fn stored(display: [u8; LEN]) -> [u8; LEN] {
    let mut raw = display;
    raw.reverse();
    raw
}

/// The qcacld parser (`hdd_update_mac_config`): per line, skip blanks and '#',
/// stop at "END", take `name=value` where the value is exactly 12 characters;
/// at least one entry, each 12 hex digits and not zero.
fn driver_parse(file: &str) -> Option<Vec<[u8; LEN]>> {
    let mut macs = Vec::new();
    for line in file.split('\n') {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with("END") {
            break;
        }
        let (name, value) = line.split_once('=')?;
        if name.trim().is_empty() || value.trim().len() != 12 {
            continue;
        }
        let value = value.trim();
        let mut mac = [0u8; LEN];
        for (i, byte) in mac.iter_mut().enumerate() {
            *byte = u8::from_str_radix(value.get(2 * i..2 * i + 2)?, 16).ok()?;
        }
        if mac == [0; LEN] {
            return None;
        }
        macs.push(mac);
    }
    (!macs.is_empty()).then_some(macs)
}

#[test]
fn offset_follows_the_bluetooth_address() {
    assert_eq!(TRACE_OFFSET, bdaddr::TRACE_OFFSET + bdaddr::LEN as u64);
    assert_eq!(TRACE_OFFSET, 0x39);
}

#[test]
fn mac_file_is_stock_format() {
    // setwlanmac.sh: echo "Intf0MacAddress="$wlan_mac; echo END (33 bytes).
    let mac = WlanMac::from_trace(&DOC_STORED).unwrap();
    let file = mac.mac_file();
    assert_eq!(file, "Intf0MacAddress=00005e005302\nEND\n");
    assert_eq!(file.len(), FILE_LEN);
}

#[test]
fn stored_order_is_least_significant_first() {
    let mac = WlanMac::from_trace(&stored([0x00, 0x00, 0x5e, 0xab, 0xcd, 0xef])).unwrap();
    assert_eq!(mac.mac_file(), "Intf0MacAddress=00005eabcdef\nEND\n");
}

#[test]
fn driver_reads_the_mac_back() {
    let display = [0x00, 0x00, 0x5e, 0x00, 0x53, 0x02];
    let file = WlanMac::from_trace(&stored(display)).unwrap().mac_file();
    assert_eq!(driver_parse(&file), Some(vec![display]));
}

#[test]
fn rejects_wrong_length() {
    assert_eq!(WlanMac::from_trace(&DOC_STORED[..5]), Err(Error::Length));
    assert_eq!(
        WlanMac::from_trace(&[0x02, 0x53, 0x00, 0x5e, 0x00, 0x00, 0x00]),
        Err(Error::Length)
    );
    assert_eq!(WlanMac::from_trace(&[]), Err(Error::Length));
}

#[test]
fn rejects_unwritten_erased_and_broadcast() {
    assert_eq!(WlanMac::from_trace(&[0x00; LEN]), Err(Error::Zero));
    assert_eq!(WlanMac::from_trace(&[0xff; LEN]), Err(Error::Erased));
}

#[test]
fn rejects_multicast() {
    // 01:00:5e:00:53:02 (IPv4 multicast prefix) and any odd first byte.
    assert_eq!(
        WlanMac::from_trace(&stored([0x01, 0x00, 0x5e, 0x00, 0x53, 0x02])),
        Err(Error::Group)
    );
    assert_eq!(
        WlanMac::from_trace(&stored([0x33, 0x33, 0x00, 0x00, 0x00, 0x01])),
        Err(Error::Group)
    );
    // The real stored order puts the last byte first: reading it unreversed
    // would test the wrong byte. 0x03 last is fine once reversed.
    assert!(WlanMac::from_trace(&stored([0x00, 0x00, 0x5e, 0x00, 0x53, 0x03])).is_ok());
}

#[test]
fn rejects_locally_administered() {
    // Android's randomised addresses and the driver's generated ones (02:...).
    assert_eq!(
        WlanMac::from_trace(&stored([0x02, 0x00, 0x5e, 0x00, 0x53, 0x02])),
        Err(Error::Local)
    );
    assert_eq!(
        WlanMac::from_trace(&stored([0xda, 0xa1, 0x19, 0x00, 0x00, 0x01])),
        Err(Error::Local)
    );
}

#[test]
fn bluetooth_only_rules_do_not_apply() {
    // A LAP the Bluetooth address rejects is a valid Wi-Fi MAC.
    let display = [0x00, 0x00, 0x5e, 0x9e, 0x8b, 0x00];
    assert_eq!(bdaddr::validate(&display), Err(bdaddr::Error::ReservedLap));
    assert!(WlanMac::from_trace(&stored(display)).is_ok());
}

#[test]
fn shared_checks_match_the_bluetooth_ones() {
    for (display, fault) in [
        ([0x00; LEN], eui48::Fault::Zero),
        ([0xff; LEN], eui48::Fault::Erased),
        ([0x01, 0x00, 0x5e, 0x00, 0x53, 0x02], eui48::Fault::Group),
        ([0x02, 0x00, 0x5e, 0x00, 0x53, 0x02], eui48::Fault::Local),
    ] {
        assert_eq!(eui48::check(&display), Err(fault));
        assert_eq!(bdaddr::validate(&display), Err(bdaddr::Error::from(fault)));
    }
    assert_eq!(eui48::check(&[0xfc, 0x00, 0x5e, 0x00, 0x53, 0x02]), Ok(()));
}

#[test]
fn debug_output_is_redacted() {
    let mac = WlanMac::from_trace(&DOC_STORED).unwrap();
    let debug = format!("{mac:?}");
    assert_eq!(debug, "WlanMac(<redacted>)");
    assert!(!debug.contains("5e") && !debug.contains("53"));
    assert_eq!(format!("{:?}", Error::Group), "Group");
}
