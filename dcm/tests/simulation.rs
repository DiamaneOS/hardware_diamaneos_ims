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

#[test]
fn slotless_service_is_reserved_for_emergency_fallback() {
    for omit in [false, true] {
        for emergency in [false, true] {
            let original = activate(17, 0, 0, emergency, 0);
            let parsed = Frame::parse(&original).unwrap();
            let bytes = if omit {
                let mut encoded = Encoder::new(Kind::Request, 17, ACTIVATE);
                encoded.tlv(1, parsed.required(1).unwrap()).unwrap();
                encoded.finish()
            } else {
                original
            };
            let mut engine = Engine::new(MODEM.node, 2).unwrap();
            engine.broker_connected();
            let effects = engine.receive(MODEM, &bytes);
            assert_eq!(engine.session_count(), usize::from(emergency));
            assert_eq!(
                effects
                    .iter()
                    .any(|effect| matches!(effect, Effect::BringUp(_))),
                emergency
            );
        }
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
        &b[..],
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
        .any(|e| matches!(e, Effect::Release(_) | Effect::ReleaseAfter { .. })));
    assert!(e
        .receive(MODEM, &deactivate(pdp(&b)))
        .iter()
        .any(|e| matches!(e, Effect::Release(_) | Effect::ReleaseAfter { .. })));
}
#[test]
fn deactivation_completes_the_original_context_before_replacement() {
    let mut e = connected();
    let packet = activate(1, 1, 1, false, 0);
    let original = Frame::parse(&packet).unwrap();
    let mut owned = Encoder::new(Kind::Request, 1, ACTIVATE);
    owned.tlv(1, original.required(1).unwrap()).unwrap();
    owned.tlv(0x10, &9u32.to_le_bytes()).unwrap();
    owned.tlv(0x12, original.required(0x12).unwrap()).unwrap();
    owned.tlv(0x13, &7u32.to_le_bytes()).unwrap();
    let packet = owned.finish();
    let activation = e.receive(MODEM, &packet);
    let id = pdp(&activation);
    let old_request = request(&activation);
    e.report(old_request, Some(network()));
    // A repeated activation replaces the cookie, but retains the same owned context.
    let original = Frame::parse(&packet).unwrap();
    let mut refreshed = Encoder::new(Kind::Request, 2, ACTIVATE);
    for tag in [1, 0x12, 0x13] {
        refreshed.tlv(tag, original.required(tag).unwrap()).unwrap();
    }
    refreshed.tlv(0x10, &42u32.to_le_bytes()).unwrap();
    let repeat = e.receive(MODEM, &refreshed.finish());
    assert_eq!(pdp(&repeat), id);
    assert!(!repeat
        .iter()
        .any(|effect| matches!(effect, Effect::BringUp(_))));
    let mut release = Encoder::new(Kind::Request, 55, DEACTIVATE);
    release.tlv(1, &[id]).unwrap();
    release.tlv(0x10, &7u32.to_le_bytes()).unwrap();
    let out = e.receive(MODEM, &release.finish());
    assert_eq!(out.len(), 3);
    let Effect::Send(to, response) = &out[0] else {
        panic!()
    };
    assert_eq!(*to, MODEM);
    let response = Frame::parse(response).unwrap();
    assert_eq!(
        (response.kind, response.id, response.txn),
        (Kind::Response, DEACTIVATE, 55)
    );
    assert_eq!(response.tlv(0x10), Some([250].as_slice()));
    assert_eq!(response.tlv(0x11), Some(7u32.to_le_bytes().as_slice()));
    assert!(
        matches!(out[1], Effect::ReleaseAfter { peer, request } if peer == MODEM && request == old_request)
    );
    let Effect::Send(to, terminal) = &out[2] else {
        panic!()
    };
    assert_eq!(*to, MODEM);
    let terminal = Frame::parse(terminal).unwrap();
    assert_eq!((terminal.kind, terminal.id), (Kind::Indication, ACTIVATE));
    assert_eq!(terminal.tlv(1), Some([id].as_slice()));
    assert_eq!(terminal.tlv(2), Some([0, 0, 13, 0].as_slice()));
    assert_eq!(terminal.tlv(0x10), Some(42u32.to_le_bytes().as_slice()));
    assert_eq!(terminal.tlv(0x12), Some(7u32.to_le_bytes().as_slice()));
    assert!(terminal.tlv(0x11).is_none()); // No stale address in a terminal result.
    assert_eq!(e.session_count(), 0);
    assert!(e.report(old_request, Some(network())).is_empty());
    let repeated = e.receive(MODEM, &deactivate(id));
    assert_eq!(repeated.len(), 1); // An unknown/repeated ID cannot notify a removed owner.
    let replacement = e.receive(MODEM, &activate(2, 1, 1, false, 0));
    assert_ne!(request(&replacement).serial, old_request.serial);
    assert_eq!(e.report(request(&replacement), Some(network())).len(), 1);
}

#[test]
fn deactivation_notification_is_owner_scoped_and_preserves_shared_bearer() {
    let mut e = connected();
    let first = e.receive(MODEM, &activate(1, 1, 0, false, 0));
    let second = e.receive(MODEM, &activate(2, 1, 1, false, 0));
    let id = pdp(&first);
    let other = Peer {
        node: MODEM.node,
        port: MODEM.port + 1,
    };
    assert_eq!(e.receive(other, &deactivate(id)).len(), 1);
    let mut wrong_instance = Encoder::new(Kind::Request, 55, DEACTIVATE);
    wrong_instance.tlv(1, &[id]).unwrap();
    wrong_instance.tlv(0x10, &1u32.to_le_bytes()).unwrap();
    assert_eq!(e.receive(MODEM, &wrong_instance.finish()).len(), 1);
    assert_eq!(e.session_count(), 2);
    assert_eq!(e.report(request(&first), Some(network())).len(), 2);
    let out = e.receive(MODEM, &deactivate(id));
    assert_eq!(out.len(), 2); // Response, terminal result; shared bearer remains requested.
    assert!(!out
        .iter()
        .any(|x| matches!(x, Effect::Release(_) | Effect::ReleaseAfter { .. })));
    assert_eq!(e.session_count(), 1);
    let up = e.report(request(&first), Some(network()));
    assert!(up.is_empty()); // Survivor keeps its existing address; no redundant UP notice.
    let mut changed = network();
    changed.v6 = Some("2001:db8::2".parse().unwrap());
    let up = e.report(request(&first), Some(changed));
    assert_eq!(up.len(), 1);
    let Effect::Send(to, bytes) = &up[0] else {
        panic!()
    };
    assert_eq!(*to, MODEM);
    assert_eq!(
        Frame::parse(bytes).unwrap().tlv(1),
        Some([pdp(&second)].as_slice())
    );
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
fn undelivered_bring_up_completes_the_sessions_instead_of_waiting() {
    let mut e = connected();
    let a = e.receive(MODEM, &activate(1, 1, 0, false, 0));
    let r = request(&a);
    let out = e.bring_up_failed(r);
    assert!(out.iter().any(|x| matches!(x, Effect::Send(_, b)
        if Frame::parse(b).unwrap().tlv(2) == Some([0, 0, 13, 0].as_slice()))));
    assert!(out
        .iter()
        .any(|x| matches!(x, Effect::Release(released) if *released == r)));
    assert_eq!(e.session_count(), 0);
    assert!(e.bring_up_failed(r).is_empty()); // A stale failure has nothing left to end.
    e.release_failed();
    assert_eq!(e.diagnostics().broker_call_failures, 3);
    assert_eq!(e.diagnostics().broker_losses, 0);
    let b = e.receive(MODEM, &activate(2, 1, 0, false, 0));
    assert_ne!(request(&b).serial, r.serial); // The modem's retry files a fresh request.
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
    assert!(out
        .iter()
        .any(|e| matches!(e, Effect::Release(_) | Effect::ReleaseAfter { .. })));
    let diagnostics = e.diagnostics();
    assert_eq!(
        diagnostics.modem_instance_destructions_by_slot,
        [0, 1, 0, 0]
    );
    assert_eq!(diagnostics.modem_releases_by_slot, [0; 4]);
    assert_eq!(diagnostics.missing_family_releases_by_slot, [0; 4]);
    e.receive(MODEM, &packet); // A duplicate must not count nonexistent removals.
    assert_eq!(
        e.diagnostics().modem_instance_destructions_by_slot,
        [0, 1, 0, 0]
    );
    e.receive(other, &packet);
    assert_eq!(
        e.diagnostics().modem_instance_destructions_by_slot,
        [0, 1, 1, 0]
    );
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
fn conflict_watch_ignores_self_and_remote_nodes_and_rejects_local_publishers() {
    use diamaneos_ims_dcm::qrtr::*;
    let local = Peer { node: 1, port: 900 };
    // The daemon's own record, also a stale one for the same fixed port.
    assert!(!Control::server(NEW_SERVER, local).conflicts_with(local));
    assert!(!Control::server(NEW_SERVER, local).remote_server(local));
    assert!(Control::server(NEW_SERVER, Peer { node: 1, port: 901 }).conflicts_with(local));
    let remote = Control::server(NEW_SERVER, Peer { node: 3, port: 900 });
    assert!(!remote.conflicts_with(local));
    assert!(remote.remote_server(local));
    assert!(!Control::server(DEL_SERVER, MODEM).conflicts_with(local));
    assert!(!Control::server(DEL_SERVER, MODEM).remote_server(local));
    let complete = Control::decode(&[4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    assert!(complete.is_some_and(|c| c.lookup_complete()
        && !c.conflicts_with(local)
        && !c.remote_server(local)));
}

#[test]
fn publication_keeps_modem_requests_and_notices_but_not_local_datagrams() {
    use diamaneos_ims_dcm::qrtr::*;
    let local = Peer {
        node: 1,
        port: 0x7fff,
    };
    let control = Peer {
        node: 1,
        port: CTRL_PORT,
    };
    let competitor = Control::server(NEW_SERVER, Peer { node: 1, port: 901 }).encode();
    assert_eq!(lookup_input(local, control, &competitor), Lookup::Conflict);
    // Only the local name service decides publication; the engine drops the rest.
    let remote_control = Peer {
        node: MODEM.node,
        port: CTRL_PORT,
    };
    assert_eq!(
        lookup_input(local, remote_control, &competitor),
        Lookup::Keep
    );
    let complete = Control {
        command: NEW_SERVER,
        words: [0; 4],
    };
    assert_eq!(
        lookup_input(local, control, &complete.encode()),
        Lookup::Complete
    );
    // Own and remote records, and loss notices, go to the main loop's rules.
    for notice in [
        Control::server(NEW_SERVER, local),
        Control::server(NEW_SERVER, MODEM),
        Control {
            command: BYE,
            words: [MODEM.node, 0, 0, 0],
        },
    ] {
        assert_eq!(lookup_input(local, control, &notice.encode()), Lookup::Keep);
    }
    let request = activate(1, 1, 0, false, 0);
    assert_eq!(lookup_input(local, MODEM, &request), Lookup::Keep);
    // A local client cannot fill the bounded buffer ahead of modem requests.
    let client = Peer { node: 1, port: 77 };
    assert_eq!(lookup_input(local, client, &request), Lookup::Ignore);
    assert_eq!(lookup_input(local, client, &competitor), Lookup::Ignore);
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
    let held = engine.diagnostics();
    assert_eq!(
        (held.active_sessions, held.active_groups, held.held_groups),
        (0, 1, 1)
    );
    engine.failed(r);
    engine.report(r, Some(network())); // Late callback must not resurrect the group.
    let down = engine.diagnostics();
    assert_eq!((down.active_sessions, down.active_groups), (0, 0));
    assert_eq!(
        (down.down_reports, down.failed_reports, down.stale_reports),
        (1, 1, 1)
    );
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

// Network loss keeps the request, as stock does

fn terminals(effects: &[Effect]) -> Vec<u8> {
    effects
        .iter()
        .filter_map(|e| match e {
            Effect::Send(_, b) => {
                let f = Frame::parse(b).ok()?;
                (f.kind == Kind::Indication && f.tlv(2) == Some([0, 0, 13, 0].as_slice()))
                    .then(|| f.tlv(1).unwrap()[0])
            }
            _ => None,
        })
        .collect()
}
fn releases(effects: &[Effect]) -> Vec<Request> {
    effects
        .iter()
        .filter_map(|e| match e {
            Effect::Release(r) | Effect::ReleaseAfter { request: r, .. } => Some(*r),
            _ => None,
        })
        .collect()
}
fn files(effects: &[Effect]) -> bool {
    effects.iter().any(|e| matches!(e, Effect::BringUp(_)))
}
/// An initial result with this address, never an address change.
fn initial_up(effect: &Effect, address: &str) -> bool {
    let Effect::Send(_, b) = effect else {
        return false;
    };
    let f = Frame::parse(b).unwrap();
    (f.kind, f.id) == (Kind::Indication, ACTIVATE)
        && f.tlv(2) == Some([0, 0, 0, 0].as_slice())
        && b.windows(address.len()).any(|w| w == address.as_bytes())
}
fn wifi() -> Network {
    Network {
        handle: 43,
        v4: Some(Ipv4Addr::new(198, 51, 100, 7)),
        v6: Some("2001:db8:1::7".parse::<Ipv6Addr>().unwrap()),
        mtu: 1280,
    }
}
fn destroy(instance: u32) -> Vec<u8> {
    let mut e = Encoder::new(Kind::Request, 66, 0x33);
    e.tlv(1, &instance.to_le_bytes()).unwrap();
    e.finish()
}

#[test]
fn failed_handover_keeps_the_request_for_the_modems_next_activation() {
    // A failed LTE to Wi-Fi handover makes telephony tear the LTE network down
    // and retry on Wi-Fi for the same request. The request must still be there.
    let mut e = connected();
    let v4 = e.receive(MODEM, &activate(1, 1, 0, false, 0));
    let r = request(&v4);
    let v6 = e.receive(MODEM, &activate(2, 1, 1, false, 0));
    assert_eq!(e.report(r, Some(network())).len(), 2);
    let lost = e.report(r, None);
    assert_eq!(terminals(&lost), [pdp(&v4), pdp(&v6)]);
    assert!(releases(&lost).is_empty());
    assert!(e.request_is_current(r));
    let d = e.diagnostics();
    assert_eq!(
        (d.active_sessions, d.active_groups, d.held_groups),
        (0, 1, 1)
    );
    // The retry brings the network back over Wi-Fi before the modem asks again.
    assert!(e.report(r, Some(wifi())).is_empty());
    let mut ids = vec![];
    for (txn, family, address) in [(3, 0, "198.51.100.7"), (4, 1, "2001:db8:1::7")] {
        let out = e.receive(MODEM, &activate(txn, 1, family, false, 0));
        assert!(!files(&out), "the held request serves the new context");
        assert_eq!(out.len(), 2);
        assert!(initial_up(&out[1], address));
        ids.push(pdp(&out));
    }
    assert_eq!(e.diagnostics().held_groups, 0);
    // The modem's own release still ends the request with its last context.
    assert!(releases(&e.receive(MODEM, &deactivate(ids[0]))).is_empty());
    assert_eq!(releases(&e.receive(MODEM, &deactivate(ids[1]))), [r]);
    assert_eq!(e.diagnostics().active_groups, 0);
}

#[test]
fn activation_during_the_loss_waits_for_the_returning_network() {
    let mut e = connected();
    let first = e.receive(MODEM, &activate(1, 2, 1, false, 0));
    let r = request(&first);
    e.report(r, Some(network()));
    e.report(r, None);
    // Acknowledged only: no second filing and no result without a network.
    let again = e.receive(MODEM, &activate(2, 2, 1, false, 0));
    assert_eq!(again.len(), 1);
    // The same address on the returning network is a fresh initial result.
    let up = e.report(r, Some(network()));
    assert_eq!(up.len(), 1);
    assert!(initial_up(&up[0], "2001:db8::1"));
}

#[test]
fn a_failure_report_releases_the_request_unlike_a_loss() {
    let mut e = connected();
    let a = e.receive(MODEM, &activate(1, 1, 0, false, 0));
    let r = request(&a);
    let stale = Request {
        serial: r.serial + 1,
        ..r
    };
    assert!(e.failed(stale).is_empty());
    let out = e.failed(r);
    assert_eq!(terminals(&out), [pdp(&a)]);
    assert_eq!(releases(&out), [r]);
    // A request held after a loss is released by a failure too.
    let b = e.receive(MODEM, &activate(2, 1, 0, false, 0));
    let held = request(&b);
    assert!(held.serial > r.serial);
    e.report(held, None);
    assert_eq!(releases(&e.failed(held)), [held]);
    let d = e.diagnostics();
    assert_eq!(
        (d.failed_reports, d.stale_reports, d.active_groups),
        (2, 1, 0)
    );
}

#[test]
fn a_held_request_ends_with_whatever_ends_its_holder() {
    let other = Peer {
        node: MODEM.node,
        port: MODEM.port + 1,
    };
    type End = fn(&mut Engine, Request) -> Vec<Effect>;
    let ends: [(&str, End); 6] = [
        ("client gone", |e, _| e.peer_gone(MODEM)),
        ("modem gone", |e, _| e.node_gone(MODEM.node)),
        ("shutdown", |e, _| e.shutdown()),
        ("broker lost", |e, _| e.broker_lost()),
        ("request failed", |e, r| e.failed(r)),
        ("instance destroyed", |e, _| e.receive(MODEM, &destroy(0))),
    ];
    for (what, end) in ends {
        let mut e = connected();
        let a = e.receive(MODEM, &activate(1, 1, 0, false, 0));
        let r = request(&a);
        e.report(r, None);
        // None of these belong to the holder, so the request stays.
        assert_eq!(e.receive(MODEM, &deactivate(pdp(&a))).len(), 1, "{what}");
        assert!(e.peer_gone(other).is_empty(), "{what}");
        assert!(
            releases(&e.receive(MODEM, &destroy(1))).is_empty(),
            "{what}"
        );
        assert!(e.set_emergency_enabled(false).is_empty(), "{what}");
        assert!(e.request_is_current(r), "{what}");
        assert_eq!(releases(&end(&mut e, r)), [r], "{what}");
        assert_eq!(e.diagnostics().active_groups, 0, "{what}");
    }
    // The kill switch ends a held emergency request.
    let mut e = connected();
    let a = e.receive(MODEM, &activate(1, 1, 0, true, 0));
    let r = request(&a);
    e.report(r, None);
    assert_eq!(releases(&e.set_emergency_enabled(false)), [r]);
}

#[test]
fn a_shared_request_stays_held_until_every_holder_lets_go() {
    let other = Peer {
        node: MODEM.node,
        port: MODEM.port + 1,
    };
    let mut e = connected();
    let a = e.receive(MODEM, &activate(1, 1, 0, false, 0));
    let r = request(&a);
    e.receive(other, &activate(2, 1, 1, false, 0));
    e.report(r, Some(network()));
    assert_eq!(terminals(&e.report(r, None)).len(), 2);
    assert!(e.peer_gone(other).is_empty());
    let again = e.receive(MODEM, &activate(3, 1, 0, false, 0));
    assert!(!files(&again));
    assert_eq!(releases(&e.receive(MODEM, &deactivate(pdp(&again)))), [r]);
}

#[test]
fn holds_stay_bounded_across_many_instances_and_losses() {
    let mut e = connected();
    let mut r = None;
    for instance in 0..300u32 {
        let original = activate(1, 1, 0, false, 0);
        let parsed = Frame::parse(&original).unwrap();
        let mut owned = Encoder::new(Kind::Request, instance as u16, ACTIVATE);
        owned.tlv(1, parsed.required(1).unwrap()).unwrap();
        owned.tlv(0x12, parsed.required(0x12).unwrap()).unwrap();
        owned.tlv(0x13, &instance.to_le_bytes()).unwrap();
        let out = e.receive(MODEM, &owned.finish());
        let current = *r.get_or_insert_with(|| request(&out));
        assert!(!files(&out) || instance == 0);
        assert_eq!(terminals(&e.report(current, None)).len(), 1);
    }
    let r = r.unwrap();
    assert!(e.request_is_current(r));
    assert_eq!(releases(&e.peer_gone(MODEM)), [r]);
}
