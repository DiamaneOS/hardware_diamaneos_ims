// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! QMI NV read/write messages for the Fairphone TCL service (TCT_QMI).
//!
//! Reverse-engineered from stock `tctd`:
//! - service: QRTR service id 0x2ff, IDL v1 (`tct_get_service_object_internal_v01`
//!   @0xfdfc; service object @0x13e30 = {lib 5, idl 1, service 0x2ff, max_msg 0x100e},
//!   message counts {4 requests, 4 responses, 1 indication}).
//! - Message layouts come from the IDL message table @0x126b8 (encoded TLV
//!   descriptors @0x688a; each element is tag, type, struct offset, and for
//!   arrays a fixed length). The service-table maximum lengths confirm them.
//!   - NV read request (0x10): TLV 0x01 u32 item id.
//!   - NV read response: TLV 0x01 u32 data length, TLV 0x02 u8 NV status,
//!     TLV 0x03 u8[4096] data (fixed; max encoded 4110 bytes).
//!   - NV write request (0x30): TLV 0x01 u16 item id, TLV 0x02 u32 data length,
//!     TLV 0x03 u8[512] data (fixed; max encoded 527 bytes). Stock `imei_build`
//!     @0xb618 fills the data with the 9-byte NV 550 value followed by the
//!     subscription index, data length 10.
//!   - NV write response: TLV 0x01 u32, TLV 0x02 u8 NV status, TLV 0x03 u8[512].
//! - The service has no standard QMI result TLV: success is NV status 0
//!   (NV_DONE_S). Status 5 (NV_NOTACTIVE_S) means the item was never written,
//!   which is what an unprovisioned modem reports for NV 550.
//!
//! The framing is QMI-over-QRTR (no QMUX header), the codec in
//! `diamaneos_ims_dcm::protocol`, used here with this service's larger maximum.

use crate::imei::{Imei, NV550_LEN, NV_ITEM_IMEI};
use diamaneos_ims_dcm::protocol::{Encoder, Error as WireError, Frame, Kind};

pub const MSG_NV_READ: u16 = 0x10;
pub const MSG_NV_WRITE: u16 = 0x30;
/// Largest datagram the TCL service sends: its service object declares
/// max_msg 0x100e bytes of TLVs (NV read replies use all of it), plus the
/// 7-byte QMI header.
pub const MAX_DATAGRAM: usize = 7 + 0x100e;

const TLV_ITEM: u8 = 0x01;
const TLV_DATA_LEN: u8 = 0x02;
const TLV_DATA: u8 = 0x03;
const TLV_REPLY_LEN: u8 = 0x01;
const TLV_REPLY_STATUS: u8 = 0x02;
/// Fixed size of the write request's data array.
pub const WRITE_DATA_SIZE: usize = 512;
/// NV 550 value plus the subscription byte, as stock sends it.
const WRITE_DATA_LEN: u32 = NV550_LEN as u32 + 1;

/// NV status: done.
pub const NV_DONE: u8 = 0;
/// NV status: the item was never written.
pub const NV_NOTACTIVE: u8 = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageError {
    Wire(WireError),
    Kind,
    MessageId,
    Transaction,
    /// A mandatory reply TLV is missing or has the wrong size.
    MissingField(u8),
    /// The modem answered with this NV status (not done, not "never written").
    NvStatus(u8),
    /// NV status done, but the data is not a valid NV 550 value.
    BadData,
}

impl From<WireError> for MessageError {
    fn from(e: WireError) -> Self {
        MessageError::Wire(e)
    }
}

/// Build the NV read request (message 0x10): TLV 0x01, the item id as a
/// little-endian u32.
pub fn build_nv_read_request(txn: u16, item: u32) -> Result<Vec<u8>, MessageError> {
    let mut e = Encoder::new(Kind::Request, txn, MSG_NV_READ);
    e.tlv(TLV_ITEM, &item.to_le_bytes())?;
    Ok(e.finish())
}

/// Build the IMEI NV read request.
pub fn build_imei_read_request(txn: u16) -> Result<Vec<u8>, MessageError> {
    build_nv_read_request(txn, NV_ITEM_IMEI)
}

/// Build the NV write request (message 0x30) for one subscription: TLV 0x01
/// u16 item 550, TLV 0x02 u32 data length 10, TLV 0x03 the 512-byte data array
/// holding the NV 550 value and the subscription index.
pub fn build_imei_write_request(
    txn: u16,
    imei: &Imei,
    subscription: u8,
) -> Result<Vec<u8>, MessageError> {
    let mut data = [0u8; WRITE_DATA_SIZE];
    data[..NV550_LEN].copy_from_slice(&imei.to_nv550_bcd());
    data[NV550_LEN] = subscription;
    let mut e = Encoder::new(Kind::Request, txn, MSG_NV_WRITE);
    e.tlv(TLV_ITEM, &(NV_ITEM_IMEI as u16).to_le_bytes())?;
    e.tlv(TLV_DATA_LEN, &WRITE_DATA_LEN.to_le_bytes())?;
    e.tlv(TLV_DATA, &data)?;
    Ok(e.finish())
}

/// Parse a reply frame: check kind, message id and transaction, then return
/// the frame and its NV status (TLV 0x02).
fn parse_reply<'a>(
    bytes: &'a [u8],
    txn: u16,
    message_id: u16,
) -> Result<(Frame<'a>, u8), MessageError> {
    let frame = Frame::parse_bounded(bytes, MAX_DATAGRAM)?;
    if frame.kind != Kind::Response {
        return Err(MessageError::Kind);
    }
    if frame.id != message_id {
        return Err(MessageError::MessageId);
    }
    if frame.txn != txn {
        return Err(MessageError::Transaction);
    }
    match frame.tlv(TLV_REPLY_STATUS) {
        Some([status]) => Ok((frame, *status)),
        _ => Err(MessageError::MissingField(TLV_REPLY_STATUS)),
    }
}

/// Parse an NV read reply. `Ok(None)`: NV 550 was never written (the modem uses
/// its placeholder). `Ok(Some(imei))`: the provisioned value.
pub fn parse_imei_read_response(bytes: &[u8], txn: u16) -> Result<Option<Imei>, MessageError> {
    let (frame, status) = parse_reply(bytes, txn, MSG_NV_READ)?;
    match status {
        NV_DONE => {}
        NV_NOTACTIVE => return Ok(None),
        other => return Err(MessageError::NvStatus(other)),
    }
    let len = match frame.tlv(TLV_REPLY_LEN) {
        Some(v) if v.len() == 4 => u32::from_le_bytes([v[0], v[1], v[2], v[3]]) as usize,
        _ => return Err(MessageError::MissingField(TLV_REPLY_LEN)),
    };
    let data = frame.tlv(TLV_DATA).ok_or(MessageError::MissingField(TLV_DATA))?;
    if len < NV550_LEN || len > data.len() {
        return Err(MessageError::BadData);
    }
    Imei::from_nv550_bcd(&data[..NV550_LEN]).map(Some).map_err(|_| MessageError::BadData)
}

/// Parse an NV write reply: success is NV status 0.
pub fn parse_write_response(bytes: &[u8], txn: u16) -> Result<(), MessageError> {
    match parse_reply(bytes, txn, MSG_NV_WRITE)? {
        (_, NV_DONE) => Ok(()),
        (_, other) => Err(MessageError::NvStatus(other)),
    }
}
