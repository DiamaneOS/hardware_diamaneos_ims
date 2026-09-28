// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
use diamaneos_wlan_reporting::{response, Connected, Error};
use std::net::{Ipv4Addr, Ipv6Addr};

const BSSID: [u8; 6] = [2, 0, 0, 0, 0, 1];

#[test]
fn fixed_wire_vectors_bind_byte_order_and_withdrawal() {
    let network = Connected::new(BSSID, Some(Ipv4Addr::new(192, 0, 2, 1)), None, true).unwrap();
    // Independent vector from IDL TLV tags/widths and stock STA builder offsets.
    assert_eq!(
        network.encode(0x1234).unwrap(),
        vec![
            0, 0x34, 0x12, 0x20, 0, 30, 0, 1, 6, 0, 2, 0, 0, 0, 0, 1, 0x10, 4, 0, 1, 2, 0, 192,
            0x1f, 4, 0, 2, 0, 0, 0, 0x21, 4, 0, 2, 0, 0, 0,
        ]
    );
    assert_eq!(
        network.encode_withdrawal(2).unwrap(),
        vec![
            0, 2, 0, 0x20, 0, 23, 0, 1, 6, 0, 2, 0, 0, 0, 0, 1, 0x1f, 4, 0, 2, 0, 0, 0, 0x21, 4, 0,
            0, 0, 0, 0,
        ]
    );
    assert_eq!(network.encode(0), Err(Error::ZeroTransaction));
    assert_eq!(network.encode_withdrawal(0), Err(Error::ZeroTransaction));
}

#[test]
fn ipv6_keeps_octets_prefix_and_unvalidated_state() {
    let ip: Ipv6Addr = "2001:db8::1".parse().unwrap();
    let network = Connected::new(BSSID, None, Some((ip, 64)), false).unwrap();
    let mut expected = vec![
        0, 1, 0, 0x20, 0, 43, 0, 1, 6, 0, 2, 0, 0, 0, 0, 1, 0x11, 17, 0,
    ];
    expected.extend_from_slice(&ip.octets());
    expected.extend_from_slice(&[64, 0x1f, 4, 0, 2, 0, 0, 0, 0x21, 4, 0, 1, 0, 0, 0]);
    assert_eq!(network.encode(1).unwrap(), expected);
}

#[test]
fn reject_redaction_and_unusable_observations() {
    let ip = Some(Ipv4Addr::new(192, 0, 2, 1));
    for bssid in [[0; 6], [2, 0, 0, 0, 0, 0], [255; 6], [1, 0, 0, 0, 0, 1]] {
        assert!(matches!(
            Connected::new(bssid, ip, None, true),
            Err(Error::InvalidBssid)
        ));
    }
    assert!(matches!(
        Connected::new(BSSID, None, None, true),
        Err(Error::MissingAddress)
    ));
    for addr in [
        "0.0.0.0",
        "0.1.2.3",
        "127.0.0.1",
        "169.254.1.1",
        "224.0.0.1",
        "240.1.2.3",
        "255.255.255.255",
    ] {
        assert!(matches!(
            Connected::new(BSSID, Some(addr.parse().unwrap()), None, true),
            Err(Error::InvalidAddress)
        ));
    }
    for addr in ["::", "::1", "fe80::1", "ff02::1", "::ffff:192.0.2.1"] {
        assert!(matches!(
            Connected::new(BSSID, None, Some((addr.parse().unwrap(), 64)), true),
            Err(Error::InvalidAddress)
        ));
    }
    assert!(matches!(
        Connected::new(
            BSSID,
            None,
            Some(("2001:db8::1".parse().unwrap(), 129)),
            true
        ),
        Err(Error::InvalidAddress)
    ));
}

#[test]
fn responses_do_not_accept_mismatched_or_malformed_success() {
    let success = [2, 7, 0, 0x20, 0, 7, 0, 2, 4, 0, 0, 0, 0, 0];
    assert_eq!(response(&success, 7), Ok(()));
    assert_eq!(response(&success, 8), Err(Error::WrongTransaction));
    assert_eq!(response(&success, 0), Err(Error::ZeroTransaction));
    for size in 0..success.len() {
        assert_eq!(response(&success[..size], 7), Err(Error::MalformedResponse));
    }
    let mut extra = success.to_vec();
    extra.extend_from_slice(&[2, 4, 0, 0, 0, 0, 0]);
    extra[5] = 14;
    assert_eq!(response(&extra, 7), Err(Error::MalformedResponse));
    // Wrong kind/message, malformed header/TLV length, unknown mandatory TLV,
    // nonzero error on success, and impossible result all fail closed.
    for (offset, value) in [
        (0, 0),
        (0, 4),
        (3, 0x21),
        (5, 8),
        (7, 1),
        (8, 3),
        (12, 1),
        (10, 2),
    ] {
        let mut bad = success;
        bad[offset] = value;
        assert_eq!(response(&bad, 7), Err(Error::MalformedResponse));
    }
    let mut failure = success;
    failure[10] = 1;
    failure[12] = 5;
    assert_eq!(response(&failure, 7), Err(Error::ModemFailure(5)));
    failure[12] = 0;
    assert_eq!(response(&failure, 7), Err(Error::MalformedResponse));
}

#[test]
fn dns_wire_metadata_matches_stock_slots_and_is_absent_on_withdrawal() {
    let base = Connected::new(BSSID, Some("192.0.2.1".parse().unwrap()), None, true).unwrap();
    let network = base
        .clone()
        .with_dns(
            [
                Some("192.0.2.53".parse().unwrap()),
                Some("198.51.100.53".parse().unwrap()),
            ],
            [
                Some("2001:db8::53".parse().unwrap()),
                Some("fe80::53".parse().unwrap()),
            ],
        )
        .unwrap();
    let packet = network.encode(1).unwrap();
    // Independently specified tags, lengths and address byte order.
    let expected = [
        0x13, 4, 0, 53, 2, 0, 192, 0x14, 4, 0, 53, 100, 51, 198, 0x15, 16, 0, 0x20, 1, 0x0d, 0xb8,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x53, 0x16, 16, 0, 0xfe, 0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0x53,
    ];
    assert_eq!(&packet[23..75], &expected);
    assert_eq!(
        u16::from_le_bytes([packet[5], packet[6]]) as usize,
        packet.len() - 7
    );
    assert_eq!(network.encode_withdrawal(2), base.encode_withdrawal(2));
}

#[test]
fn reject_invalid_or_ambiguous_resolver_slots() {
    let base = Connected::new(BSSID, Some("192.0.2.1".parse().unwrap()), None, true).unwrap();
    for value in [
        "0.0.0.0",
        "127.0.0.1",
        "224.0.0.1",
        "255.255.255.255",
        "240.0.0.1",
    ] {
        assert!(base
            .clone()
            .with_dns([Some(value.parse().unwrap()), None], [None; 2])
            .is_err());
    }
    for value in ["::", "::1", "ff02::1", "::ffff:192.0.2.53"] {
        assert!(base
            .clone()
            .with_dns([None; 2], [Some(value.parse().unwrap()), None])
            .is_err());
    }
    let ip = Some("192.0.2.53".parse().unwrap());
    assert!(base.clone().with_dns([None, ip], [None; 2]).is_err());
    assert!(base.with_dns([ip, ip], [None; 2]).is_err());
}
