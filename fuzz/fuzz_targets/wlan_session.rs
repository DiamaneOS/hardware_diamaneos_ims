// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! One DSD session (wlan/src/session.rs) driven by modem packets, observer
//! snapshots and the clock. Packets may echo the session's own transactions,
//! so the fuzzer reaches every acknowledged step.
//! Checks: no panic or unbounded recursion, every packet is a valid request
//! with a nonzero, never reused transaction id.
#![no_main]

use arbitrary::Arbitrary;
use diamaneos_ims_dcm::protocol::{Encoder, Frame, Kind};
use diamaneos_wlan_reporting::session::{Observation, Session};
use diamaneos_wlan_reporting::Connected;
use libfuzzer_sys::fuzz_target;
use std::collections::BTreeMap;
use std::net::{Ipv4Addr, Ipv6Addr};

#[derive(Arbitrary, Debug)]
enum Op {
    /// Acknowledge (result 0) or reject the last sent packet.
    Ack { which: u8, error: u16 },
    Raw(Vec<u8>),
    Indication { id: u8, tlvs: Vec<(u8, Vec<u8>)> },
    Observe { enabled: bool, network: Option<Net> },
    Poll(u16),
}

#[derive(Arbitrary, Debug)]
struct Net {
    bssid: [u8; 6],
    v4: Option<[u8; 4]>,
    v6: Option<([u8; 16], u8)>,
    validated: bool,
    default_route: bool,
    dns4: [Option<[u8; 4]>; 2],
    dns6: [Option<[u8; 16]>; 2],
}

#[derive(Arbitrary, Debug)]
struct Input {
    subscription: u8,
    generation: u64,
    ops: Vec<Op>,
}

fn connected(n: Net) -> Option<Connected> {
    Connected::new(
        n.bssid,
        n.v4.map(Ipv4Addr::from),
        n.v6.map(|(a, p)| (Ipv6Addr::from(a), p)),
        n.validated,
    )
    .ok()?
    .with_default_route(n.default_route)
    .with_dns(n.dns4.map(|a| a.map(Ipv4Addr::from)), n.dns6.map(|a| a.map(Ipv6Addr::from)))
    .ok()
}

fuzz_target!(|input: Input| {
    let Some(mut session) = Session::new(u32::from(input.subscription % 3), input.generation)
    else {
        return;
    };
    let mut now = 0u64;
    let mut sent: Vec<Vec<u8>> = Vec::new();
    let mut used: BTreeMap<u16, Vec<u8>> = BTreeMap::new();
    for op in input.ops.into_iter().take(512) {
        match op {
            Op::Ack { which, error } => {
                let Some(packet) = sent.get(usize::from(which) % sent.len().max(1)) else {
                    continue;
                };
                let mut e = Encoder::new(Kind::Response, u16::from_le_bytes([packet[1], packet[2]]),
                    u16::from_le_bytes([packet[3], packet[4]]));
                let v = if error == 0 { [0, 0, 0, 0] } else {
                    let b = error.to_le_bytes();
                    [1, 0, b[0], b[1]]
                };
                e.tlv(2, &v).unwrap();
                session.receive(&e.finish());
            }
            Op::Raw(bytes) => session.receive(&bytes),
            Op::Indication { id, tlvs } => {
                let ids = [0x41u16, 0x45, 0x3f];
                let mut e = Encoder::new(Kind::Indication, 0, ids[usize::from(id) % 3]);
                for (t, v) in tlvs.iter().take(8) {
                    if e.tlv(*t, &v[..v.len().min(16)]).is_err() {
                        break;
                    }
                }
                session.receive(&e.finish());
            }
            Op::Observe { enabled, network } => session.observe(Observation {
                enabled,
                network: network.and_then(connected),
            }),
            Op::Poll(ms) => {
                now = now.saturating_add(u64::from(ms));
                for packet in session.poll_cycle(now).into_iter().flatten() {
                    let f = Frame::parse(&packet).expect("session output parses");
                    assert_eq!(f.kind, Kind::Request);
                    assert_ne!(f.txn, 0);
                    // A retry repeats the same bytes. A new packet takes a new id.
                    match used.get(&f.txn) {
                        Some(earlier) => assert_eq!(earlier, &packet, "transaction reused"),
                        None => {
                            used.insert(f.txn, packet.clone());
                        }
                    }
                    if sent.len() >= 64 {
                        sent.remove(0);
                    }
                    sent.push(packet);
                }
            }
        }
        let _ = (session.diagnostics(), session.settled(), session.failed());
    }
});
