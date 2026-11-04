// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! Modem input: QMI-over-QRTR framing and DCM request decoding
//! (dcm/src/protocol.rs). Input: 1 byte selects the slot count, then the frame.
#![no_main]

use diamaneos_ims_dcm::protocol::{self, Activation, Encoder, Frame, MAX_DATAGRAM};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Some((&config, bytes)) = data.split_first() else {
        return;
    };
    let slots = u32::from(config % 3) + 1;

    // A larger service bound (the TCL service's) must agree with the DCM one
    // on every frame the DCM bound accepts.
    let bounded = Frame::parse_bounded(bytes, 7 + 0x100e);
    let Ok(frame) = Frame::parse(bytes) else {
        if bytes.len() <= MAX_DATAGRAM {
            assert!(bounded.is_err());
        }
        return;
    };
    assert!(bounded.is_ok());
    assert!(bytes.len() <= MAX_DATAGRAM);

    // The iterator and the lookup see the same TLVs. No tag repeats.
    let mut seen = [false; 256];
    let mut encoder = Encoder::new(frame.kind, frame.txn, frame.id);
    for (tag, value) in frame.tlvs() {
        assert!(!seen[usize::from(tag)]);
        seen[usize::from(tag)] = true;
        assert_eq!(frame.tlv(tag), Some(value));
        encoder.tlv(tag, value).expect("re-encoding fits");
    }
    for tag in 0..=255u8 {
        assert_eq!(frame.tlv(tag).is_some(), seen[usize::from(tag)]);
        let _ = frame.optional_u32(tag);
    }
    // Re-encoding gives back the same bytes.
    assert_eq!(encoder.finish(), bytes);

    let valid = protocol::validate_request(&frame).is_ok();
    if let Ok(a) = Activation::decode(&frame, slots) {
        assert!(a.apn.len() <= 100);
        assert!(a.slot >= -1 && a.slot < slots as i32);
        assert!(a.same_pdn(&a));
        if valid {
            // Replies built from a decoded activation are themselves valid frames.
            for reply in [
                protocol::activation_response(frame.txn, 20, &a),
                protocol::indication(frame.txn, 20, &a, None, false),
                protocol::indication(frame.txn, 20, &a, Some("192.0.2.1".parse().unwrap()), true),
                protocol::indication(frame.txn, 20, &a, Some("2001:db8::1".parse().unwrap()), false),
            ] {
                assert!(Frame::parse(&reply).is_ok());
            }
        }
    }
    let _ = protocol::u8_value(bytes);
    let _ = protocol::u32_value(bytes);
});
