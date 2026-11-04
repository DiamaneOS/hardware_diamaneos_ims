// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! Modem input: TCL QMI NV read and write replies (imeiprov/src/message.rs).
//! Input layout: 2 bytes transaction id, then the datagram as received.
#![no_main]

use diamaneos_ims_dcm::protocol::{Frame, Kind};
use diamaneos_imeiprov::imei::NV550_LEN;
use diamaneos_imeiprov::message::{
    self, MessageError, MAX_DATAGRAM, MSG_NV_READ, MSG_NV_WRITE, NV_DONE, NV_NOTACTIVE,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() < 2 {
        return;
    }
    let txn = u16::from_le_bytes([data[0], data[1]]);
    let bytes = &data[2..];

    let read = message::parse_imei_read_response(bytes, txn);
    let write = message::parse_write_response(bytes, txn);

    // Anything accepted must be a well-formed response frame for our
    // transaction, within the service's declared maximum.
    if read.is_ok() || write.is_ok() {
        assert!(bytes.len() <= MAX_DATAGRAM);
        let frame = Frame::parse_bounded(bytes, MAX_DATAGRAM).expect("accepted frame parses");
        assert_eq!(frame.kind, Kind::Response);
        assert_eq!(frame.txn, txn);
    }
    match read {
        Ok(Some(imei)) => {
            let frame = Frame::parse_bounded(bytes, MAX_DATAGRAM).unwrap();
            assert_eq!(frame.id, MSG_NV_READ);
            assert_eq!(frame.tlv(2), Some(&[NV_DONE][..]));
            // The decoded value is exactly the first 9 data bytes.
            let data = frame.tlv(3).unwrap();
            assert_eq!(&imei.to_nv550_bcd()[..], &data[..NV550_LEN]);
            // Redacting Debug: never print digits.
            assert_eq!(format!("{imei:?}"), "Imei(<redacted>)");
        }
        Ok(None) => {
            let frame = Frame::parse_bounded(bytes, MAX_DATAGRAM).unwrap();
            assert_eq!(frame.tlv(2), Some(&[NV_NOTACTIVE][..]));
        }
        Err(MessageError::NvStatus(s)) => assert!(s != NV_DONE && s != NV_NOTACTIVE),
        Err(_) => {}
    }
    if write.is_ok() {
        let frame = Frame::parse_bounded(bytes, MAX_DATAGRAM).unwrap();
        assert_eq!(frame.id, MSG_NV_WRITE);
    }

    // Requests built from fuzzed values parse back with our transaction id.
    if let Ok(req) = message::build_nv_read_request(txn, u32::from_le_bytes([
        bytes.first().copied().unwrap_or(0),
        bytes.get(1).copied().unwrap_or(0),
        bytes.get(2).copied().unwrap_or(0),
        bytes.get(3).copied().unwrap_or(0),
    ])) {
        let f = Frame::parse_bounded(&req, MAX_DATAGRAM).unwrap();
        assert_eq!((f.kind, f.txn, f.id), (Kind::Request, txn, MSG_NV_READ));
    }
});
