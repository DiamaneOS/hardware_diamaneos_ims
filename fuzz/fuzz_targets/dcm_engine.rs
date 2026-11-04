// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! The DCM engine driven by everything imsdcmd feeds it: modem datagrams
//! (structured and raw), broker Binder reports with unchecked network values,
//! QRTR control notices (client and node loss) and deferred-output completion.
//! Checks: no panic, every output is a valid bounded frame, state stays bounded.
#![no_main]

use arbitrary::Arbitrary;
use diamaneos_ims_dcm::engine::{Effect, Engine, Network, Peer, Request, MAX_SESSIONS};
use diamaneos_ims_dcm::outgoing::{Disposition, Retained};
use diamaneos_ims_dcm::protocol::{Encoder, Frame, Kind, ACTIVATE, DEACTIVATE, MAX_DATAGRAM};
use libfuzzer_sys::fuzz_target;
use std::net::{Ipv4Addr, Ipv6Addr};

const MODEM_NODE: u32 = 3;
// Retained output the harness holds, like the daemon's bounded outbox.
const MAX_HELD: usize = 4 * 4 * MAX_SESSIONS;

#[derive(Arbitrary, Debug)]
struct Who {
    node: u8,
    port: u8,
}
impl Who {
    fn peer(&self) -> Peer {
        let node = if self.node % 4 == 0 { u32::from(self.node) } else { MODEM_NODE };
        let port = match self.port % 8 {
            0 => 0,
            1 => 0xffff_fffe,
            2 => 0xffff_ffff,
            p => 100 + u32::from(p) + u32::from(self.port / 64),
        };
        Peer { node, port }
    }
}

#[derive(Arbitrary, Debug)]
enum Op {
    Raw(Who, Vec<u8>),
    Activate {
        who: Who,
        txn: u16,
        apn: u8,
        pdn: u8,
        family: u8,
        rat: u32,
        profile: u8,
        cookie: Option<u32>,
        subscription: Option<u32>,
        slot: Option<u8>,
        instance: Option<u8>,
    },
    Deactivate { who: Who, txn: u16, pdp: u8, instance: Option<u8> },
    Destroy { who: Who, txn: u16, instance: u8 },
    Time { who: Who, txn: u16, pdp: u8, sequence: u32 },
    Simple { who: Who, txn: u16, id: u8, body: Vec<(u8, Vec<u8>)> },
    BrokerConnected,
    BrokerLost,
    Report { which: u8, serial: Option<i32>, network: Option<Net> },
    Failed { which: u8 },
    BringUpFailed { which: u8 },
    ReleaseFailed,
    Emergency(bool),
    PeerGone(Who),
    PeerStalled(Who),
    NodeGone(u8),
    Shutdown,
    Finish { index: u16, how: u8 },
    Calendar { which: u8, fields: [u16; 8], seconds: u64 },
}

#[derive(Arbitrary, Debug)]
struct Net {
    handle: u8,
    v4: Option<[u8; 4]>,
    v6: Option<[u8; 16]>,
    mtu: u32,
}

#[derive(Arbitrary, Debug)]
struct Input {
    slots: u8,
    ops: Vec<Op>,
}

struct Harness {
    engine: Engine,
    requests: Vec<Request>,
    held: Vec<Retained>,
    reads: Vec<(Peer, u16, u8, u32)>,
}

impl Harness {
    fn apply(&mut self, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::Send(peer, packet) => {
                    assert!(packet.len() <= MAX_DATAGRAM);
                    let frame = Frame::parse(&packet).expect("engine output parses");
                    assert!(frame.kind != Kind::Request);
                    if let Some(retained) = self.engine.retain_output(peer, packet) {
                        if self.held.len() >= MAX_HELD {
                            self.engine.finish_output(retained, Disposition::Obsolete);
                        } else {
                            self.held.push(retained);
                        }
                    }
                }
                Effect::BringUp(r) | Effect::Release(r) | Effect::ReleaseAfter { request: r, .. } => {
                    if self.requests.len() >= 16 {
                        self.requests.remove(0);
                    }
                    self.requests.push(r);
                }
                Effect::ReadTime { peer, txn, pdp, sequence } => {
                    if self.reads.len() >= 16 {
                        self.reads.remove(0);
                    }
                    self.reads.push((peer, txn, pdp, sequence));
                }
            }
        }
        let d = self.engine.diagnostics();
        assert!(self.engine.session_count() <= MAX_SESSIONS);
        assert!(d.active_groups <= 8);
        assert!(d.held_groups <= d.active_groups);
    }
    fn request(&self, which: u8) -> Option<Request> {
        if self.requests.is_empty() {
            return None;
        }
        Some(self.requests[usize::from(which) % self.requests.len()])
    }
}

fn frame(txn: u16, id: u16, tlvs: &[(u8, Vec<u8>)]) -> Vec<u8> {
    let mut e = Encoder::new(Kind::Request, txn, id);
    for (tag, value) in tlvs {
        if e.tlv(*tag, value).is_err() {
            break;
        }
    }
    e.finish()
}

fn run(input: Input) {
    let slots = u32::from(input.slots % 3) + 1;
    let mut h = Harness {
        engine: Engine::new(MODEM_NODE, slots).unwrap(),
        requests: vec![],
        held: vec![],
        reads: vec![],
    };
    for op in input.ops.into_iter().take(256) {
        let effects = match op {
            Op::Raw(who, bytes) => h.engine.receive(who.peer(), &bytes),
            Op::Activate { who, txn, apn, pdn, family, rat, profile, cookie, subscription, slot, instance } => {
                let apn: &[u8] = match apn % 4 {
                    0 => b"ims",
                    1 => b"sos",
                    2 => b"IMS.Example-1",
                    _ => b"",
                };
                let mut v = vec![apn.len() as u8];
                v.extend(apn);
                v.extend(u32::from(pdn % 4).to_le_bytes());
                v.extend(rat.to_le_bytes());
                v.extend(u32::from(family % 3).to_le_bytes());
                v.extend(u32::from(profile % 4).to_le_bytes());
                let mut tlvs = vec![(1, v)];
                if let Some(c) = cookie {
                    tlvs.push((0x10, c.to_le_bytes().to_vec()));
                }
                if let Some(s) = subscription {
                    tlvs.push((0x11, s.to_le_bytes().to_vec()));
                }
                if let Some(s) = slot {
                    tlvs.push((0x12, u32::from(s % 5).to_le_bytes().to_vec()));
                }
                if let Some(i) = instance {
                    tlvs.push((0x13, u32::from(i % 4).to_le_bytes().to_vec()));
                }
                h.engine.receive(who.peer(), &frame(txn, ACTIVATE, &tlvs))
            }
            Op::Deactivate { who, txn, pdp, instance } => {
                let mut tlvs = vec![(1, vec![20 + pdp % 82])];
                if let Some(i) = instance {
                    tlvs.push((0x10, u32::from(i % 4).to_le_bytes().to_vec()));
                }
                h.engine.receive(who.peer(), &frame(txn, DEACTIVATE, &tlvs))
            }
            Op::Destroy { who, txn, instance } => h.engine.receive(
                who.peer(),
                &frame(txn, 0x33, &[(1, u32::from(instance % 4).to_le_bytes().to_vec())]),
            ),
            Op::Time { who, txn, pdp, sequence } => h.engine.receive(
                who.peer(),
                &frame(txn, 0x32, &[(1, vec![pdp]), (2, sequence.to_le_bytes().to_vec())]),
            ),
            Op::Simple { who, txn, id, body } => {
                h.engine.receive(who.peer(), &frame(txn, 0x20 + u16::from(id % 0x18), &body))
            }
            Op::BrokerConnected => h.engine.broker_connected(),
            Op::BrokerLost => h.engine.broker_lost(),
            Op::Report { which, serial, network } => {
                let Some(mut r) = h.request(which) else { continue };
                if let Some(s) = serial {
                    r.serial = s;
                }
                let network = network.map(|n| Network {
                    handle: u64::from(n.handle),
                    v4: n.v4.map(Ipv4Addr::from),
                    v6: n.v6.map(Ipv6Addr::from),
                    mtu: n.mtu,
                });
                h.engine.report(r, network)
            }
            Op::Failed { which } => match h.request(which) {
                Some(r) => h.engine.failed(r),
                None => continue,
            },
            Op::BringUpFailed { which } => match h.request(which) {
                Some(r) => h.engine.bring_up_failed(r),
                None => continue,
            },
            Op::ReleaseFailed => {
                h.engine.release_failed();
                vec![]
            }
            Op::Emergency(on) => h.engine.set_emergency_enabled(on),
            Op::PeerGone(who) => {
                let peer = who.peer();
                // As the daemon does: purge the peer's queued output first.
                let (gone, kept): (Vec<_>, Vec<_>) =
                    h.held.drain(..).partition(|r| r.peer() == peer);
                h.held = kept;
                for r in gone {
                    h.engine.finish_output(r, Disposition::PeerLost);
                }
                h.engine.peer_gone(peer)
            }
            Op::PeerStalled(who) => h.engine.peer_stalled(who.peer()),
            Op::NodeGone(n) => {
                let node = if n % 2 == 0 { MODEM_NODE } else { u32::from(n) };
                let (gone, kept): (Vec<_>, Vec<_>) =
                    h.held.drain(..).partition(|r| r.peer().node == node);
                h.held = kept;
                for r in gone {
                    h.engine.finish_output(r, Disposition::PeerLost);
                }
                h.engine.node_gone(node)
            }
            Op::Shutdown => {
                for r in h.held.drain(..) {
                    h.engine.finish_output(r, Disposition::PeerLost);
                }
                h.engine.shutdown()
            }
            Op::Finish { index, how } => {
                if h.held.is_empty() {
                    continue;
                }
                let r = h.held.remove(usize::from(index) % h.held.len());
                if let Some(bytes) = h.engine.prepare_output(&r) {
                    assert!(bytes.len() <= MAX_DATAGRAM);
                    Frame::parse(&bytes).expect("prepared output parses");
                }
                let how = match how % 3 {
                    0 => Disposition::Submitted,
                    1 => Disposition::Obsolete,
                    _ => Disposition::PeerLost,
                };
                h.engine.finish_output(r, how);
                vec![]
            }
            Op::Calendar { which, fields, seconds } => {
                if h.reads.is_empty() {
                    continue;
                }
                let (peer, txn, pdp, sequence) = h.reads[usize::from(which) % h.reads.len()];
                vec![h.engine.calendar_reply(peer, txn, pdp, sequence, Some((fields, seconds)))]
            }
        };
        h.apply(effects);
    }
    for r in h.held.drain(..) {
        h.engine.finish_output(r, Disposition::PeerLost);
    }
    let effects = h.engine.shutdown();
    h.apply(effects);
    assert_eq!(h.engine.session_count(), 0);
}

fuzz_target!(|input: Input| run(input));
