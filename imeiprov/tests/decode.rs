// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! Host tests for IMEI decode/validate/encode and the NV message codec.
//! All values are synthetic (fake all-zero TACs with correct Luhn digits); no
//! real identifier appears here or in any output.

use diamaneos_imeiprov::imei::{check_pair, Imei, NV_ITEM_IMEI};
use diamaneos_imeiprov::message;

// Synthetic IMEI A: "000000000000018" (Luhn-valid: doubled d14=1 -> 2, plus
// check digit 8 == 10, 10 % 10 == 0).
const A_ASCII: &[u8; 15] = b"000000000000018";
// Synthetic IMEI B: "000000000000026" (doubled d14=2 -> 4, check digit 6).
const B_ASCII: &[u8; 15] = b"000000000000026";

fn imei_a() -> Imei {
    Imei::from_ascii(A_ASCII).unwrap()
}

#[test]
fn ascii_and_digits_round_trip() {
    let a = imei_a();
    let mut expected = [0u8; 15];
    expected[13] = 1;
    expected[14] = 8;
    assert_eq!(a.digits(), expected);
}

#[test]
fn rejects_wrong_length_and_non_digits() {
    assert!(Imei::from_ascii(b"12345").is_err());
    assert!(Imei::from_ascii(b"00000000000001X").is_err());
}

#[test]
fn luhn_accepts_valid_and_rejects_bad_check_digit() {
    assert!(imei_a().is_luhn_valid());
    assert!(Imei::from_ascii(B_ASCII).unwrap().is_luhn_valid());
    // Flip the check digit of A -> invalid.
    assert!(!Imei::from_ascii(b"000000000000017")
        .unwrap()
        .is_luhn_valid());
}

#[test]
fn bcd_layout_matches_stock_imei_build() {
    // A = ...d14=1, d15=8. byte1 low nibble = 0x0A identity; byte8 = (8<<4)|1.
    let bcd = imei_a().to_nv550_bcd();
    assert_eq!(bcd, [0x08, 0x0A, 0, 0, 0, 0, 0, 0, 0x81]);
}

#[test]
fn bcd_round_trip() {
    for ascii in [A_ASCII, B_ASCII] {
        let imei = Imei::from_ascii(ascii).unwrap();
        let decoded = Imei::from_nv550_bcd(&imei.to_nv550_bcd()).unwrap();
        assert_eq!(imei, decoded);
    }
}

#[test]
fn bcd_rejects_bad_count_or_identity() {
    assert!(Imei::from_nv550_bcd(&[0x07, 0x0A, 0, 0, 0, 0, 0, 0, 0]).is_err());
    assert!(Imei::from_nv550_bcd(&[0x08, 0x0B, 0, 0, 0, 0, 0, 0, 0]).is_err());
    assert!(Imei::from_nv550_bcd(&[0x08, 0x0A, 0, 0]).is_err());
}

#[test]
fn pair_check_requires_valid_and_distinct() {
    let a = imei_a();
    let b = Imei::from_ascii(B_ASCII).unwrap();
    assert!(check_pair(&a, &b).usable());
    // Identical pair -> not distinct.
    assert!(!check_pair(&a, &a).usable());
}

#[test]
fn read_request_is_tlv01_with_item_550() {
    let bytes = message::build_imei_read_request(0x1234).unwrap();
    // [type=0][txn lo hi][msg lo hi][len lo hi] then TLV 0x01 len4 value.
    assert_eq!(bytes[0], 0); // request
    assert_eq!(&bytes[1..3], &0x1234u16.to_le_bytes());
    assert_eq!(&bytes[3..5], &message::MSG_NV_READ.to_le_bytes());
    assert_eq!(&bytes[5..7], &7u16.to_le_bytes()); // TLV bytes = 3 + 4
    assert_eq!(bytes[7], 0x01); // TLV tag
    assert_eq!(&bytes[8..10], &4u16.to_le_bytes());
    assert_eq!(&bytes[10..14], &NV_ITEM_IMEI.to_le_bytes());
}

#[test]
fn write_request_matches_the_stock_idl() {
    // TLV 0x01 u16 item, TLV 0x02 u32 data length, TLV 0x03 u8[512] data
    // (stock IDL message 7, max encoded length 527).
    let bytes = message::build_imei_write_request(1, &imei_a(), 1).unwrap();
    assert_eq!(bytes[0], 0);
    assert_eq!(&bytes[3..5], &message::MSG_NV_WRITE.to_le_bytes());
    assert_eq!(&bytes[5..7], &527u16.to_le_bytes());
    assert_eq!(bytes.len(), 7 + 527);
    assert_eq!(&bytes[7..10], &[0x01, 2, 0]);
    assert_eq!(&bytes[10..12], &(NV_ITEM_IMEI as u16).to_le_bytes());
    assert_eq!(&bytes[12..15], &[0x02, 4, 0]);
    assert_eq!(&bytes[15..19], &10u32.to_le_bytes());
    assert_eq!(&bytes[19..22], &[0x03, 0x00, 0x02]); // length 512
    let data = &bytes[22..];
    assert_eq!(data.len(), message::WRITE_DATA_SIZE);
    assert_eq!(&data[0..9], &[0x08, 0x0A, 0, 0, 0, 0, 0, 0, 0x81]); // NV 550
    assert_eq!(data[9], 1); // subscription
    assert!(data[10..].iter().all(|&b| b == 0));
}

/// Build a minimal QMI response frame for tests.
fn response_frame(msg_id: u16, txn: u16, tlvs: &[(u8, &[u8])]) -> Vec<u8> {
    let mut body = Vec::new();
    for (tag, value) in tlvs {
        body.push(*tag);
        body.extend_from_slice(&(value.len() as u16).to_le_bytes());
        body.extend_from_slice(value);
    }
    let mut frame = vec![2u8]; // response
    frame.extend_from_slice(&txn.to_le_bytes());
    frame.extend_from_slice(&msg_id.to_le_bytes());
    frame.extend_from_slice(&(body.len() as u16).to_le_bytes());
    frame.extend_from_slice(&body);
    frame
}

/// A read reply as the modem sends it: TLV 0x01 u32 length, TLV 0x02 u8 NV
/// status, TLV 0x03 the fixed 4096-byte data array.
fn read_reply(txn: u16, status: u8, nv: &[u8]) -> Vec<u8> {
    let mut data = nv.to_vec();
    data.resize(4096, 0);
    response_frame(
        message::MSG_NV_READ,
        txn,
        &[(0x01, &(nv.len() as u32).to_le_bytes()), (0x02, &[status]), (0x03, &data)],
    )
}

#[test]
fn read_response_decodes_a_provisioned_imei() {
    let frame = read_reply(0x20, message::NV_DONE, &imei_a().to_nv550_bcd());
    assert_eq!(frame.len(), message::MAX_DATAGRAM); // full-size, as on the phone
    assert_eq!(message::parse_imei_read_response(&frame, 0x20).unwrap(), Some(imei_a()));
}

#[test]
fn read_response_reports_never_written() {
    // What the FP6 modem answered on 2026-10-05: length 0, status 5.
    let frame = read_reply(0x20, message::NV_NOTACTIVE, &[]);
    assert_eq!(message::parse_imei_read_response(&frame, 0x20).unwrap(), None);
}

#[test]
fn read_response_rejects_other_status_short_data_and_bad_frames() {
    let fail = read_reply(0x20, 4, &[]);
    assert_eq!(
        message::parse_imei_read_response(&fail, 0x20),
        Err(message::MessageError::NvStatus(4))
    );
    let short = read_reply(0x20, message::NV_DONE, &imei_a().to_nv550_bcd()[..5]);
    assert!(message::parse_imei_read_response(&short, 0x20).is_err());
    let good = read_reply(0x20, message::NV_DONE, &imei_a().to_nv550_bcd());
    assert!(message::parse_imei_read_response(&good, 0x21).is_err()); // transaction
    let mut oversized = good.clone();
    oversized.push(0);
    assert!(message::parse_imei_read_response(&oversized, 0x20).is_err());
    let no_status = response_frame(message::MSG_NV_READ, 0x20, &[(0x01, &[0, 0, 0, 0])]);
    assert!(message::parse_imei_read_response(&no_status, 0x20).is_err());
}

#[test]
fn write_response_accepts_status_done_only() {
    let reply = |status: u8| {
        response_frame(
            message::MSG_NV_WRITE,
            5,
            &[(0x01, &[0, 0, 0, 0]), (0x02, &[status]), (0x03, &[0u8; 512])],
        )
    };
    assert!(message::parse_write_response(&reply(0), 5).is_ok());
    assert_eq!(
        message::parse_write_response(&reply(7), 5),
        Err(message::MessageError::NvStatus(7))
    );
}
