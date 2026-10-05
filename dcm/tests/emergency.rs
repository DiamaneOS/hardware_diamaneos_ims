// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! Emergency (EIMS) simulations: the modem's request patterns against the engine.
//! Synthetic frames and documentation addresses only. Nothing here opens a
//! socket or Binder, so no test can reach a modem, a network or an emergency
//! service.
use diamaneos_ims_dcm::{
    engine::*,
    outgoing::{Disposition, Retained},
    protocol::*,
};
use std::collections::{BTreeMap, BTreeSet};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

const MODEM: Peer = Peer { node: 3, port: 100 };
const SECOND: Peer = Peer { node: 3, port: 101 };
// Must match the engine: 79 PDP IDs, eight of them kept for emergency.
const NORMAL_CAP: usize = MAX_SESSIONS - 8;

/// One activation request as the modem sends it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Act {
    /// TLV 0x12, one-based; None omits it (the no-subscription fallback).
    slot: Option<u32>,
    emergency: bool,
    family: u32,
    profile: u32,
    cookie: u32,
    instance: Option<u32>,
}
fn eims(slot: u32) -> Act {
    Act {
        slot: Some(slot),
        emergency: true,
        family: 0,
        profile: 0,
        cookie: 9,
        instance: None,
    }
}
fn ims(slot: u32) -> Act {
    Act {
        emergency: false,
        ..eims(slot)
    }
}
fn no_sim() -> Act {
    Act {
        slot: None,
        ..eims(1)
    }
}
impl Act {
    fn v6(self) -> Self {
        Self { family: 1, ..self }
    }
    fn profile(self, profile: u32) -> Self {
        Self { profile, ..self }
    }
    fn cookie(self, cookie: u32) -> Self {
        Self { cookie, ..self }
    }
    fn instance(self, instance: u32) -> Self {
        Self {
            instance: Some(instance),
            ..self
        }
    }
    fn key(&self) -> Key {
        Key {
            slot: match self.slot {
                None | Some(0) => -1,
                Some(slot) => slot as i32 - 1,
            },
            kind: if self.emergency {
                PdnType::Emergency
            } else {
                PdnType::Ims
            },
        }
    }
    /// The engine's notion of "the same context": everything but the cookie.
    fn same_context(&self, other: &Act) -> bool {
        Act { cookie: 0, ..*self }
            == Act {
                cookie: 0,
                ..*other
            }
    }
    fn bytes(&self, txn: u16) -> Vec<u8> {
        let apn: &[u8] = if self.emergency { b"sos" } else { b"ims" };
        let mut value = vec![apn.len() as u8];
        value.extend(apn);
        value.extend((if self.emergency { 2u32 } else { 0 }).to_le_bytes());
        value.extend(0u32.to_le_bytes()); // RAT
        value.extend(self.family.to_le_bytes());
        value.extend(self.profile.to_le_bytes());
        let mut e = Encoder::new(Kind::Request, txn, ACTIVATE);
        e.tlv(1, &value).unwrap();
        e.tlv(0x10, &self.cookie.to_le_bytes()).unwrap();
        if let Some(slot) = self.slot {
            e.tlv(0x12, &slot.to_le_bytes()).unwrap();
        }
        if let Some(instance) = self.instance {
            e.tlv(0x13, &instance.to_le_bytes()).unwrap();
        }
        e.finish()
    }
}
fn deactivate(txn: u16, pdp: u8, instance: Option<u32>) -> Vec<u8> {
    let mut e = Encoder::new(Kind::Request, txn, DEACTIVATE);
    e.tlv(1, &[pdp]).unwrap();
    if let Some(instance) = instance {
        e.tlv(0x10, &instance.to_le_bytes()).unwrap();
    }
    e.finish()
}
fn destroy(txn: u16, instance: u32) -> Vec<u8> {
    let mut e = Encoder::new(Kind::Request, txn, 0x33);
    e.tlv(1, &instance.to_le_bytes()).unwrap();
    e.finish()
}

/// What the modem and the broker see, decoded from the engine's effects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Seen {
    Accepted { to: Peer, txn: u16, pdp: u8 },
    Refused { to: Peer, txn: u16, error: u16 },
    Up { to: Peer, pdp: u8, address: IpAddr },
    Change { to: Peer, pdp: u8, address: IpAddr },
    Ended { to: Peer, pdp: u8 },
    Deactivated { to: Peer, txn: u16 },
    Answered { to: Peer, id: u16, result: u16 },
    BringUp(Request),
    Release(Request),
}
const OK: &[u8] = &[0, 0, 0, 0];
const ENDED: &[u8] = &[0, 0, 13, 0];
fn address(value: &[u8]) -> IpAddr {
    let n = value[4] as usize;
    std::str::from_utf8(&value[5..5 + n])
        .unwrap()
        .parse()
        .unwrap()
}
fn decode(effect: &Effect) -> Seen {
    match effect {
        Effect::Send(to, packet) => {
            let to = *to;
            let f = Frame::parse(packet).unwrap();
            let status = f.tlv(2);
            match (f.kind, f.id) {
                (Kind::Response, ACTIVATE) if status == Some(OK) => Seen::Accepted {
                    to,
                    txn: f.txn,
                    pdp: f.tlv(0x10).unwrap()[0],
                },
                (Kind::Response, DEACTIVATE) if status == Some(OK) => {
                    Seen::Deactivated { to, txn: f.txn }
                }
                (Kind::Response, ACTIVATE | DEACTIVATE) => {
                    let s = status.unwrap();
                    assert_eq!(s[..2], [1, 0]);
                    Seen::Refused {
                        to,
                        txn: f.txn,
                        error: u16::from_le_bytes([s[2], s[3]]),
                    }
                }
                (Kind::Response, id) => {
                    let s = status.unwrap();
                    Seen::Answered {
                        to,
                        id,
                        result: u16::from_le_bytes([s[0], s[1]]),
                    }
                }
                (Kind::Indication, ACTIVATE) => {
                    let pdp = f.tlv(1).unwrap()[0];
                    match f.tlv(0x11) {
                        Some(value) => {
                            assert_eq!(status, Some(OK));
                            Seen::Up {
                                to,
                                pdp,
                                address: address(value),
                            }
                        }
                        None => {
                            // A terminal result never carries a stale address.
                            assert_eq!(status, Some(ENDED));
                            Seen::Ended { to, pdp }
                        }
                    }
                }
                (Kind::Indication, ADDRESS_CHANGE) => Seen::Change {
                    to,
                    pdp: f.tlv(1).unwrap()[0],
                    address: address(f.tlv(0x10).unwrap()),
                },
                _ => panic!("unexpected frame"),
            }
        }
        Effect::BringUp(request) => Seen::BringUp(*request),
        Effect::Release(request) | Effect::ReleaseAfter { request, .. } => Seen::Release(*request),
        Effect::ReadTime { .. } => panic!("unexpected calendar read"),
    }
}
fn seen(effects: Vec<Effect>) -> Vec<Seen> {
    effects.iter().map(decode).collect()
}
fn accepted(out: &[Seen]) -> u8 {
    out.iter()
        .find_map(|s| match s {
            Seen::Accepted { pdp, .. } => Some(*pdp),
            _ => None,
        })
        .unwrap_or_else(|| panic!("not accepted: {out:?}"))
}
fn bring_ups(out: &[Seen]) -> Vec<Request> {
    out.iter()
        .filter_map(|s| match s {
            Seen::BringUp(r) => Some(*r),
            _ => None,
        })
        .collect()
}
fn bring_up(out: &[Seen]) -> Request {
    let all = bring_ups(out);
    assert_eq!(all.len(), 1, "{out:?}");
    all[0]
}
fn released(out: &[Seen]) -> Vec<Request> {
    out.iter()
        .filter_map(|s| match s {
            Seen::Release(r) => Some(*r),
            _ => None,
        })
        .collect()
}
fn refused(error: u16) -> impl Fn(&[Seen]) -> bool {
    move |out| matches!(out, [Seen::Refused { error: e, .. }] if *e == error)
}
fn network(handle: u64, v4: Option<[u8; 4]>, v6: Option<&str>) -> Network {
    Network {
        handle,
        v4: v4.map(Ipv4Addr::from),
        v6: v6.map(|a| a.parse::<Ipv6Addr>().unwrap()),
        mtu: 1500,
    }
}
fn dual(handle: u64) -> Network {
    let last = handle as u8;
    network(
        handle,
        Some([192, 0, 2, last]),
        Some(&format!("2001:db8::{last:x}")),
    )
}
fn v4(n: &Network) -> IpAddr {
    IpAddr::V4(n.v4.unwrap())
}
fn v6(n: &Network) -> IpAddr {
    IpAddr::V6(n.v6.unwrap())
}

struct Sim {
    e: Engine,
    txn: u16,
}
impl Sim {
    fn new() -> Self {
        let mut s = Self::without_broker();
        assert!(s.e.broker_connected().is_empty());
        s
    }
    fn without_broker() -> Self {
        Self {
            e: Engine::new(MODEM.node, 2).unwrap(),
            txn: 0,
        }
    }
    fn next(&mut self) -> u16 {
        self.txn = self.txn.wrapping_add(1);
        self.txn
    }
    fn activate(&mut self, peer: Peer, act: Act) -> Vec<Seen> {
        let txn = self.next();
        seen(self.e.receive(peer, &act.bytes(txn)))
    }
    /// A new context: the modem gets its PDP ID and the broker one request.
    fn open(&mut self, peer: Peer, act: Act) -> (u8, Request) {
        let out = self.activate(peer, act);
        (accepted(&out), bring_up(&out))
    }
    fn deactivate(&mut self, peer: Peer, pdp: u8) -> Vec<Seen> {
        let txn = self.next();
        seen(self.e.receive(peer, &deactivate(txn, pdp, None)))
    }
    fn report(&mut self, request: Request, network: Option<Network>) -> Vec<Seen> {
        seen(self.e.report(request, network))
    }
}

// Kill switch and default service

#[test]
fn emergency_is_served_by_a_fresh_engine_without_any_switch() {
    let mut s = Sim::new();
    let out = s.activate(MODEM, eims(1));
    assert!(matches!(out[0], Seen::Accepted { to: MODEM, .. }));
    assert_eq!(
        bring_up(&out).key,
        Key {
            slot: 0,
            kind: PdnType::Emergency
        }
    );
}

#[test]
fn kill_switch_refuses_new_emergency_only_while_set() {
    let mut s = Sim::new();
    assert!(s.e.set_emergency_enabled(false).is_empty());
    let out = s.activate(MODEM, eims(1));
    assert!(refused(0)(&out), "{out:?}");
    assert_eq!(s.e.session_count(), 0);
    assert_eq!(s.e.diagnostics().activation_requests_by_slot, [0, 1, 0, 0]);
    // Normal IMS and the no-SIM path are refused or served exactly as before.
    let (_, ims_request) = s.open(MODEM, ims(1));
    assert!(refused(0)(&s.activate(MODEM, no_sim())));
    assert!(s.e.set_emergency_enabled(true).is_empty());
    let (_, emergency) = s.open(MODEM, eims(1));
    assert!(emergency.serial > ims_request.serial);
}

#[test]
fn reapplying_the_default_every_loop_pass_never_disturbs_a_pending_or_live_emergency() {
    let mut s = Sim::new();
    let (pending, waiting) = s.open(MODEM, eims(1));
    let (live, request) = s.open(MODEM, eims(2));
    let n = dual(10);
    assert_eq!(
        s.report(request, Some(n.clone())),
        [Seen::Up {
            to: MODEM,
            pdp: live,
            address: v4(&n)
        }]
    );
    // The daemon calls this on every pass; the engine has no timer of its own,
    // so a pending emergency request waits as long as the modem waits.
    for _ in 0..10_000 {
        assert!(s.e.set_emergency_enabled(true).is_empty());
    }
    assert_eq!(s.e.session_count(), 2);
    assert!(s.e.request_is_current(waiting) && s.e.request_is_current(request));
    let late = dual(11);
    assert!(s.report(waiting, Some(late.clone())).contains(&Seen::Up {
        to: MODEM,
        pdp: pending,
        address: v4(&late)
    }));
}

#[test]
fn kill_switch_ends_live_emergency_on_both_sims_and_keeps_ims() {
    let mut s = Sim::new();
    let mut ims_requests = vec![];
    let mut emergency = BTreeMap::new();
    for slot in [1, 2] {
        let (_, r) = s.open(MODEM, ims(slot));
        s.report(r, Some(dual(u64::from(slot))));
        ims_requests.push(r);
        let (pdp, r) = s.open(MODEM, eims(slot));
        s.report(r, Some(dual(10 + u64::from(slot))));
        emergency.insert(pdp, r);
    }
    let out = seen(s.e.set_emergency_enabled(false));
    let ended: BTreeSet<_> = out
        .iter()
        .filter_map(|x| match x {
            Seen::Ended { pdp, .. } => Some(*pdp),
            _ => None,
        })
        .collect();
    assert_eq!(ended, emergency.keys().copied().collect());
    let mut gone = released(&out);
    gone.sort_by_key(|r| r.serial);
    let mut expected: Vec<_> = emergency.values().copied().collect();
    expected.sort_by_key(|r| r.serial);
    assert_eq!(gone, expected);
    assert_eq!(out.len(), 4);
    assert!(s.e.set_emergency_enabled(false).is_empty());
    assert_eq!(s.e.session_count(), 2);
    for r in ims_requests {
        assert!(s.e.request_is_current(r));
    }
}

// Dual SIM, and IMS with emergency on the same SIM

#[test]
fn dual_sim_ims_and_emergency_are_four_independent_requests() {
    let mut s = Sim::new();
    let (ims1, ims1_r) = s.open(MODEM, ims(1));
    let (ims2, ims2_r) = s.open(MODEM, ims(2));
    let (sos1, sos1_r) = s.open(MODEM, eims(1));
    let (sos2, sos2_r) = s.open(MODEM, eims(2));
    let keys: BTreeSet<_> = [ims1_r, ims2_r, sos1_r, sos2_r]
        .iter()
        .map(|r| r.key)
        .collect();
    assert_eq!(keys.len(), 4);
    assert_eq!(
        [ims1, ims2, sos1, sos2]
            .iter()
            .collect::<BTreeSet<_>>()
            .len(),
        4
    );
    // A report for one SIM's emergency request carries no other context.
    let b = dual(22);
    assert_eq!(
        s.report(sos2_r, Some(b.clone())),
        [Seen::Up {
            to: MODEM,
            pdp: sos2,
            address: v4(&b)
        }]
    );
    // A serial from another request cannot answer this one.
    let crossed = Request {
        key: sos1_r.key,
        serial: sos2_r.serial,
    };
    assert!(s.report(crossed, Some(dual(23))).is_empty());
    // Losing SIM 1's IMS network ends only SIM 1's IMS context.
    assert_eq!(
        s.report(ims1_r, None),
        [
            Seen::Ended {
                to: MODEM,
                pdp: ims1
            },
            Seen::Release(ims1_r)
        ]
    );
    let c = dual(31);
    assert_eq!(
        s.report(sos1_r, Some(c.clone())),
        [Seen::Up {
            to: MODEM,
            pdp: sos1,
            address: v4(&c)
        }]
    );
    assert_eq!(s.e.diagnostics().sessions_by_slot, [0, 1, 2, 0]);
    assert!(s.e.request_is_current(ims2_r));
}

#[test]
fn no_sim_emergency_is_separate_from_a_sim_emergency() {
    for encoding in [None, Some(0)] {
        let mut s = Sim::new();
        let fallback = Act {
            slot: encoding,
            ..eims(1)
        };
        let (any, any_r) = s.open(MODEM, fallback);
        assert_eq!(any_r.key.slot, -1);
        let (sim, sim_r) = s.open(MODEM, eims(1));
        assert_ne!(any_r.key, sim_r.key);
        let n = dual(40);
        assert_eq!(
            s.report(any_r, Some(n.clone())),
            [Seen::Up {
                to: MODEM,
                pdp: any,
                address: v4(&n)
            }]
        );
        assert_eq!(
            s.report(any_r, None),
            [
                Seen::Ended {
                    to: MODEM,
                    pdp: any
                },
                Seen::Release(any_r)
            ]
        );
        assert!(s.e.request_is_current(sim_r));
        assert_eq!(s.e.session_count(), 1);
        let n = dual(41);
        assert_eq!(
            s.report(sim_r, Some(n.clone())),
            [Seen::Up {
                to: MODEM,
                pdp: sim,
                address: v4(&n)
            }]
        );
        // Normal IMS never takes the slotless path.
        assert!(refused(0)(&s.activate(
            MODEM,
            Act {
                emergency: false,
                ..fallback
            }
        )));
    }
}

#[test]
fn ims_and_emergency_on_one_sim_never_share_a_network() {
    let mut s = Sim::new();
    let (ims4, ims_r) = s.open(MODEM, ims(1));
    let ims6 = accepted(&s.activate(MODEM, ims(1).v6()));
    let ims_net = dual(50);
    assert_eq!(s.report(ims_r, Some(ims_net.clone())).len(), 2);
    // IMS is up, yet a new emergency context waits for its own network.
    let first = s.activate(MODEM, eims(1));
    let sos4 = accepted(&first);
    let sos_r = bring_up(&first);
    assert_eq!(first.len(), 2);
    assert_eq!(s.activate(MODEM, eims(1).v6()).len(), 1);
    assert_eq!(s.e.session_count(), 4);
    let sos_net = dual(60);
    let up = s.report(sos_r, Some(sos_net.clone()));
    assert_eq!(up.len(), 2);
    for x in &up {
        let Seen::Up { pdp, address, .. } = x else {
            panic!("{up:?}")
        };
        assert!(![ims4, ims6].contains(pdp));
        assert!(*address == v4(&sos_net) || *address == v6(&sos_net));
    }
    // Losing IMS ends both IMS contexts and nothing else.
    let out = s.report(ims_r, None);
    assert_eq!(out.len(), 3);
    assert_eq!(released(&out), [ims_r]);
    assert!(out.contains(&Seen::Ended {
        to: MODEM,
        pdp: ims4
    }));
    assert!(out.contains(&Seen::Ended {
        to: MODEM,
        pdp: ims6
    }));
    assert!(s.e.request_is_current(sos_r));
    // The emergency request is released with its last context only.
    assert!(released(&s.deactivate(MODEM, sos4)).is_empty());
    let sos6 = up
        .iter()
        .find_map(|x| match x {
            Seen::Up { pdp, .. } if *pdp != sos4 => Some(*pdp),
            _ => None,
        })
        .unwrap();
    assert_eq!(released(&s.deactivate(MODEM, sos6)), [sos_r]);
    assert_eq!(s.e.session_count(), 0);
}

// Broker restarts

#[test]
fn emergency_requested_while_the_broker_restarts_is_filed_when_it_returns() {
    let mut s = Sim::new();
    assert!(s.e.broker_lost().is_empty());
    let out = s.activate(MODEM, eims(1));
    assert_eq!(out.len(), 1); // Acknowledged and held; no one to ask yet.
    let pdp = accepted(&out);
    // With no broker registered, no report can bring it up, whatever its serial.
    for serial in 1..=4 {
        let held = Request {
            key: eims(1).key(),
            serial,
        };
        assert!(s.report(held, Some(dual(1))).is_empty());
    }
    let r = bring_up(&seen(s.e.broker_connected()));
    assert_eq!(r.key, eims(1).key());
    // The serial assigned while no broker listened is not this request's.
    for serial in 1..r.serial {
        assert!(s
            .report(Request { key: r.key, serial }, Some(dual(1)))
            .is_empty());
    }
    let n = dual(70);
    assert_eq!(
        s.report(r, Some(n.clone())),
        [Seen::Up {
            to: MODEM,
            pdp,
            address: v4(&n)
        }]
    );
}

#[test]
fn broker_loss_during_emergency_setup_ends_the_attempt_and_the_retry_succeeds() {
    let mut s = Sim::new();
    let (pdp, first) = s.open(MODEM, eims(2));
    assert_eq!(
        s.e.broker_lost().iter().map(decode).collect::<Vec<_>>(),
        [Seen::Ended { to: MODEM, pdp }, Seen::Release(first)]
    );
    assert_eq!(s.e.session_count(), 0);
    // The old broker's late answer cannot revive it, before or after return.
    assert!(s.report(first, Some(dual(1))).is_empty());
    assert!(s.e.broker_connected().is_empty());
    assert!(s.report(first, Some(dual(1))).is_empty());
    let (again, retry) = s.open(MODEM, eims(2));
    assert!(retry.serial > first.serial);
    let n = dual(80);
    assert_eq!(
        s.report(retry, Some(n.clone())),
        [Seen::Up {
            to: MODEM,
            pdp: again,
            address: v4(&n)
        }]
    );
    let d = s.e.diagnostics();
    assert_eq!((d.broker_losses, d.broker_registrations), (1, 2));
    assert_eq!(d.stale_reports, 2);
}

#[test]
fn broker_replacement_while_everything_is_up_refiles_each_request_on_retry() {
    let mut s = Sim::new();
    let mut contexts = vec![];
    for (i, act) in [ims(1), ims(2), eims(1), eims(2)].into_iter().enumerate() {
        let (pdp, r) = s.open(MODEM, act);
        s.report(r, Some(dual(90 + i as u64)));
        contexts.push((act, pdp, r));
    }
    // The daemon handles a new registration as loss of the old broker first.
    let out = seen(s.e.broker_lost());
    assert_eq!(out.len(), 8);
    for (_, pdp, r) in &contexts {
        assert!(out.contains(&Seen::Ended {
            to: MODEM,
            pdp: *pdp
        }));
        assert!(out.contains(&Seen::Release(*r)));
    }
    assert!(s.e.broker_connected().is_empty());
    let newest = contexts.iter().map(|c| c.2.serial).max().unwrap();
    for (i, (act, _, _)) in contexts.iter().enumerate() {
        let (pdp, r) = s.open(MODEM, *act);
        assert!(r.serial > newest);
        let n = dual(100 + i as u64);
        assert_eq!(
            s.report(r, Some(n.clone())),
            [Seen::Up {
                to: MODEM,
                pdp,
                address: v4(&n)
            }]
        );
    }
}

#[test]
fn a_terminal_result_after_broker_loss_carries_no_address() {
    let mut s = Sim::new();
    let (_, r) = s.open(MODEM, eims(1).v6());
    s.report(r, Some(dual(5)));
    for effect in s.e.broker_lost() {
        if let Effect::Send(_, packet) = effect {
            let f = Frame::parse(&packet).unwrap();
            assert_eq!(f.tlv(2), Some(ENDED));
            assert!(f.tlv(0x11).is_none());
        }
    }
}

// Reserve under session exhaustion

fn fill_normal(s: &mut Sim, peers: &[Peer]) -> Vec<(Peer, u8)> {
    (0..NORMAL_CAP)
        .map(|i| {
            let peer = peers[i % peers.len()];
            let act = ims(1 + (i % 2) as u32)
                .v6()
                .profile(i as u32)
                .instance((i / 2) as u32);
            (peer, accepted(&s.activate(peer, act)))
        })
        .collect()
}

#[test]
fn emergency_reserve_admits_eight_more_contexts_after_normal_ims_is_capped() {
    let mut s = Sim::new();
    let normal = fill_normal(&mut s, &[MODEM]);
    assert_eq!(s.e.session_count(), NORMAL_CAP);
    // Normal IMS stops at its cap although eight PDP IDs are still free.
    assert!(refused(5)(&s.activate(MODEM, ims(1).profile(500))));
    assert!(refused(5)(&s.activate(MODEM, ims(2).profile(500))));
    let mut emergency = vec![];
    for i in 0..8u32 {
        let act = eims(1 + i % 2).profile(i / 2).instance(i % 3);
        let act = if i % 4 < 2 { act } else { act.v6() };
        emergency.push(accepted(&s.activate(MODEM, act)));
    }
    assert_eq!(s.e.session_count(), MAX_SESSIONS);
    assert!(refused(5)(&s.activate(MODEM, eims(1).profile(99))));
    assert!(refused(5)(&s.activate(MODEM, no_sim())));
    // A finished emergency context frees its place for the next one.
    s.deactivate(MODEM, emergency[3]);
    assert_eq!(accepted(&s.activate(MODEM, no_sim())), emergency[3]);
    // Normal IMS below its cap is admitted again, never beyond it.
    s.deactivate(MODEM, emergency[0]);
    s.deactivate(MODEM, normal[0].1);
    accepted(&s.activate(MODEM, ims(1).profile(500)));
    assert!(refused(5)(&s.activate(MODEM, ims(1).profile(501))));
    accepted(&s.activate(MODEM, eims(2).profile(77)));
    assert_eq!(s.e.session_count(), MAX_SESSIONS);
}

#[test]
fn emergency_may_exceed_the_reserve_when_normal_use_is_low() {
    let mut s = Sim::new();
    for i in 0..20u32 {
        accepted(&s.activate(MODEM, eims(1 + i % 2).profile(i)));
    }
    assert_eq!(s.e.session_count(), 20);
}

#[test]
fn capped_normal_use_on_both_sims_and_four_clients_still_admits_emergency_on_either_sim() {
    let peers: Vec<_> = (0..4)
        .map(|i| Peer {
            node: 3,
            port: 100 + i,
        })
        .collect();
    let mut s = Sim::new();
    fill_normal(&mut s, &peers);
    for peer in &peers {
        assert!(refused(5)(&s.activate(*peer, ims(2).profile(900))));
    }
    let first = s.activate(peers[0], eims(1));
    let second = s.activate(peers[3], eims(2));
    assert_eq!(bring_up(&first).key, eims(1).key());
    assert_eq!(bring_up(&second).key, eims(2).key());
    assert_eq!(s.e.session_count(), NORMAL_CAP + 2);
}

#[test]
fn a_fifth_modem_client_is_refused_even_for_emergency_until_a_client_leaves() {
    // Documents the independent four-client bound: a fifth QRTR client gets no
    // emergency context. The modem's real client count is a lab check.
    let mut s = Sim::new();
    for i in 0..4 {
        accepted(&s.activate(
            Peer {
                node: 3,
                port: 100 + i,
            },
            ims(1),
        ));
    }
    let fifth = Peer { node: 3, port: 200 };
    let out = s.activate(fifth, eims(1));
    assert!(refused(5)(&out), "{out:?}");
    assert!(s.e.peer_gone(Peer { node: 3, port: 100 }).is_empty()); // IMS still shared.
    let out = s.activate(fifth, eims(1));
    assert_eq!(bring_up(&out).key, eims(1).key());
}

#[test]
fn quarantined_ids_delay_emergency_only_until_their_output_is_finished() {
    let mut s = Sim::new();
    fill_normal(&mut s, &[MODEM]);
    let mut retained = vec![];
    let mut ids = vec![];
    for i in 0..8u32 {
        let txn = s.next();
        let effects = s.e.receive(MODEM, &eims(1).profile(i).bytes(txn));
        ids.push(accepted(&seen_ref(&effects)));
        retain_all(&mut s.e, effects, &mut retained);
    }
    for id in ids {
        let txn = s.next();
        let effects = s.e.receive(MODEM, &deactivate(txn, id, None));
        retain_all(&mut s.e, effects, &mut retained);
    }
    assert_eq!(s.e.session_count(), NORMAL_CAP);
    // All eight free IDs still have output in flight: refuse, do not reuse.
    assert!(refused(5)(&s.activate(MODEM, eims(2))));
    for output in retained {
        s.e.finish_output(output, Disposition::Submitted);
    }
    assert_eq!(bring_up(&s.activate(MODEM, eims(2))).key, eims(2).key());
}
fn seen_ref(effects: &[Effect]) -> Vec<Seen> {
    effects.iter().map(decode).collect()
}
fn retain_all(engine: &mut Engine, effects: Vec<Effect>, into: &mut Vec<Retained>) {
    for effect in effects {
        if let Effect::Send(peer, packet) = effect {
            into.push(engine.retain_output(peer, packet).unwrap());
        }
    }
}

// Teardown and re-activation

#[test]
fn emergency_deactivation_then_reactivation_files_a_new_request_each_time() {
    let mut s = Sim::new();
    let mut last = 0;
    for cycle in 0..50u32 {
        let act = eims(1 + cycle % 2).instance(cycle % 3);
        let (pdp, r) = s.open(MODEM, act);
        assert!(r.serial > last);
        last = r.serial;
        let n = dual(u64::from(cycle) + 1);
        // Always an initial result, never an address change from a past cycle.
        assert_eq!(
            s.report(r, Some(n.clone())),
            [Seen::Up {
                to: MODEM,
                pdp,
                address: v4(&n)
            }]
        );
        let txn = s.next();
        let out = seen(s.e.receive(MODEM, &deactivate(txn, pdp, act.instance)));
        // Acknowledge, release the broker request, then the terminal result.
        assert_eq!(
            out,
            [
                Seen::Deactivated { to: MODEM, txn },
                Seen::Release(r),
                Seen::Ended { to: MODEM, pdp }
            ]
        );
        assert!(s.report(r, Some(n)).is_empty());
    }
    let d = s.e.diagnostics();
    assert_eq!((d.active_sessions, d.active_groups), (0, 0));
    assert_eq!(d.modem_releases_by_slot, [0, 25, 25, 0]);
}

#[test]
fn emergency_network_loss_ends_the_context_and_the_retry_refiles() {
    let mut s = Sim::new();
    let (pdp, r) = s.open(MODEM, eims(1));
    s.report(r, Some(dual(3)));
    assert_eq!(
        s.report(r, None),
        [Seen::Ended { to: MODEM, pdp }, Seen::Release(r)]
    );
    let (_, again) = s.open(MODEM, eims(1));
    assert!(again.serial > r.serial);
    // An invalid network is a loss, never an "up" with a bad address.
    let mut bad = dual(4);
    bad.v4 = Some(Ipv4Addr::LOCALHOST);
    assert!(matches!(
        s.report(again, Some(bad))[..],
        [Seen::Ended { .. }, Seen::Release(_)]
    ));
}

#[test]
fn an_emergency_bearer_moving_network_sends_an_address_change() {
    let mut s = Sim::new();
    let (pdp, r) = s.open(MODEM, eims(1).v6());
    let a = dual(1);
    s.report(r, Some(a.clone()));
    assert!(s.report(r, Some(a.clone())).is_empty());
    let b = dual(2);
    assert_eq!(
        s.report(r, Some(b.clone())),
        [Seen::Change {
            to: MODEM,
            pdp,
            address: v6(&b)
        }]
    );
    // Same address on a replacement network is still announced.
    let mut c = b.clone();
    c.handle = 77;
    assert_eq!(
        s.report(r, Some(c)),
        [Seen::Change {
            to: MODEM,
            pdp,
            address: v6(&b)
        }]
    );
}

#[test]
fn dual_family_emergency_on_a_single_family_network_never_reports_false_success() {
    let mut s = Sim::new();
    let (sos4, r) = s.open(MODEM, eims(1));
    let out = s.activate(MODEM, eims(1).v6());
    assert_eq!(out.len(), 1); // Shares the request.
    let sos6 = accepted(&out);
    let only4 = network(8, Some([198, 51, 100, 8]), None);
    let out = s.report(r, Some(only4.clone()));
    assert_eq!(
        out,
        [
            Seen::Up {
                to: MODEM,
                pdp: sos4,
                address: v4(&only4)
            },
            Seen::Ended {
                to: MODEM,
                pdp: sos6
            }
        ]
    );
    // The modem's IPv6 retry is ended at once from the known network.
    let retry = s.activate(MODEM, eims(1).v6());
    assert!(matches!(
        retry[..],
        [Seen::Accepted { .. }, Seen::Ended { .. }]
    ));
    assert!(s.e.request_is_current(r));
    assert_eq!(
        s.e.diagnostics().missing_family_releases_by_slot,
        [0, 2, 0, 0]
    );
}

#[test]
fn repeated_emergency_activation_answers_with_the_current_state() {
    let mut s = Sim::new();
    let (pdp, r) = s.open(MODEM, eims(2));
    // Before the network is up: same context, no second request.
    let out = s.activate(MODEM, eims(2).cookie(10));
    assert_eq!(
        out,
        [Seen::Accepted {
            to: MODEM,
            txn: s.txn,
            pdp
        }]
    );
    let n = dual(6);
    s.report(r, Some(n.clone()));
    let txn = s.next();
    let effects = s.e.receive(MODEM, &eims(2).cookie(11).bytes(txn));
    assert_eq!(
        seen_ref(&effects),
        [
            Seen::Accepted {
                to: MODEM,
                txn,
                pdp
            },
            Seen::Up {
                to: MODEM,
                pdp,
                address: v4(&n)
            }
        ]
    );
    let Effect::Send(_, up) = &effects[1] else {
        panic!()
    };
    let f = Frame::parse(up).unwrap();
    assert_eq!(f.id, ACTIVATE); // A fresh initial result, not an address change.
    assert_eq!(f.tlv(0x10), Some(11u32.to_le_bytes().as_slice()));
    assert_eq!(s.e.session_count(), 1);
}

#[test]
fn modem_restart_during_emergency_releases_it_and_the_new_client_is_served() {
    let mut s = Sim::new();
    let (_, r) = s.open(MODEM, eims(1));
    s.report(r, Some(dual(1)));
    assert_eq!(seen(s.e.node_gone(MODEM.node)), [Seen::Release(r)]);
    assert_eq!(s.e.diagnostics().modem_losses, 1);
    let restarted = Peer { node: 3, port: 300 };
    let (pdp, again) = s.open(restarted, eims(1));
    assert!(again.serial > r.serial);
    let n = dual(2);
    assert_eq!(
        s.report(again, Some(n.clone())),
        [Seen::Up {
            to: restarted,
            pdp,
            address: v4(&n)
        }]
    );
    // A client that leaves releases what only it held.
    assert_eq!(seen(s.e.peer_gone(restarted)), [Seen::Release(again)]);
}

#[test]
fn instance_destruction_ends_only_that_clients_emergency() {
    let mut s = Sim::new();
    let (_, r) = s.open(MODEM, eims(1).instance(7));
    let other = s.activate(SECOND, eims(1).instance(7));
    assert_eq!(other.len(), 1); // Same SIM and type: one shared request.
    let txn = s.next();
    assert_eq!(
        seen(s.e.receive(MODEM, &destroy(txn, 7))),
        [Seen::Answered {
            to: MODEM,
            id: 0x33,
            result: 0
        }]
    );
    assert_eq!(s.e.session_count(), 1);
    assert!(s.e.request_is_current(r));
    let n = dual(9);
    assert_eq!(
        s.report(r, Some(n.clone())),
        [Seen::Up {
            to: SECOND,
            pdp: accepted(&other),
            address: v4(&n)
        }]
    );
    let txn = s.next();
    assert!(released(&seen(s.e.receive(SECOND, &destroy(txn, 7)))).contains(&r));
}

// Randomised model check: two SIMs and the no-SIM path, IMS and emergency,
// both IP families, two modem clients, broker restarts, kill switch, network
// changes and client or modem loss. Each step is checked against a shadow model.

#[derive(Clone, Copy)]
enum Op {
    Activate,
    Repeat,
    Deactivate,
    Destroy,
    Up,
    Down,
    Stale,
    BrokerLost,
    BrokerBack,
    Kill,
    PeerGone,
    ModemGone,
    Idle,
}
/// Weights per mille. Frequent loss and restart, single-family and invalid
/// networks: lifecycle, fail-closed and stale-callback coverage.
const CHURN: &[(u64, Op)] = &[
    (300, Op::Activate),
    (80, Op::Repeat),
    (120, Op::Deactivate),
    (20, Op::Destroy),
    (180, Op::Up),
    (60, Op::Down),
    (40, Op::Stale),
    (20, Op::BrokerLost),
    (70, Op::BrokerBack),
    (40, Op::Kill),
    (30, Op::PeerGone),
    (10, Op::ModemGone),
    (30, Op::Idle),
];
/// Mostly new contexts on dual-stack networks: the normal cap and the
/// emergency reserve are reached, with rare losses in between.
const PRESSURE: &[(u64, Op)] = &[
    (700, Op::Activate),
    (40, Op::Repeat),
    (60, Op::Deactivate),
    (90, Op::Up),
    (5, Op::Down),
    (30, Op::Stale),
    (2, Op::BrokerLost),
    (30, Op::BrokerBack),
    (8, Op::Kill),
    (2, Op::PeerGone),
    (33, Op::Idle),
];

/// Counts that prove the walk reached the interesting states.
#[derive(Default)]
struct Coverage {
    emergency_up: u64,
    emergency_at_normal_cap: u64,
    capacity_refusals: u64,
    kill_refusals: u64,
    stale_reports: u64,
    refiled_after_broker_return: u64,
}

struct Model {
    s: Sim,
    rng: u64,
    ops: &'static [(u64, Op)],
    pending: BTreeMap<u16, Act>,
    live: BTreeMap<(Peer, u8), Act>,
    held: BTreeMap<Key, Request>,
    networks: BTreeMap<Key, Network>,
    history: Vec<Request>,
    newest: i32,
    broker: bool,
    emergency: bool,
    coverage: Coverage,
}
impl Model {
    fn new(seed: u64, ops: &'static [(u64, Op)]) -> Self {
        assert_eq!(ops.iter().map(|o| o.0).sum::<u64>(), 1000);
        Self {
            s: Sim::new(),
            rng: seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1,
            ops,
            pending: BTreeMap::new(),
            live: BTreeMap::new(),
            held: BTreeMap::new(),
            networks: BTreeMap::new(),
            history: vec![],
            newest: 0,
            broker: true,
            emergency: true,
            coverage: Coverage::default(),
        }
    }
    fn next(&mut self, n: u64) -> u64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        self.rng % n
    }
    fn peer(&mut self) -> Peer {
        [MODEM, SECOND][self.next(2) as usize]
    }
    fn act(&mut self) -> Act {
        let emergency = self.next(2) == 0;
        let slot = if emergency {
            [None, Some(1), Some(2)][self.next(3) as usize]
        } else {
            Some(1 + self.next(2) as u32)
        };
        Act {
            slot,
            emergency,
            family: self.next(2) as u32,
            profile: self.next(48) as u32,
            cookie: self.next(1000) as u32,
            instance: [None, Some(1)][self.next(2) as usize],
        }
    }
    fn network(&mut self) -> Network {
        let handle = 1 + self.next(6);
        let last = 1 + self.next(4) as u8;
        let variant = if std::ptr::eq(self.ops, PRESSURE) {
            3
        } else {
            self.next(6)
        };
        match variant {
            0 => network(handle, Some([192, 0, 2, last]), None),
            1 => network(handle, None, Some(&format!("2001:db8::{last}"))),
            2 => network(handle, Some([127, 0, 0, 1]), None), // Invalid: a loss.
            _ => network(
                handle,
                Some([198, 51, 100, last]),
                Some(&format!("2001:db8:1::{last}")),
            ),
        }
    }
    fn keys(&self) -> BTreeSet<Key> {
        self.live.values().map(Act::key).collect()
    }
    fn expected_address(&self, act: &Act) -> Option<IpAddr> {
        let n = self.networks.get(&act.key())?;
        if act.family == 0 {
            n.v4.map(IpAddr::V4)
        } else {
            n.v6.map(IpAddr::V6)
        }
    }
    fn apply(&mut self, effects: Vec<Effect>, activation: Option<(Peer, u16, Act)>) {
        let before = self.keys();
        self.apply_from(before, effects, activation);
    }
    /// Checks one engine transition and updates the shadow model. `before` is
    /// the set of requested SIM/type keys before the transition, taken ahead of
    /// any context the engine removes without a modem message.
    fn apply_from(
        &mut self,
        before: BTreeSet<Key>,
        effects: Vec<Effect>,
        activation: Option<(Peer, u16, Act)>,
    ) {
        let total = self.live.len();
        let normal = self.live.values().filter(|a| !a.emergency).count();
        let out = seen(effects);
        if let Some((peer, txn, act)) = activation {
            let repeat = self
                .live
                .iter()
                .find(|((p, _), a)| *p == peer && a.same_context(&act))
                .map(|((_, pdp), _)| *pdp);
            let expect_refusal = if repeat.is_some() {
                None
            } else if act.emergency && !self.emergency {
                Some(0)
            } else if total >= MAX_SESSIONS || (!act.emergency && normal >= NORMAL_CAP) {
                Some(5)
            } else {
                None
            };
            match expect_refusal {
                Some(error) => {
                    assert!(
                        matches!(out[..], [Seen::Refused { to, txn: t, error: e }]
                            if to == peer && t == txn && e == error),
                        "{act:?} {out:?}"
                    );
                    if error == 5 {
                        self.coverage.capacity_refusals += 1;
                    } else {
                        self.coverage.kill_refusals += 1;
                    }
                }
                None => {
                    let Seen::Accepted { to, txn: t, pdp } = out[0] else {
                        panic!("{act:?} {out:?}")
                    };
                    assert_eq!((to, t), (peer, txn));
                    if let Some(old) = repeat {
                        assert_eq!(pdp, old);
                    } else if act.emergency && normal >= NORMAL_CAP {
                        self.coverage.emergency_at_normal_cap += 1;
                    }
                    let files = repeat.is_none() && !before.contains(&act.key()) && self.broker;
                    assert_eq!(bring_ups(&out).len(), usize::from(files), "{out:?}");
                }
            }
        }
        for x in &out {
            match *x {
                Seen::Accepted { to, txn, pdp } => {
                    let act = self.pending[&txn];
                    assert!(!act.emergency || self.emergency);
                    self.live.insert((to, pdp), act);
                }
                Seen::Up { to, pdp, address } | Seen::Change { to, pdp, address } => {
                    // No false "up": a live context, a connected broker, the
                    // current network of exactly this SIM and type, the right family.
                    let act = self.live[&(to, pdp)];
                    assert!(self.broker);
                    assert!(!act.emergency || self.emergency);
                    assert_eq!(self.expected_address(&act), Some(address), "{act:?}");
                    if act.emergency {
                        self.coverage.emergency_up += 1;
                    }
                }
                Seen::Ended { to, pdp } => {
                    assert!(self.live.remove(&(to, pdp)).is_some(), "{out:?}");
                }
                Seen::BringUp(r) => {
                    assert!(self.broker);
                    assert!(r.serial > self.newest);
                    self.newest = r.serial;
                    assert!(self.live.values().any(|a| a.key() == r.key));
                    self.held.insert(r.key, r);
                    self.networks.remove(&r.key);
                    self.history.push(r);
                }
                Seen::Release(r) => {
                    if self.held.get(&r.key) == Some(&r) {
                        self.held.remove(&r.key);
                        self.networks.remove(&r.key);
                    }
                }
                Seen::Refused { .. } | Seen::Deactivated { .. } | Seen::Answered { .. } => {}
            }
        }
        let after = self.keys();
        let mut gone: Vec<_> = released(&out).iter().map(|r| r.key).collect();
        gone.sort();
        assert_eq!(
            gone,
            before.difference(&after).copied().collect::<Vec<_>>(),
            "release exactly the requests whose last context ended: {out:?}"
        );
        assert_eq!(self.s.e.session_count(), self.live.len());
        let d = self.s.e.diagnostics();
        assert_eq!(d.active_groups, after.len());
        assert!(self.live.values().filter(|a| !a.emergency).count() <= NORMAL_CAP);
        assert!(self.emergency || self.live.values().all(|a| !a.emergency));
        if self.broker {
            // No context waits on a request nobody filed.
            for key in &after {
                assert!(self.s.e.request_is_current(self.held[key]), "{key:?}");
            }
        } else {
            assert!(self.held.is_empty());
        }
    }
    fn op(&mut self) -> Op {
        let mut roll = self.next(1000);
        for &(weight, op) in self.ops {
            if roll < weight {
                return op;
            }
            roll -= weight;
        }
        unreachable!()
    }
    fn step(&mut self) {
        match self.op() {
            Op::Activate => {
                let (peer, act) = (self.peer(), self.act());
                self.activate(peer, act);
            }
            Op::Repeat => {
                if let Some((peer, _, act)) = self.pick_live() {
                    let cookie = self.next(1000) as u32;
                    self.activate(peer, act.cookie(cookie));
                }
            }
            Op::Deactivate => {
                let txn = self.s.next();
                let choice = self.pick_live();
                let roll = self.next(20);
                let (peer, pdp, instance) = match choice {
                    Some((peer, pdp, act)) if roll != 0 => (peer, pdp, act.instance),
                    // An unknown ID, or another client's: refused, changes nothing.
                    _ => (self.peer(), 20 + self.next(79) as u8, None),
                };
                let owned = self.live.contains_key(&(peer, pdp));
                let out = self.s.e.receive(peer, &deactivate(txn, pdp, instance));
                if !owned {
                    assert!(matches!(seen_ref(&out)[..], [Seen::Refused { .. }]));
                }
                self.apply(out, None);
            }
            Op::Destroy => {
                let (peer, instance) = (self.peer(), self.next(2) as u32);
                let before = self.keys();
                self.live
                    .retain(|(p, _), a| !(*p == peer && a.instance.unwrap_or(0) == instance));
                let txn = self.s.next();
                let out = self.s.e.receive(peer, &destroy(txn, instance));
                self.apply_from(before, out, None);
            }
            Op::Up => {
                if let Some(r) = self.pick_held() {
                    let n = self.network();
                    if n.valid() {
                        self.networks.insert(r.key, n.clone());
                    } else {
                        self.networks.remove(&r.key);
                    }
                    let out = self.s.e.report(r, Some(n));
                    self.apply(out, None);
                }
            }
            Op::Down => {
                if let Some(r) = self.pick_held() {
                    self.networks.remove(&r.key);
                    let out = self.s.e.report(r, None);
                    self.apply(out, None);
                }
            }
            Op::Stale => {
                if !self.history.is_empty() {
                    let i = self.next(self.history.len() as u64) as usize;
                    let r = self.history[i];
                    if self.held.get(&r.key) != Some(&r) {
                        let n = self.network();
                        assert!(self.s.e.report(r, Some(n)).is_empty());
                        self.coverage.stale_reports += 1;
                    }
                }
            }
            Op::BrokerLost => {
                if self.broker {
                    self.broker = false;
                    self.held.clear();
                    self.networks.clear();
                    let out = self.s.e.broker_lost();
                    self.apply(out, None);
                }
            }
            Op::BrokerBack => {
                if !self.broker {
                    self.broker = true;
                    let out = self.s.e.broker_connected();
                    let count = self.keys().len();
                    assert_eq!(bring_ups(&seen_ref(&out)).len(), count);
                    self.coverage.refiled_after_broker_return += count as u64;
                    self.apply(out, None);
                }
            }
            Op::Kill => {
                // Mostly on, as on a phone: off about a fifth of the time.
                if self.emergency && self.next(4) != 0 {
                    return;
                }
                self.emergency = !self.emergency;
                let out = self.s.e.set_emergency_enabled(self.emergency);
                self.apply(out, None);
                assert!(self.s.e.set_emergency_enabled(self.emergency).is_empty());
            }
            Op::PeerGone => {
                let peer = self.peer();
                let before = self.keys();
                self.live.retain(|(p, _), _| *p != peer);
                let out = self.s.e.peer_gone(peer);
                self.apply_from(before, out, None);
            }
            Op::ModemGone => {
                let before = self.keys();
                self.live.clear();
                let out = self.s.e.node_gone(MODEM.node);
                self.apply_from(before, out, None);
            }
            Op::Idle => {
                // A daemon loop pass with nothing new: no effect, no timeout.
                assert!(self.s.e.set_emergency_enabled(self.emergency).is_empty());
            }
        }
    }
    fn activate(&mut self, peer: Peer, act: Act) {
        let txn = self.s.next();
        self.pending.insert(txn, act);
        let out = self.s.e.receive(peer, &act.bytes(txn));
        self.apply(out, Some((peer, txn, act)));
    }
    fn pick_live(&mut self) -> Option<(Peer, u8, Act)> {
        if self.live.is_empty() {
            return None;
        }
        let i = self.next(self.live.len() as u64) as usize;
        self.live
            .iter()
            .nth(i)
            .map(|(&(peer, pdp), &act)| (peer, pdp, act))
    }
    fn pick_held(&mut self) -> Option<Request> {
        if self.held.is_empty() {
            return None;
        }
        let i = self.next(self.held.len() as u64) as usize;
        self.held.values().nth(i).copied()
    }
}

fn walk(ops: &'static [(u64, Op)], seeds: std::ops::RangeInclusive<u64>) -> Coverage {
    let mut total = Coverage::default();
    for seed in seeds {
        let mut m = Model::new(seed, ops);
        for _ in 0..4_000 {
            m.step();
        }
        // Shutdown releases every request still held.
        let keys = m.keys();
        let out = seen(m.s.e.shutdown());
        let mut gone: Vec<_> = released(&out).iter().map(|r| r.key).collect();
        gone.sort();
        assert_eq!(gone, keys.into_iter().collect::<Vec<_>>());
        assert_eq!(m.s.e.session_count(), 0);
        let c = m.coverage;
        total.emergency_up += c.emergency_up;
        total.emergency_at_normal_cap += c.emergency_at_normal_cap;
        total.capacity_refusals += c.capacity_refusals;
        total.kill_refusals += c.kill_refusals;
        total.stale_reports += c.stale_reports;
        total.refiled_after_broker_return += c.refiled_after_broker_return;
    }
    total
}

#[test]
fn randomised_emergency_lifecycle_model_check() {
    let c = walk(CHURN, 1..=24);
    // Fixed seeds: these floors only stop the walk silently losing its reach.
    assert!(c.emergency_up > 2_000, "{}", c.emergency_up);
    assert!(c.kill_refusals > 1_000, "{}", c.kill_refusals);
    assert!(c.stale_reports > 1_500, "{}", c.stale_reports);
    assert!(
        c.refiled_after_broker_return > 500,
        "{}",
        c.refiled_after_broker_return
    );
}

#[test]
fn randomised_emergency_capacity_model_check() {
    let c = walk(PRESSURE, 101..=112);
    assert!(c.capacity_refusals > 4_000, "{}", c.capacity_refusals);
    assert!(
        c.emergency_at_normal_cap > 30,
        "{}",
        c.emergency_at_normal_cap
    );
    assert!(c.emergency_up > 8_000, "{}", c.emergency_up);
}
