// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! Checks on the broker's Binder reports before they reach the engine. Pure,
//! so host tests and fuzzing run the same code as imsdcmd.
use diamaneos_ims_dcm::{
    engine::{Key, Network, Request},
    protocol::PdnType,
};
use std::net::{Ipv4Addr, Ipv6Addr};

/// The request a report names. `kind` is None for a PDN type the daemon does
/// not serve. None: reject the call.
pub fn request(slot: i32, kind: Option<PdnType>, serial: i32) -> Option<Request> {
    let kind = kind?;
    if !(-1..=2).contains(&slot) || serial <= 0 {
        return None;
    }
    Some(Request {
        key: Key { slot, kind },
        serial,
    })
}

/// The network of an up report. Each address is empty or exactly 4 (IPv4) or
/// 16 (IPv6) bytes in network order. None: reject the call.
pub fn network(handle: i64, ipv4: &[u8], ipv6: &[u8], mtu: i32) -> Option<Network> {
    let v4 = match ipv4 {
        [] => None,
        b => Some(Ipv4Addr::from(<[u8; 4]>::try_from(b).ok()?)),
    };
    let v6 = match ipv6 {
        [] => None,
        b => Some(Ipv6Addr::from(<[u8; 16]>::try_from(b).ok()?)),
    };
    let n = Network {
        handle: handle as u64,
        v4,
        v6,
        mtu: mtu as u32,
    };
    if handle == 0 || mtu < 0 || !n.valid() {
        return None;
    }
    Some(n)
}
