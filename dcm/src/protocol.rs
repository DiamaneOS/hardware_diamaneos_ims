// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! Bounded QMI service framing over QRTR (no QMUX header).
//! Wire facts: docs/dcm-protocol.md. No Android or socket dependencies.
use std::net::IpAddr;

pub const MAX_DATAGRAM: usize = 1043; // 7-byte service header + 1036-byte body
pub const ACTIVATE: u16 = 0x20;
pub const DEACTIVATE: u16 = 0x21;
pub const ADDRESS_CHANGE: u16 = 0x24;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Truncated,
    Length,
    Duplicate,
    Missing,
    Value,
    Kind,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Request = 0,
    Response = 2,
    Indication = 4,
}
#[derive(Clone, Copy)]
pub struct Frame<'a> {
    pub kind: Kind,
    pub txn: u16,
    pub id: u16,
    body: &'a [u8],
}
impl<'a> Frame<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, Error> {
        if bytes.len() < 7 {
            return Err(Error::Truncated);
        }
        if bytes.len() > MAX_DATAGRAM {
            return Err(Error::Length);
        }
        let kind = match bytes[0] {
            0 => Kind::Request,
            2 => Kind::Response,
            4 => Kind::Indication,
            _ => return Err(Error::Kind),
        };
        let length = u16::from_le_bytes([bytes[5], bytes[6]]) as usize;
        if length != bytes.len() - 7 {
            return Err(Error::Length);
        }
        let frame = Self {
            kind,
            txn: u16::from_le_bytes([bytes[1], bytes[2]]),
            id: u16::from_le_bytes([bytes[3], bytes[4]]),
            body: &bytes[7..],
        };
        // Reject duplicate TLVs, including unknown optional ones; do not let parsers
        // disagree about first/last occurrence. Validate once before reading fields.
        let mut seen = [false; 256];
        let mut rest = frame.body;
        while !rest.is_empty() {
            if rest.len() < 3 {
                return Err(Error::Truncated);
            }
            let tag = rest[0] as usize;
            if seen[tag] {
                return Err(Error::Duplicate);
            }
            seen[tag] = true;
            let n = u16::from_le_bytes([rest[1], rest[2]]) as usize;
            rest = rest
                .get(3..3 + n)
                .and_then(|_| rest.get(3 + n..))
                .ok_or(Error::Truncated)?;
        }
        Ok(frame)
    }
    pub fn tlv(&self, tag: u8) -> Option<&'a [u8]> {
        let mut rest = self.body;
        while rest.len() >= 3 {
            let n = u16::from_le_bytes([rest[1], rest[2]]) as usize;
            if rest[0] == tag {
                return rest.get(3..3 + n);
            }
            rest = rest.get(3 + n..)?;
        }
        None
    }
    pub fn required(&self, tag: u8) -> Result<&'a [u8], Error> {
        self.tlv(tag).ok_or(Error::Missing)
    }
    /// Iterate TLVs in wire order. `parse` has already validated the framing,
    /// so a short trailing fragment simply ends the iteration.
    pub fn tlvs(&self) -> impl Iterator<Item = (u8, &'a [u8])> {
        let mut rest = self.body;
        std::iter::from_fn(move || {
            if rest.len() < 3 {
                return None;
            }
            let n = u16::from_le_bytes([rest[1], rest[2]]) as usize;
            let (tag, value) = (rest[0], rest.get(3..3 + n)?);
            rest = &rest[3 + n..];
            Some((tag, value))
        })
    }
    pub fn optional_u32(&self, tag: u8) -> Result<Option<u32>, Error> {
        self.tlv(tag).map(u32_value).transpose()
    }
}
pub fn u32_value(bytes: &[u8]) -> Result<u32, Error> {
    Ok(u32::from_le_bytes(
        bytes.try_into().map_err(|_| Error::Length)?,
    ))
}
pub fn u8_value(bytes: &[u8]) -> Result<u8, Error> {
    if bytes.len() == 1 {
        Ok(bytes[0])
    } else {
        Err(Error::Length)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PdnType {
    Ims,
    Emergency,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    V4,
    V6,
}
// Deliberately no Debug: APN and network metadata must not enter logs.
#[derive(Clone, PartialEq, Eq)]
pub struct Activation {
    pub apn: Vec<u8>,
    pub pdn_type: PdnType,
    pub family: Family,
    pub rat: u32,
    pub profile: u32,
    pub cookie: Option<u32>,
    pub subscription: Option<u32>,
    pub slot: i32,
    pub instance: Option<u32>,
}
impl Activation {
    pub fn decode(frame: &Frame<'_>, slots: u32) -> Result<Self, Error> {
        let p = frame.required(1)?;
        let n = *p.first().ok_or(Error::Truncated)? as usize;
        if n > 100 || p.len() != n + 17 {
            return Err(Error::Length);
        }
        let apn = &p[1..1 + n];
        if !apn
            .iter()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'-'))
        {
            return Err(Error::Value);
        }
        let o = n + 1;
        let pdn_type = match u32_value(&p[o..o + 4])? {
            0 => PdnType::Ims,
            2 => PdnType::Emergency,
            _ => return Err(Error::Value),
        };
        let family = match u32_value(&p[o + 8..o + 12])? {
            0 => Family::V4,
            1 => Family::V6,
            _ => return Err(Error::Value),
        };
        let slot = match frame.optional_u32(0x12)? {
            None | Some(0) => -1,
            Some(s) if s <= slots => (s - 1) as i32,
            _ => return Err(Error::Value),
        };
        // Only emergency service may use the stock no-subscription fallback.
        // Reject unsupported slotless IMS before allocating daemon state.
        if slot < 0 && pdn_type != PdnType::Emergency {
            return Err(Error::Value);
        }
        Ok(Self {
            apn: apn.iter().map(u8::to_ascii_lowercase).collect(),
            pdn_type,
            family,
            rat: u32_value(&p[o + 4..o + 8])?,
            profile: u32_value(&p[o + 12..o + 16])?,
            cookie: frame.optional_u32(0x10)?,
            subscription: frame.optional_u32(0x11)?,
            slot,
            instance: frame.optional_u32(0x13)?,
        })
    }
    pub fn same_pdn(&self, rhs: &Self) -> bool {
        self.apn == rhs.apn
            && self.pdn_type == rhs.pdn_type
            && self.family == rhs.family
            && self.slot == rhs.slot
            && self.instance == rhs.instance
            && self.profile == rhs.profile
            && self.rat == rhs.rat
    }
}
pub struct Encoder {
    bytes: Vec<u8>,
}
impl Encoder {
    pub fn new(kind: Kind, txn: u16, id: u16) -> Self {
        let mut bytes = vec![kind as u8];
        bytes.extend(txn.to_le_bytes());
        bytes.extend(id.to_le_bytes());
        bytes.extend([0, 0]);
        Self { bytes }
    }
    pub fn tlv(&mut self, tag: u8, value: &[u8]) -> Result<(), Error> {
        if self.bytes.len() + 3 + value.len() > MAX_DATAGRAM {
            return Err(Error::Length);
        }
        self.bytes.push(tag);
        self.bytes.extend((value.len() as u16).to_le_bytes());
        self.bytes.extend(value);
        Ok(())
    }
    pub fn finish(mut self) -> Vec<u8> {
        let n = (self.bytes.len() - 7) as u16;
        self.bytes[5..7].copy_from_slice(&n.to_le_bytes());
        self.bytes
    }
}
pub fn response(txn: u16, id: u16, result: u16, error: u16) -> Vec<u8> {
    let mut e = Encoder::new(Kind::Response, txn, id);
    let mut v = result.to_le_bytes().to_vec();
    v.extend(error.to_le_bytes());
    let _ = e.tlv(2, &v);
    e.finish()
}
pub fn activation_response(txn: u16, pdp: u8, a: &Activation) -> Vec<u8> {
    let mut e = Encoder::new(Kind::Response, txn, ACTIVATE);
    let _ = e.tlv(2, &[0, 0, 0, 0]);
    let _ = e.tlv(0x10, &[pdp]);
    let _ = e.tlv(0x11, &a.cookie.unwrap_or(0).to_le_bytes());
    if let Some(instance) = a.instance {
        let _ = e.tlv(0x12, &instance.to_le_bytes());
    }
    e.finish()
}
pub fn indication(
    txn: u16,
    pdp: u8,
    a: &Activation,
    address: Option<IpAddr>,
    change: bool,
) -> Vec<u8> {
    let mut e = Encoder::new(
        Kind::Indication,
        txn,
        if change { ADDRESS_CHANGE } else { ACTIVATE },
    );
    if !change {
        let _ = e.tlv(2, &[0, 0, if address.is_some() { 0 } else { 13 }, 0]);
    }
    let _ = e.tlv(1, &[pdp]);
    if !change {
        if let Some(cookie) = a.cookie {
            let _ = e.tlv(0x10, &cookie.to_le_bytes());
        }
    }
    if let Some(ip) = address {
        let text = ip.to_string();
        let mut v = (if ip.is_ipv4() { 0u32 } else { 1 }).to_le_bytes().to_vec();
        v.push(text.len() as u8);
        v.extend(text.as_bytes());
        let _ = e.tlv(if change { 0x10 } else { 0x11 }, &v);
    }
    let _ = e.tlv(
        if change { 0x11 } else { 0x12 },
        &a.instance.unwrap_or(0).to_le_bytes(),
    );
    e.finish()
}

/// Validate every known request against the observed IDL 1.22 field widths.
/// Unknown optional fields are ignored for forward compatibility. Unknown
/// mandatory fields, duplicates and trailing bytes in known fields are rejected.
pub fn validate_request(f: &Frame<'_>) -> Result<(), Error> {
    // (tag, exact bytes; 0 means counted blob, 255 means link-address aggregate).
    let fields: &[(u8, usize)] = match f.id {
        0x20 => &[(1, 254), (0x10, 4), (0x11, 4), (0x12, 4), (0x13, 4)],
        0x21 | 0x22 => &[(1, 1), (0x10, 4)],
        0x23 => &[(1, 255), (0x10, 4), (0x11, 4), (0x12, 2)],
        0x25 => &[
            (1, 1),
            (2, 4),
            (3, 4),
            (4, 4),
            (5, 4),
            (6, 4),
            (7, 4),
            (8, 4),
            (9, 4),
            (10, 4),
            (11, 4),
        ],
        0x26 | 0x2d => &[(1, 1)],
        0x27 | 0x2a | 0x32 => &[(1, 1), (2, 4)],
        0x28 => &[
            (1, 1),
            (2, 4),
            (3, 4),
            (4, 4),
            (5, 4),
            (6, 4),
            (0x10, 4),
            (0x11, 4),
            (0x12, 16),
            (0x13, 16),
            (0x14, 4),
        ],
        0x29 => &[(1, 1), (2, 4), (3, 4), (4, 4)],
        0x2c => &[(1, 1), (2, 4), (3, 4)],
        0x2e => &[(0x10, 1), (0x11, 4)],
        0x31 => &[(0x10, 0), (0x11, 4)],
        0x33 => &[(1, 4)],
        0x34 => &[(1, 8), (0x10, 4)],
        _ => return Err(Error::Value),
    };
    for &(tag, width) in fields {
        let Some(value) = f.tlv(tag) else {
            if tag < 0x10 {
                return Err(Error::Missing);
            }
            continue;
        };
        match width {
            254 => {
                let n = *value.first().ok_or(Error::Truncated)? as usize;
                if n > 100 || value.len() != n + 17 {
                    return Err(Error::Length);
                }
            }
            255 => {
                if value.len() < 7 {
                    return Err(Error::Truncated);
                }
                let n = value[6] as usize;
                if n > 40 || value.len() != 7 + n {
                    return Err(Error::Length);
                }
                let family = u32_value(&value[2..6])?;
                let text = std::str::from_utf8(&value[7..]).map_err(|_| Error::Value)?;
                let ip: IpAddr = text.parse().map_err(|_| Error::Value)?;
                if (family == 0 && !ip.is_ipv4()) || (family == 1 && !ip.is_ipv6()) || family > 1 {
                    return Err(Error::Value);
                }
            }
            0 => {
                if value.len() < 2 {
                    return Err(Error::Truncated);
                }
                let n = u16::from_le_bytes([value[0], value[1]]) as usize;
                if n > 1024 || n + 2 != value.len() {
                    return Err(Error::Length);
                }
            }
            _ => {
                if value.len() != width {
                    return Err(Error::Length);
                }
            }
        }
    }
    for tag in 0..0x10 {
        if f.tlv(tag).is_some() && !fields.iter().any(|(t, _)| *t == tag) {
            return Err(Error::Value);
        }
    }
    Ok(())
}

/// Verified 0x32 response: result, PDP echo, sequence echo, 24-byte calendar.
pub fn timezone_response(
    txn: u16,
    pdp: u8,
    sequence: u32,
    fields: [u16; 8],
    utc_seconds: u64,
) -> Result<Vec<u8>, Error> {
    if fields[0] > 60
        || fields[1] > 59
        || fields[2] > 23
        || !(1..=31).contains(&fields[3])
        || !(1..=12).contains(&fields[4])
        || !(1970..=9999).contains(&fields[5])
        || fields[6] > 6
        || !(-56..=56).contains(&(fields[7] as i16))
        || utc_seconds == 0
    {
        return Err(Error::Value);
    }
    let mut e = Encoder::new(Kind::Response, txn, 0x32);
    e.tlv(2, &[0; 4])?;
    e.tlv(3, &[pdp])?;
    e.tlv(4, &sequence.to_le_bytes())?;
    let mut value = Vec::with_capacity(24);
    for v in fields {
        value.extend(v.to_le_bytes());
    }
    value.extend(utc_seconds.to_le_bytes());
    e.tlv(5, &value)?;
    Ok(e.finish())
}
