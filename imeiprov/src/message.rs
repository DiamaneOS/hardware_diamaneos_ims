// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! QMI NV read/write messages for the Fairphone TCL service (TCT_QMI).
//!
//! Reverse-engineered from stock `tctd`:
//! - service: QRTR service id 0x2ff, IDL v1 (`tct_get_service_object_internal_v01`
//!   @0xfdfc; service object @0x13e30 = {lib 5, idl 1, service 0x2ff, max_msg 0x100e}).
//! - NV read: message id 0x10, request = item id as a u32 (stock passes a 4-byte
//!   request; `tct_nv_read_register` @0xfe1c, `qmi_client_send_msg_sync` @0x1003c).
//! - NV write: message id 0x30, request = the `tct_nv_write_req_msg_v01` fields
//!   (`imei_build` @0xb618, `start` @0xbdd0/0xbde0).
//!
//! The framing is QMI-over-QRTR (no QMUX header), identical to the codec in
//! `diamaneos_ims_dcm::protocol`, which this module reuses. The request payload
//! is carried in the mandatory TLV 0x01; the response result is the standard
//! TLV 0x02 {u16 result, u16 error}. The exact TLV wrapping of the *write*
//! payload is the one field not provable offline; `--write` reads the value back
//! and fails closed if the modem rejects it (see docs/imei-provisioning.md).

use crate::imei::{Imei, NV550_LEN, NV_ITEM_IMEI};
use diamaneos_ims_dcm::protocol::{Encoder, Error as WireError, Frame, Kind};

pub const MSG_NV_READ: u16 = 0x10;
pub const MSG_NV_WRITE: u16 = 0x30;
const TLV_PAYLOAD: u8 = 0x01;
const TLV_RESULT: u8 = 0x02;

/// Fixed field in `tct_nv_write_req_msg_v01` at offset 0x04 (`imei_build` 0xb64c:
/// `mov w11,#0xa`). Reproduced verbatim; its exact meaning is not needed.
const WRITE_DATA_LEN: u32 = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageError {
    Wire(WireError),
    Kind,
    MessageId,
    Transaction,
    /// The modem returned QMI_RESULT_FAILURE; carries result/error codes only.
    Result {
        result: u16,
        error: u16,
    },
    MissingResult,
    MissingData,
}

impl From<WireError> for MessageError {
    fn from(e: WireError) -> Self {
        MessageError::Wire(e)
    }
}

/// Build the NV read request (message 0x10): one mandatory TLV 0x01 holding the
/// item id as a little-endian u32.
pub fn build_nv_read_request(txn: u16, item: u32) -> Result<Vec<u8>, MessageError> {
    let mut e = Encoder::new(Kind::Request, txn, MSG_NV_READ);
    e.tlv(TLV_PAYLOAD, &item.to_le_bytes())?;
    Ok(e.finish())
}

/// Build the IMEI NV read request.
pub fn build_imei_read_request(txn: u16) -> Result<Vec<u8>, MessageError> {
    build_nv_read_request(txn, NV_ITEM_IMEI)
}

/// Build the NV write request (message 0x30) for one subscription. The payload
/// mirrors the meaningful prefix of `tct_nv_write_req_msg_v01`:
///   u16 item | u16 pad | u32 data_len | u8 count(0x08) | u8 bcd[8] | u8 sub
/// (offsets 0x00, 0x02, 0x04, 0x08, 0x09.., 0x11 in the stock struct).
pub fn build_imei_write_request(
    txn: u16,
    imei: &Imei,
    subscription: u8,
) -> Result<Vec<u8>, MessageError> {
    let bcd = imei.to_nv550_bcd();
    let mut payload = Vec::with_capacity(18);
    payload.extend_from_slice(&(NV_ITEM_IMEI as u16).to_le_bytes()); // 0x00
    payload.extend_from_slice(&[0, 0]); // 0x02 pad
    payload.extend_from_slice(&WRITE_DATA_LEN.to_le_bytes()); // 0x04
    payload.extend_from_slice(&bcd); // 0x08: count byte + 8 BCD bytes
    payload.push(subscription); // 0x11
    let mut e = Encoder::new(Kind::Request, txn, MSG_NV_WRITE);
    e.tlv(TLV_PAYLOAD, &payload)?;
    Ok(e.finish())
}

/// Parse a response frame: check kind, message id and transaction, then the
/// mandatory result TLV 0x02.
fn parse_response<'a>(
    bytes: &'a [u8],
    txn: u16,
    message_id: u16,
) -> Result<Frame<'a>, MessageError> {
    let frame = Frame::parse(bytes)?;
    if frame.kind != Kind::Response {
        return Err(MessageError::Kind);
    }
    if frame.id != message_id {
        return Err(MessageError::MessageId);
    }
    if frame.txn != txn {
        return Err(MessageError::Transaction);
    }
    let result = frame.tlv(TLV_RESULT).ok_or(MessageError::MissingResult)?;
    if result.len() < 4 {
        return Err(MessageError::Wire(WireError::Length));
    }
    let code = u16::from_le_bytes([result[0], result[1]]);
    let err = u16::from_le_bytes([result[2], result[3]]);
    if code != 0 {
        return Err(MessageError::Result {
            result: code,
            error: err,
        });
    }
    Ok(frame)
}

/// Parse an NV read response and decode the IMEI. The NV value lives in a data
/// TLV; rather than assume its exact tag, scan every non-result TLV for a value
/// that decodes as a valid NV 550 block (count byte 0x08 + identity nibble).
pub fn parse_imei_read_response(bytes: &[u8], txn: u16) -> Result<Imei, MessageError> {
    let frame = parse_response(bytes, txn, MSG_NV_READ)?;
    for (tag, value) in frame.tlvs() {
        if tag == TLV_RESULT {
            continue;
        }
        // The data TLV may carry a small header before the NV bytes; try every
        // window that starts a valid NV 550 block.
        for start in 0..=value.len().saturating_sub(NV550_LEN) {
            if let Ok(imei) = Imei::from_nv550_bcd(&value[start..]) {
                return Ok(imei);
            }
        }
    }
    Err(MessageError::MissingData)
}

/// Parse an NV write response: only the result TLV matters.
pub fn parse_write_response(bytes: &[u8], txn: u16) -> Result<(), MessageError> {
    parse_response(bytes, txn, MSG_NV_WRITE).map(|_| ())
}
