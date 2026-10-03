// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! Offline DSD STA-status codec. No sockets, Android permissions or service entry point.
//! See docs/wlan-reporting-protocol.md for the observed wire subset and its limits.
#![forbid(unsafe_code)]

use diamaneos_ims_dcm::protocol::{Frame, Kind};
use std::net::{Ipv4Addr, Ipv6Addr};

pub const DSD_SERVICE: u32 = 0x2a;
pub const DSD_MAJOR_VERSION: u32 = 1;
pub const WLAN_STATUS: u16 = 0x20;
pub const BIND_SUBSCRIPTION: u16 = 0x27;
pub const DATA_SETTINGS: u16 = 0x34;
pub const INDICATION_REGISTRATION: u16 = 0x38;
pub const NAT_KEEPALIVE_INDICATION: u16 = 0x41;
pub const NAT_KEEPALIVE_OPERATION_STATUS: u16 = 0x42;
pub const DEFAULT_PROFILE_STATUS: u16 = 0x43;

pub mod keepalive;
pub mod profile_notice;
pub mod profiles;
pub mod session;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidBssid,
    InvalidAddress,
    MissingAddress,
    InvalidDnsAddress,
    ZeroTransaction,
    MalformedResponse,
    WrongTransaction,
    ModemFailure(u16),
}

/// A checked current connected-network observation, not proof of carrier usability.
/// Deliberately has no Debug/Display implementation: identifiers must not enter logs.
#[derive(Clone, PartialEq, Eq)]
pub struct Connected {
    bssid: [u8; 6],
    ipv4: Option<Ipv4Addr>,
    ipv6: Option<(Ipv6Addr, u8)>,
    validated: bool,
    default_route: bool,
    dns4: [Option<Ipv4Addr>; 2],
    dns6: [Option<Ipv6Addr>; 2],
}

impl Connected {
    pub fn new(
        bssid: [u8; 6],
        ipv4: Option<Ipv4Addr>,
        ipv6: Option<(Ipv6Addr, u8)>,
        validated: bool,
    ) -> Result<Self, Error> {
        // Locally administered unicast BSSIDs are legitimate. Reject the Android
        // redaction sentinel, all-zero identifier, multicast and broadcast.
        if bssid == [0; 6] || bssid == [2, 0, 0, 0, 0, 0] || bssid[0] & 1 != 0 {
            return Err(Error::InvalidBssid);
        }
        if ipv4.is_none() && ipv6.is_none() {
            return Err(Error::MissingAddress);
        }
        if let Some(ip) = ipv4 {
            if ip.is_unspecified()
                || ip.is_loopback()
                || ip.is_multicast()
                || ip.is_broadcast()
                || ip.is_link_local()
                || ip.octets()[0] == 0
                || ip.octets()[0] >= 240
            {
                return Err(Error::InvalidAddress);
            }
        }
        if let Some((ip, prefix)) = ipv6 {
            if prefix > 128
                || ip.is_unspecified()
                || ip.is_loopback()
                || ip.is_multicast()
                || ip.is_unicast_link_local()
                || ip.to_ipv4_mapped().is_some()
            {
                return Err(Error::InvalidAddress);
            }
        }
        Ok(Self {
            bssid,
            ipv4,
            ipv6,
            validated,
            default_route: false,
            dns4: [None; 2],
            dns6: [None; 2],
        })
    }

    /// Whether Android currently selects this Wi-Fi transport as the default,
    /// including an unambiguous Wi-Fi-backed VPN. This does not alter routing.
    pub fn with_default_route(mut self, default_route: bool) -> Self {
        self.default_route = default_route;
        self
    }

    /// Resolvers observed on this same Wi-Fi link. No resolution or fallback.
    pub fn with_dns(
        mut self,
        dns4: [Option<Ipv4Addr>; 2],
        dns6: [Option<Ipv6Addr>; 2],
    ) -> Result<Self, Error> {
        if (dns4[0].is_none() && dns4[1].is_some())
            || (dns6[0].is_none() && dns6[1].is_some())
            || (dns4[0].is_some() && dns4[0] == dns4[1])
            || (dns6[0].is_some() && dns6[0] == dns6[1])
        {
            return Err(Error::InvalidDnsAddress);
        }
        for ip in dns4.iter().flatten() {
            if ip.is_unspecified()
                || ip.is_loopback()
                || ip.is_multicast()
                || ip.is_broadcast()
                || ip.octets()[0] == 0
                || ip.octets()[0] >= 240
            {
                return Err(Error::InvalidDnsAddress);
            }
        }
        for ip in dns6.iter().flatten() {
            if ip.is_unspecified()
                || ip.is_loopback()
                || ip.is_multicast()
                || ip.to_ipv4_mapped().is_some()
            {
                return Err(Error::InvalidDnsAddress);
            }
        }
        self.dns4 = dns4;
        self.dns6 = dns6;
        Ok(self)
    }

    pub fn encode(&self, transaction: u16) -> Result<Vec<u8>, Error> {
        let mut body = Vec::with_capacity(110);
        tlv(&mut body, 1, &self.bssid);
        if let Some(ip) = self.ipv4 {
            // Stock reverses in_addr before the IDL uint32 encoder. IPv4 is a
            // little-endian numeric address on this interface; IPv6 is octets.
            tlv(
                &mut body,
                0x10,
                &u32::from_be_bytes(ip.octets()).to_le_bytes(),
            );
        }
        if let Some((ip, prefix)) = self.ipv6 {
            let mut value = [0; 17];
            value[..16].copy_from_slice(&ip.octets());
            value[16] = prefix;
            tlv(&mut body, 0x11, &value);
        }
        for (index, ip) in self.dns4.iter().enumerate() {
            if let Some(ip) = ip {
                tlv(
                    &mut body,
                    0x13 + index as u8,
                    &u32::from_be_bytes(ip.octets()).to_le_bytes(),
                );
            }
        }
        for (index, ip) in self.dns6.iter().enumerate() {
            if let Some(ip) = ip {
                tlv(&mut body, 0x15 + index as u8, &ip.octets());
            }
        }
        // Stock STA mode = 2. State 1 means connected but not validated;
        // state 2 means connected and Android-validated. Never manufacture 2.
        tlv(&mut body, 0x1f, &2_u32.to_le_bytes());
        tlv(
            &mut body,
            0x21,
            &(if self.validated { 2_u32 } else { 1_u32 }).to_le_bytes(),
        );
        tlv(&mut body, 0x24, &[u8::from(self.default_route)]);
        frame(transaction, body)
    }

    /// Withdraw this particular previously reported network, preserving its real
    /// BSSID in the mandatory field. This does not encode an unknown cold-start
    /// state; startup reconciliation remains an integration requirement.
    pub fn encode_withdrawal(&self, transaction: u16) -> Result<Vec<u8>, Error> {
        withdrawal(transaction, self.bssid)
    }
}

/// Explicit STA-disconnected state, not a fabricated connected-network identity.
/// A zero BSSID is used only when reconciling an unknown previous observation.
/// This cold-start form requires firmware qualification before release.
pub fn withdrawal(transaction: u16, previous_bssid: [u8; 6]) -> Result<Vec<u8>, Error> {
    let mut body = Vec::with_capacity(23);
    tlv(&mut body, 1, &previous_bssid);
    tlv(&mut body, 0x1f, &2_u32.to_le_bytes());
    tlv(&mut body, 0x21, &0_u32.to_le_bytes());
    tlv(&mut body, 0x24, &[0]);
    frame(transaction, body)
}

pub fn bind_subscription(transaction: u16, subscription: u32) -> Result<Vec<u8>, Error> {
    if !(1..=2).contains(&subscription) {
        return Err(Error::MalformedResponse);
    }
    let mut body = Vec::with_capacity(7);
    tlv(&mut body, 1, &subscription.to_le_bytes());
    request(transaction, BIND_SUBSCRIPTION, body)
}

pub fn wifi_switch(transaction: u16, enabled: bool) -> Result<Vec<u8>, Error> {
    let mut body = Vec::with_capacity(4);
    // Stock sendInitialModemNotifications writes valid at offset 12 and the
    // observed Wi-Fi switch boolean at offset 13: IDL optional TLV 0x13.
    tlv(&mut body, 0x13, &[u8::from(enabled)]);
    request(transaction, DATA_SETTINGS, body)
}

/// PRIVATE DIAGNOSTIC: the two optional byte fields enabled in both stock
/// subscription contexts. Their event/service-readiness meaning is not proven.
/// The source observes headers only; it emits no quality or capability verdict.
/// Never select this path in a release runtime or accept caller-chosen fields.
pub fn diagnostic_notification_registration(transaction: u16) -> Result<Vec<u8>, Error> {
    let mut body = Vec::with_capacity(8);
    tlv(&mut body, 0x12, &[1]);
    tlv(&mut body, 0x14, &[1]);
    request(transaction, INDICATION_REGISTRATION, body)
}

/// A structurally valid stock keepalive instruction. Endpoints and timer values
/// are neither interpreted nor retained: the adapter cannot perform this operation.
/// The authenticated runtime must establish the current bound modem peer first.
pub fn keepalive_instruction(bytes: &[u8]) -> bool {
    // Stock IDL: at most47 body bytes. Required operation byte, optional
    // destination IPv4/IPv6, destination/source ports and timer. No extensions.
    if bytes.len() > 54 {
        return false;
    }
    let Ok(frame) = Frame::parse(bytes) else {
        return false;
    };
    if frame.kind != Kind::Indication
        || frame.id != NAT_KEEPALIVE_INDICATION
        || frame.tlv(1).is_none_or(|value| value.len() != 1)
    {
        return false;
    }
    let mut rest = &bytes[7..];
    while !rest.is_empty() {
        let expected = match rest[0] {
            1 => 1,
            0x10 | 0x14 => 4,
            0x11 => 16,
            0x12 | 0x13 => 2,
            _ => return false,
        };
        let length = u16::from_le_bytes([rest[1], rest[2]]) as usize;
        if length != expected {
            return false;
        }
        rest = &rest[3 + length..];
    }
    true
}

/// Stock maps a failed operation to uint32 1. This does not name an unsupported
/// enum, claim successful cancellation, echo an endpoint or emit keepalive traffic.
pub fn keepalive_operation_failed(transaction: u16) -> Result<Vec<u8>, Error> {
    let mut body = Vec::with_capacity(7);
    tlv(&mut body, 1, &1_u32.to_le_bytes());
    request(transaction, NAT_KEEPALIVE_OPERATION_STATUS, body)
}

/// Stock's default connectivity profile only; not a signal/throughput measurement
/// and never a caller-selected profile. IDL: required uint32 profile, optional
/// uint32 status (0 met, 1 not met). The stock default profile is always ID 0.
pub fn default_profile_status(transaction: u16, connected_default: bool) -> Result<Vec<u8>, Error> {
    let mut body = Vec::with_capacity(14);
    tlv(&mut body, 1, &0_u32.to_le_bytes());
    tlv(
        &mut body,
        0x10,
        &u32::from(!connected_default).to_le_bytes(),
    );
    request(transaction, DEFAULT_PROFILE_STATUS, body)
}

fn tlv(body: &mut Vec<u8>, tag: u8, value: &[u8]) {
    // All callers are private and provide fixed-size values of at most 17 bytes.
    body.push(tag);
    body.extend_from_slice(&(value.len() as u16).to_le_bytes());
    body.extend_from_slice(value);
}

fn frame(transaction: u16, body: Vec<u8>) -> Result<Vec<u8>, Error> {
    request(transaction, WLAN_STATUS, body)
}

fn request(transaction: u16, message: u16, body: Vec<u8>) -> Result<Vec<u8>, Error> {
    if transaction == 0 {
        return Err(Error::ZeroTransaction);
    }
    let mut out = Vec::with_capacity(7 + body.len());
    out.push(Kind::Request as u8);
    out.extend_from_slice(&transaction.to_le_bytes());
    out.extend_from_slice(&message.to_le_bytes());
    out.extend_from_slice(&(body.len() as u16).to_le_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

/// Validate the fixed result-only response observed for WLAN_STATUS. The caller
/// must separately authenticate the QRTR peer and modem generation. This codec
/// cannot prove that an acknowledged state was acted on by modem firmware.
pub fn response(bytes: &[u8], expected_transaction: u16) -> Result<(), Error> {
    response_for(bytes, expected_transaction, WLAN_STATUS)
}

pub fn response_for(bytes: &[u8], expected_transaction: u16, message: u16) -> Result<(), Error> {
    if expected_transaction == 0 {
        return Err(Error::ZeroTransaction);
    }
    // Header(7), result TLV header(3), two uint16 result fields(4).
    // Reject extra fields rather than silently extending the accepted protocol.
    if bytes.len() != 14 {
        return Err(Error::MalformedResponse);
    }
    let parsed = Frame::parse(bytes).map_err(|_| Error::MalformedResponse)?;
    if !matches!(
        message,
        WLAN_STATUS
            | BIND_SUBSCRIPTION
            | DATA_SETTINGS
            | INDICATION_REGISTRATION
            | NAT_KEEPALIVE_OPERATION_STATUS
            | DEFAULT_PROFILE_STATUS
    ) || parsed.kind != Kind::Response
        || parsed.id != message
    {
        return Err(Error::MalformedResponse);
    }
    if parsed.txn != expected_transaction {
        return Err(Error::WrongTransaction);
    }
    let value = parsed.tlv(2).ok_or(Error::MalformedResponse)?;
    if value.len() != 4 {
        return Err(Error::MalformedResponse);
    }
    let result = u16::from_le_bytes([value[0], value[1]]);
    let error = u16::from_le_bytes([value[2], value[3]]);
    match (result, error) {
        (0, 0) => Ok(()),
        (1, e) if e != 0 => Err(Error::ModemFailure(e)),
        _ => Err(Error::MalformedResponse),
    }
}
