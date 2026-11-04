// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! Modem input to wlanreportd: DSD responses, NAT keepalive instructions and
//! connectivity-profile notices (wlan/src/lib.rs, profile_notice.rs,
//! profiles.rs). Input: 2 bytes transaction, 1 byte message choice, packet.
#![no_main]

use diamaneos_ims_dcm::protocol::{Frame, Kind};
use diamaneos_wlan_reporting::{
    keepalive_instruction, profile_notice::Notice, profiles::Registry, response_for, Error,
    BIND_SUBSCRIPTION, DATA_SETTINGS, DEFAULT_PROFILE_STATUS, INDICATION_REGISTRATION,
    NAT_KEEPALIVE_INDICATION, NAT_KEEPALIVE_OPERATION_STATUS, WLAN_STATUS,
};
use libfuzzer_sys::fuzz_target;

const MESSAGES: [u16; 7] = [
    WLAN_STATUS,
    BIND_SUBSCRIPTION,
    DATA_SETTINGS,
    INDICATION_REGISTRATION,
    NAT_KEEPALIVE_OPERATION_STATUS,
    DEFAULT_PROFILE_STATUS,
    0x3f,
];

fuzz_target!(|data: &[u8]| {
    if data.len() < 3 {
        return;
    }
    let txn = u16::from_le_bytes([data[0], data[1]]);
    let message = MESSAGES[usize::from(data[2]) % MESSAGES.len()];
    let bytes = &data[3..];

    match response_for(bytes, txn, message) {
        Ok(()) | Err(Error::ModemFailure(_)) => {
            let f = Frame::parse(bytes).expect("accepted response parses");
            assert_eq!((f.kind, f.txn, f.id), (Kind::Response, txn, message));
            assert_eq!(bytes.len(), 14);
        }
        Err(_) => {}
    }

    if keepalive_instruction(bytes) {
        let f = Frame::parse(bytes).unwrap();
        assert_eq!((f.kind, f.id), (Kind::Indication, NAT_KEEPALIVE_INDICATION));
        assert!(bytes.len() <= 54);
    }

    if let Some(notice) = Notice::parse(bytes) {
        assert!(bytes.len() <= 69);
        match notice {
            Notice::Initialized(p) => assert!((4..=43).contains(&p)),
            Notice::Selected(s) => {
                for p in 0..=255u8 {
                    let _ = s.contains(p);
                }
            }
        }
    }

    // A registry fed this packet and a few of its prefixes stays bounded and
    // only reports tokens that are current.
    let mut registry = Registry::new(1).unwrap();
    for end in [bytes.len(), bytes.len() / 2, bytes.len().saturating_sub(1)] {
        let _ = registry.receive(&bytes[..end]);
    }
    while let Some(token) = registry.report_due() {
        assert!(registry.is_current(token));
        let packet = token.inconclusive(txn.max(1)).expect("encodes");
        assert!(Frame::parse(&packet).is_ok());
        assert!(registry.acknowledged(token));
        assert!(!registry.is_current(token));
    }
});
