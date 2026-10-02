// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
use diamaneos_ims_dcm::{engine::*, protocol::*};
use std::net::{Ipv4Addr, Ipv6Addr};
const MODEM: Peer = Peer { node: 3, port: 100 };
fn activate(txn: u16, slot: u32, family: u32, emergency: bool, profile: u32) -> Vec<u8> {
    let apn = if emergency {
        b"sos".as_slice()
    } else {
        b"ims".as_slice()
    };
    let mut v = vec![apn.len() as u8];
    v.extend(apn);
    v.extend((if emergency { 2u32 } else { 0 }).to_le_bytes());
    v.extend(0u32.to_le_bytes());
    v.extend(family.to_le_bytes());
    v.extend(profile.to_le_bytes());
    let mut e = Encoder::new(Kind::Request, txn, ACTIVATE);
    e.tlv(1, &v).unwrap();
    e.tlv(0x10, &9u32.to_le_bytes()).unwrap();
    e.tlv(0x12, &slot.to_le_bytes()).unwrap();
    e.finish()
}
fn deactivate(id: u8) -> Vec<u8> {
    let mut e = Encoder::new(Kind::Request, 55, DEACTIVATE);
    e.tlv(1, &[id]).unwrap();
    e.finish()
}
fn network() -> Network {
    Network {
        handle: 42,
        v4: Some(Ipv4Addr::new(192, 0, 2, 1)),
        v6: Some("2001:db8::1".parse::<Ipv6Addr>().unwrap()),
        mtu: 1500,
    }
}
fn request(effects: &[Effect]) -> Request {
    effects
        .iter()
        .find_map(|e| {
            if let Effect::BringUp(r) = e {
                Some(*r)
            } else {
                None
            }
        })
        .unwrap()
}
fn pdp(effects: &[Effect]) -> u8 {
    effects
        .iter()
        .find_map(|e| {
            if let Effect::Send(_, b) = e {
                Frame::parse(b).ok()?.tlv(0x10)?.first().copied()
            } else {
                None
            }
        })
        .unwrap()
}
fn connected() -> Engine {
    let mut e = Engine::new(3, 2).unwrap();
    e.broker_connected();
    e
}
#[test]
fn modem_node_zero_is_valid_but_local_node_is_not_a_modem_peer() {
    let mut e = Engine::new(0, 2).unwrap();
    e.broker_connected();
    let packet = activate(1, 1, 0, true, 0);
    assert!(e.receive(Peer { node: 1, port: 100 }, &packet).is_empty());
    assert_eq!(e.session_count(), 0);
    let peer = Peer { node: 0, port: 100 };
    let effects = e.receive(peer, &packet);
    assert_eq!(request(&effects).key.kind, PdnType::Emergency);
    assert_eq!(e.session_count(), 1);
    assert!(effects
        .iter()
        .any(|effect| matches!(effect, Effect::Send(to, _) if *to == peer)));
}
#[test]
fn golden_activate_response() {
    let mut e = connected();
    let out = e.receive(MODEM, &activate(0x1234, 1, 0, false, 0));
    let Effect::Send(_, b) = &out[0] else {
        panic!()
    };
    assert_eq!(
        b,
        &[
            2, 0x34, 0x12, 0x20, 0, 18, 0, 2, 4, 0, 0, 0, 0, 0, 0x10, 1, 0, 20, 0x11, 4, 0, 9, 0,
            0, 0
        ]
    );
    assert_eq!(
        request(&out).key,
        Key {
            slot: 0,
            kind: PdnType::Ims
        }
    );
}
#[test]
fn response_precedes_broker_and_up_requires_network() {
    let mut e = connected();
    let out = e.receive(MODEM, &activate(1, 1, 0, true, 0));
    assert_eq!(out.len(), 2);
    let r = request(&out);
    let up = e.report(r, Some(network()));
    assert_eq!(up.len(), 1);
    let Effect::Send(_, b) = &up[0] else { panic!() };
    let f = Frame::parse(b).unwrap();
    assert_eq!(f.kind, Kind::Indication);
    assert_eq!(f.tlv(2), Some([0, 0, 0, 0].as_slice()));
    assert!(f.tlv(0x11).is_some());
}
#[test]
fn dual_family_shares_network_and_releases_last_reference() {
    let mut e = connected();
    let a = e.receive(MODEM, &activate(1, 1, 0, false, 0));
    let b = e.receive(MODEM, &activate(2, 1, 1, false, 0));
    assert_eq!(b.len(), 1);
    assert_eq!(e.report(request(&a), Some(network())).len(), 2);
    assert!(!e
        .receive(MODEM, &deactivate(pdp(&a)))
        .iter()
        .any(|e| matches!(e, Effect::Release(_))));
    assert!(e
        .receive(MODEM, &deactivate(pdp(&b)))
        .iter()
        .any(|e| matches!(e, Effect::Release(_))));
}
#[test]
fn missing_requested_family_never_reports_success() {
    let mut e = connected();
    let a = e.receive(MODEM, &activate(1, 1, 1, true, 0));
    let mut n = network();
    n.v6 = None;
    let out = e.report(request(&a), Some(n));
    assert!(out.iter().any(|x|matches!(x,Effect::Send(_,b) if Frame::parse(b).unwrap().tlv(2)==Some([0,0,13,0].as_slice()))));
    assert_eq!(e.session_count(), 0);
    let diagnostics = e.diagnostics();
    assert_eq!(diagnostics.missing_family_releases_by_slot, [0, 1, 0, 0]);
    assert_eq!(diagnostics.modem_releases_by_slot, [0; 4]);
    assert_eq!(diagnostics.down_reports, 0);
}
#[test]
fn stale_callback_cannot_revive_released_session() {
    let mut e = connected();
    let a = e.receive(MODEM, &activate(1, 1, 0, false, 0));
    e.receive(MODEM, &deactivate(pdp(&a)));
    let b = e.receive(MODEM, &activate(2, 1, 0, false, 0));
    assert_ne!(request(&a).serial, request(&b).serial);
    assert!(e.report(request(&a), Some(network())).is_empty());
    let diagnostics = e.diagnostics();
    assert_eq!(diagnostics.modem_releases_by_slot, [0, 1, 0, 0]);
    assert_eq!(diagnostics.activation_requests_by_slot, [0, 2, 0, 0]);
    assert_eq!(diagnostics.missing_family_releases_by_slot, [0; 4]);
}
#[test]
fn dual_sim_separate_networks() {
    let mut e = connected();
    let a = e.receive(MODEM, &activate(1, 1, 0, false, 0));
    let b = e.receive(MODEM, &activate(2, 2, 0, false, 0));
    assert_ne!(request(&a).key, request(&b).key);
    assert_eq!(e.report(request(&a), Some(network())).len(), 1);
    assert_eq!(e.diagnostics().activation_requests_by_slot, [0, 1, 1, 0]);
}
#[test]
fn duplicate_activation_does_not_leak_or_refile() {
    let mut e = connected();
    let a = e.receive(MODEM, &activate(1, 1, 0, false, 0));
    let b = e.receive(MODEM, &activate(1, 1, 0, false, 0));
    assert_eq!(pdp(&a), pdp(&b));
    assert_eq!(b.len(), 1);
    assert_eq!(e.session_count(), 1);
    assert_eq!(e.diagnostics().activation_requests_by_slot, [0, 2, 0, 0]);
}
#[test]
fn modem_reset_releases_every_network() {
    let mut e = connected();
    e.receive(MODEM, &activate(1, 1, 0, false, 0));
    e.receive(MODEM, &activate(2, 2, 0, true, 0));
    assert_eq!(e.node_gone(3).len(), 2);
    assert_eq!(e.session_count(), 0);
}
#[test]
fn unknown_peer_cannot_allocate_or_release() {
    let mut e = connected();
    let local = Peer { node: 1, port: 100 };
    assert!(e.receive(local, &activate(1, 1, 0, false, 0)).is_empty());
    assert_eq!(e.session_count(), 0);
    let a = e.receive(MODEM, &activate(1, 1, 0, false, 0));
    e.receive(local, &deactivate(pdp(&a)));
    assert_eq!(e.session_count(), 1);
    let diagnostics = e.diagnostics();
    assert_eq!(diagnostics.activation_requests_by_slot, [0, 1, 0, 0]);
    assert_eq!(diagnostics.modem_releases_by_slot, [0; 4]);
}
#[test]
fn reserve_for_emergency_after_normal_exhaustion() {
    let mut e = connected();
    for i in 0..90 {
        e.receive(MODEM, &activate(i as u16, 1, 0, false, i));
    }
    assert_eq!(e.session_count(), 71);
    let a = e.receive(MODEM, &activate(99, 2, 0, true, 0));
    assert!(a.iter().any(|e| matches!(e, Effect::BringUp(_))));
    assert_eq!(e.session_count(), 72);
}
#[test]
fn emergency_kill_does_not_drop_normal_ims() {
    let mut e = connected();
    e.receive(MODEM, &activate(1, 1, 0, false, 0));
    e.receive(MODEM, &activate(2, 1, 0, true, 0));
    let out = e.set_emergency_enabled(false);
    assert_eq!(e.session_count(), 1);
    assert!(out
        .iter()
        .any(|e| matches!(e,Effect::Release(r)if r.key.kind==PdnType::Emergency)));
}
#[test]
fn absent_broker_waits_without_an_ap_timeout() {
    let mut e = Engine::new(3, 2).unwrap();
    assert_eq!(e.receive(MODEM, &activate(1, 1, 0, true, 0)).len(), 1);
    assert_eq!(e.session_count(), 1);
    assert_eq!(e.broker_connected().len(), 1);
}
#[test]
fn lost_broker_fails_then_clears_state() {
    let mut e = connected();
    e.receive(MODEM, &activate(1, 1, 0, true, 0));
    assert!(e.broker_lost().iter().any(|e|matches!(e,Effect::Send(_,b)if Frame::parse(b).unwrap().tlv(2)==Some([0,0,13,0].as_slice()))));
    assert_eq!(e.session_count(), 0);
}
#[test]
fn network_changes_send_address_change_not_extra_activation() {
    let mut e = connected();
    let a = e.receive(MODEM, &activate(1, 1, 0, false, 0));
    e.report(request(&a), Some(network()));
    let mut n = network();
    n.v4 = Some(Ipv4Addr::new(192, 0, 2, 2));
    let out = e.report(request(&a), Some(n));
    assert!(out
        .iter()
        .any(|e| matches!(e,Effect::Send(_,b)if Frame::parse(b).unwrap().id==ADDRESS_CHANGE)));
}
#[test]
fn malicious_network_metadata_fails_closed() {
    for ip in [
        Ipv4Addr::UNSPECIFIED,
        Ipv4Addr::LOCALHOST,
        Ipv4Addr::new(224, 0, 0, 1),
    ] {
        let mut e = connected();
        let a = e.receive(MODEM, &activate(1, 1, 0, false, 0));
        let mut n = network();
        n.v4 = Some(ip);
        e.report(request(&a), Some(n));
        assert_eq!(e.session_count(), 0);
    }
}
#[test]
fn malformed_length_duplicate_tlv_and_truncated_frames() {
    let b = activate(1, 1, 0, false, 0);
    for n in 0..b.len() {
        assert!(Frame::parse(&b[..n]).is_err());
    }
    let mut e = Encoder::new(Kind::Request, 1, 0x20);
    e.tlv(1, &[]).unwrap();
    e.tlv(1, &[]).unwrap();
    assert!(matches!(Frame::parse(&e.finish()), Err(Error::Duplicate)));
}
#[test]
fn random_modem_bytes_are_bounded_and_never_panic() {
    let mut e = connected();
    let mut x = 0x15ca_fed1u64;
    for n in 0..6000 {
        let len = n % 1100;
        let mut v = vec![0; len];
        for b in &mut v {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = x as u8;
        }
        let out = e.receive(MODEM, &v);
        assert!(out.len() <= 2);
        assert!(e.session_count() <= MAX_SESSIONS);
    }
}
#[test]
fn destruction_and_foreign_deactivation_preserve_other_client() {
    let mut e = connected();
    let a = e.receive(MODEM, &activate(1, 1, 0, false, 0));
    let other = Peer { node: 3, port: 101 };
    e.receive(other, &deactivate(pdp(&a)));
    assert_eq!(e.session_count(), 1);
    assert_eq!(e.diagnostics().modem_releases_by_slot, [0; 4]);
    assert_eq!(e.peer_gone(other).len(), 0);
    assert_eq!(e.peer_gone(MODEM).len(), 1);
}

#[test]
fn instance_destroy_is_client_scoped_and_releases_the_broker() {
    let mut e = connected();
    e.receive(MODEM, &activate(1, 1, 0, true, 0));
    let other = Peer { node: 3, port: 101 };
    e.receive(other, &activate(2, 2, 0, false, 0));
    let mut f = Encoder::new(Kind::Request, 2, 0x33);
    f.tlv(1, &0u32.to_le_bytes()).unwrap();
    let packet = f.finish();
    let out = e.receive(MODEM, &packet);
    assert_eq!(e.session_count(), 1);
    assert!(out.iter().any(|e| matches!(e, Effect::Release(_))));
    let diagnostics = e.diagnostics();
    assert_eq!(diagnostics.modem_instance_destructions_by_slot, [0, 1, 0, 0]);
    assert_eq!(diagnostics.modem_releases_by_slot, [0; 4]);
    assert_eq!(diagnostics.missing_family_releases_by_slot, [0; 4]);
    e.receive(MODEM, &packet); // A duplicate must not count nonexistent removals.
    assert_eq!(e.diagnostics().modem_instance_destructions_by_slot, [0, 1, 0, 0]);
    e.receive(other, &packet);
    assert_eq!(e.diagnostics().modem_instance_destructions_by_slot, [0, 1, 1, 0]);
    assert_eq!(e.session_count(), 0);
}
#[test]
fn malformed_control_payload_is_not_acknowledged_as_success() {
    let mut e = connected();
    for id in [0x23, 0x33, 0x34] {
        let f = Encoder::new(Kind::Request, 1, id).finish();
        let out = e.receive(MODEM, &f);
        let Effect::Send(_, bytes) = &out[0] else {
            panic!()
        };
        assert_eq!(
            Frame::parse(bytes).unwrap().tlv(2),
            Some([1, 0, 0x3a, 0].as_slice())
        );
    }
    assert_eq!(e.diagnostics().modem_instance_destructions_by_slot, [0; 4]);
}
#[test]
fn control_packets_use_exact_uapi_layout() {
    use diamaneos_ims_dcm::qrtr::*;
    let c = Control::server(
        NEW_SERVER,
        Peer {
            node: 1,
            port: 20000,
        },
    );
    assert_eq!(
        c.encode(),
        [4, 0, 0, 0, 2, 3, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0x20, 0x4e, 0, 0]
    );
    assert_eq!(Control::decode(&c.encode()), Some(c));
    for n in 0..20 {
        assert!(Control::decode(&c.encode()[..n]).is_none());
    }
    assert!(!Control {
        command: NEW_SERVER,
        words: [0x302, 1, 3, 4]
    }
    .lookup_complete());
}
#[test]
fn parser_rejects_oversized_apn_invalid_slot_and_wrong_peer() {
    let mut e = connected();
    let out = e.receive(MODEM, &activate(1, 9, 0, false, 0));
    assert_eq!(e.session_count(), 0);
    assert_eq!(out.len(), 1);
    let mut bytes = activate(1, 1, 0, false, 0);
    bytes[10] = 101;
    assert!(Activation::decode(&Frame::parse(&bytes).unwrap(), 2).is_err());
}
#[test]
fn full_capacity_still_accepts_deactivation_and_no_session_leaks() {
    let mut e = connected();
    for i in 0..71 {
        e.receive(MODEM, &activate(i, 1, 0, false, i as u32));
    }
    for i in 0..8 {
        e.receive(MODEM, &activate(i, 1, 0, true, i as u32));
    }
    assert_eq!(e.session_count(), MAX_SESSIONS);
    let out = e.receive(MODEM, &activate(100, 1, 0, true, 9));
    assert_eq!(out.len(), 1);
    for id in 20..=98 {
        e.receive(MODEM, &deactivate(id));
    }
    assert_eq!(e.session_count(), 0);
}

#[test]
fn timezone_echo_and_negative_offset_are_exact() {
    let fields = [12, 34, 15, 26, 9, 2026, 6, (-4i16) as u16];
    let b = timezone_response(9, 20, 0x12345678, fields, 1_790_000_000).unwrap();
    let f = Frame::parse(&b).unwrap();
    assert_eq!((f.id, f.txn), (0x32, 9));
    assert_eq!(b.len(), 52);
    assert_eq!(f.tlv(3), Some([20].as_slice()));
    assert_eq!(f.tlv(4), Some([0x78, 0x56, 0x34, 0x12].as_slice()));
    assert_eq!(&f.tlv(5).unwrap()[14..16], &[0xfc, 0xff]);
    assert!(timezone_response(9, 20, 1, [0; 8], 1).is_err());
    let mut req = Encoder::new(Kind::Request, 9, 0x32);
    req.tlv(1, &[20]).unwrap();
    req.tlv(2, &7u32.to_le_bytes()).unwrap();
    let mut e = connected();
    assert!(matches!(
        &e.receive(MODEM, &req.finish())[..],
        [Effect::ReadTime {
            txn: 9,
            pdp: 20,
            sequence: 7,
            ..
        }]
    ));
}

#[test]
fn conflict_watch_ignores_self_and_rejects_competing_publishers() {
    use diamaneos_ims_dcm::qrtr::*;
    let local = Peer { node: 1, port: 900 };
    assert!(!Control::server(NEW_SERVER, local).conflicts_with(local));
    assert!(Control::server(NEW_SERVER, Peer { node: 1, port: 901 }).conflicts_with(local));
    assert!(Control::server(NEW_SERVER, Peer { node: 3, port: 900 }).conflicts_with(local));
    assert!(!Control::server(DEL_SERVER, MODEM).conflicts_with(local));
}

#[test]
fn lifecycle_diagnostics_distinguish_release_stale_report_and_reactivation() {
    let mut engine = connected();
    let first = engine.receive(MODEM, &activate(1, 1, 0, false, 0));
    let r = request(&first);
    engine.report(r, Some(network()));
    let up = engine.diagnostics();
    assert!(up.broker_connected);
    assert_eq!(up.active_sessions, 1);
    assert_eq!(up.active_groups, 1);
    assert_eq!(up.sessions_by_slot, [0, 1, 0, 0]);
    assert_eq!((up.requests, up.up_reports, up.down_reports), (1, 1, 0));
    engine.report(r, None);
    engine.report(r, Some(network())); // Late callback must not resurrect the group.
    let down = engine.diagnostics();
    assert_eq!((down.active_sessions, down.active_groups), (0, 0));
    assert_eq!((down.down_reports, down.stale_reports), (1, 1));
    assert_eq!(down.modem_releases_by_slot, [0; 4]);
    assert_eq!(down.missing_family_releases_by_slot, [0; 4]);
    engine.peer_gone(MODEM); // A known client can disappear after releasing its last session.
    assert_eq!(engine.diagnostics().client_losses, 1);
    engine.receive(MODEM, &activate(2, 2, 1, false, 0));
    assert_eq!(engine.diagnostics().sessions_by_slot, [0, 0, 1, 0]);
    assert_eq!(engine.diagnostics().requests, 2);
    assert_eq!(engine.diagnostics().last_request, ACTIVATE);
    engine.node_gone(MODEM.node);
    let lost = engine.diagnostics();
    assert_eq!(lost.modem_losses, 1);
    assert_eq!(lost.active_sessions, 0);
    assert_eq!(lost.active_groups, 0);
    engine.receive(MODEM, &[0]);
    assert_eq!(engine.diagnostics().malformed, 1);
}
