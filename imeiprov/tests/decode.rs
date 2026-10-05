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
fn write_request_payload_mirrors_struct() {
    let bytes = message::build_imei_write_request(1, &imei_a(), 0).unwrap();
    assert_eq!(bytes[0], 0);
    assert_eq!(&bytes[3..5], &message::MSG_NV_WRITE.to_le_bytes());
    assert_eq!(bytes[7], 0x01); // mandatory TLV tag
    let payload = &bytes[10..];
    assert_eq!(payload.len(), 18);
    assert_eq!(&payload[0..2], &(NV_ITEM_IMEI as u16).to_le_bytes()); // item
    assert_eq!(&payload[2..4], &[0, 0]); // pad
    assert_eq!(&payload[4..8], &10u32.to_le_bytes()); // data_len
    assert_eq!(&payload[8..17], &[0x08, 0x0A, 0, 0, 0, 0, 0, 0, 0x81]); // NV 550
    assert_eq!(payload[17], 0); // subscription
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

#[test]
fn read_response_decodes_imei_from_data_tlv() {
    let bcd = imei_a().to_nv550_bcd();
    let frame = response_frame(
        message::MSG_NV_READ,
        0x20,
        &[(0x02, &[0, 0, 0, 0]), (0x01, &bcd)],
    );
    let decoded = message::parse_imei_read_response(&frame, 0x20).unwrap();
    assert_eq!(decoded, imei_a());
}

#[test]
fn read_response_rejects_failure_result() {
    let frame = response_frame(message::MSG_NV_READ, 0x20, &[(0x02, &[1, 0, 2, 0])]);
    assert!(message::parse_imei_read_response(&frame, 0x20).is_err());
}

#[test]
fn read_response_rejects_wrong_transaction() {
    let bcd = imei_a().to_nv550_bcd();
    let frame = response_frame(
        message::MSG_NV_READ,
        0x20,
        &[(0x02, &[0, 0, 0, 0]), (0x01, &bcd)],
    );
    assert!(message::parse_imei_read_response(&frame, 0x21).is_err());
}

#[test]
fn write_response_accepts_success_only() {
    let ok = response_frame(message::MSG_NV_WRITE, 5, &[(0x02, &[0, 0, 0, 0])]);
    assert!(message::parse_write_response(&ok, 5).is_ok());
    let fail = response_frame(message::MSG_NV_WRITE, 5, &[(0x02, &[3, 0, 9, 0])]);
    assert!(message::parse_write_response(&fail, 5).is_err());
}
