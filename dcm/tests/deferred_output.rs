// SPDX-License-Identifier: Apache-2.0
use diamaneos_ims_dcm::{
    engine::{Effect, Engine, Network, Peer, Request},
    outgoing::{Disposition, Retained},
    protocol::{Encoder, Frame, Kind, ACTIVATE, DEACTIVATE},
};
const A: Peer = Peer { node: 3, port: 100 };
const B: Peer = Peer { node: 3, port: 101 };
fn activate(transaction: u16) -> Vec<u8> {
    let mut p = Encoder::new(Kind::Request, transaction, ACTIVATE);
    let mut value = vec![3];
    value.extend(b"ims");
    value.extend([0u32, 0, 0, 0].into_iter().flat_map(u32::to_le_bytes));
    p.tlv(1, &value).unwrap();
    p.tlv(0x12, &1u32.to_le_bytes()).unwrap();
    p.finish()
}
fn deactivate(id: u8) -> Vec<u8> {
    let mut p = Encoder::new(Kind::Request, 80, DEACTIVATE);
    p.tlv(1, &[id]).unwrap();
    p.finish()
}
fn retain(engine: &mut Engine, effects: Vec<Effect>) -> (Vec<Retained>, Option<Request>) {
    let mut packets = vec![];
    let mut request = None;
    for effect in effects {
        match effect {
            Effect::Send(peer, packet) => packets.push(engine.retain_output(peer, packet).unwrap()),
            Effect::BringUp(value) => request = Some(value),
            _ => (),
        }
    }
    (packets, request)
}
fn network(last: u8, handle: u64) -> Network {
    Network {
        handle,
        v4: Some([192, 0, 2, last].into()),
        v6: None,
        mtu: 1500,
    }
}
fn prepared_sid(engine: &Engine, packet: &Retained) -> u8 {
    let bytes = engine.prepare_output(packet).unwrap();
    Frame::parse(&bytes).unwrap().required(0x10).unwrap()[0]
}

#[test]
fn stale_initial_up_is_discarded_and_replacement_remains_initial_up() {
    let mut e = Engine::new(A.node, 2).unwrap();
    e.broker_connected();
    let effects = e.receive(A, &activate(1));
    let (mut replies, request) = retain(&mut e, effects);
    e.finish_output(replies.pop().unwrap(), Disposition::Submitted);
    let request = request.unwrap();
    let effects = e.report(request, Some(network(1, 10)));
    let (mut old, _) = retain(&mut e, effects);
    let effects = e.report(request, Some(network(2, 11)));
    let (mut new, _) = retain(&mut e, effects);
    assert!(e.prepare_output(&old[0]).is_none());
    e.finish_output(old.pop().unwrap(), Disposition::Obsolete);
    let bytes = e.prepare_output(&new[0]).unwrap();
    assert_eq!(Frame::parse(&bytes).unwrap().id, ACTIVATE);
    e.finish_output(new.pop().unwrap(), Disposition::Submitted);
    let effects = e.report(request, Some(network(3, 12)));
    let (new, _) = retain(&mut e, effects);
    assert_eq!(
        Frame::parse(&e.prepare_output(&new[0]).unwrap())
            .unwrap()
            .id,
        0x24
    );
    for p in new {
        e.finish_output(p, Disposition::Submitted);
    }
}

#[test]
fn retired_id_is_quarantined_until_owned_replies_and_terminal_are_finished() {
    let mut e = Engine::new(A.node, 2).unwrap();
    e.broker_connected();
    let effects = e.receive(A, &activate(1));
    let (mut first, _) = retain(&mut e, effects);
    let id = prepared_sid(&e, &first[0]);
    let effects = e.receive(A, &deactivate(id));
    let (retired, _) = retain(&mut e, effects);
    assert!(e.prepare_output(&first[0]).is_some()); // Preserve the awaited confirmation.
    let effects = e.receive(A, &activate(2));
    let (mut second, _) = retain(&mut e, effects);
    let second_id = prepared_sid(&e, &second[0]);
    assert_ne!(id, second_id);
    e.finish_output(first.pop().unwrap(), Disposition::Submitted);
    for p in retired {
        assert!(e.prepare_output(&p).is_some());
        e.finish_output(p, Disposition::Submitted);
    }
    e.finish_output(second.pop().unwrap(), Disposition::Submitted);
    let effects = e.receive(A, &deactivate(second_id));
    let (retired, _) = retain(&mut e, effects);
    for p in retired {
        e.finish_output(p, Disposition::Submitted);
    }
    let effects = e.receive(A, &activate(3));
    let (packets, _) = retain(&mut e, effects);
    assert_eq!(prepared_sid(&e, &packets[0]), id);
    for p in packets {
        e.finish_output(p, Disposition::Submitted);
    }
}

#[test]
fn peer_loss_invalidates_old_output_without_dropping_another_peer_reply() {
    let mut e = Engine::new(A.node, 2).unwrap();
    e.broker_connected();
    let effects = e.receive(A, &activate(1));
    let (a, _) = retain(&mut e, effects);
    let effects = e.receive(B, &activate(2));
    let (b, _) = retain(&mut e, effects);
    e.peer_gone(A);
    assert!(e.prepare_output(&a[0]).is_none());
    assert!(e.prepare_output(&b[0]).is_some());
    for p in a {
        e.finish_output(p, Disposition::PeerLost);
    }
    for p in b {
        e.finish_output(p, Disposition::Submitted);
    }
}

#[test]
fn network_handle_replacement_with_same_address_invalidates_queued_old_up() {
    let mut e = Engine::new(A.node, 2).unwrap();
    e.broker_connected();
    let effects = e.receive(A, &activate(1));
    let (packets, request) = retain(&mut e, effects);
    for p in packets {
        e.finish_output(p, Disposition::Submitted);
    }
    let request = request.unwrap();
    let effects = e.report(request, Some(network(1, 10)));
    let (old, _) = retain(&mut e, effects);
    let effects = e.report(request, Some(network(1, 11)));
    let (new, _) = retain(&mut e, effects);
    assert!(e.prepare_output(&old[0]).is_none());
    assert!(e.prepare_output(&new[0]).is_some());
    for p in old {
        e.finish_output(p, Disposition::Obsolete);
    }
    for p in new {
        e.finish_output(p, Disposition::Submitted);
    }
}

#[test]
fn a_transaction_reply_without_an_admitted_peer_lifetime_cannot_be_deferred() {
    let mut e = Engine::new(A.node, 2).unwrap();
    let effects = e.receive(A, &[0, 1, 0, 0x20, 0, 1, 0]);
    assert!(!e.peer_is_tracked(A));
    for effect in effects {
        if let Effect::Send(peer, packet) = effect {
            assert!(e.retain_output(peer, packet).is_none());
        }
    }
}

#[test]
fn retiring_a_peer_does_not_turn_its_pending_up_into_an_unadmitted_reply() {
    let mut e = Engine::new(A.node, 2).unwrap();
    e.broker_connected();
    let effects = e.receive(A, &activate(1));
    let (packets, request) = retain(&mut e, effects);
    for p in packets {
        e.finish_output(p, Disposition::Submitted);
    }
    let request = request.unwrap();
    assert!(e.request_is_current(request));
    let pending = e.report(request, Some(network(1, 10)));
    e.peer_gone(A);
    assert!(!e.request_is_current(request));
    for effect in pending {
        if let Effect::Send(peer, packet) = effect {
            assert!(!packet.is_unadmitted_reply());
            assert!(e.retain_output(peer, packet).is_none());
        }
    }
    let rejected = e.receive(A, &[0, 1, 0, 0x20, 0, 1, 0]);
    for effect in rejected {
        if let Effect::Send(_, packet) = effect {
            assert!(packet.is_unadmitted_reply());
        }
    }
}

#[test]
fn known_broker_loss_invalidates_retained_up_before_submission() {
    let mut e = Engine::new(A.node, 2).unwrap();
    e.broker_connected();
    let effects = e.receive(A, &activate(1));
    let (packets, request) = retain(&mut e, effects);
    for p in packets {
        e.finish_output(p, Disposition::Submitted);
    }
    let request = request.unwrap();
    let effects = e.report(request, Some(network(1, 10)));
    let (pending, _) = retain(&mut e, effects);
    e.broker_lost();
    assert!(!e.request_is_current(request));
    for p in pending {
        assert!(e.prepare_output(&p).is_none());
        e.finish_output(p, Disposition::Obsolete);
    }
}

#[test]
fn a_stalled_peer_gets_terminal_results_before_its_request_is_released() {
    let mut e = Engine::new(A.node, 2).unwrap();
    e.broker_connected();
    let effects = e.receive(A, &activate(1));
    let (packets, request) = retain(&mut e, effects);
    for p in packets {
        e.finish_output(p, Disposition::Submitted);
    }
    let request = request.unwrap();
    let effects = e.report(request, Some(network(1, 10)));
    let (pending, _) = retain(&mut e, effects);
    let effects = e.peer_stalled(A);
    assert!(effects
        .iter()
        .any(|effect| matches!(effect, Effect::Release(released) if *released == request)));
    // The client is still tracked, so its terminal result keeps a lifetime.
    assert!(e.peer_is_tracked(A));
    let (terminal, _) = retain(&mut e, effects);
    assert_eq!(terminal.len(), 1);
    let bytes = e.prepare_output(&terminal[0]).unwrap();
    let frame = Frame::parse(&bytes).unwrap();
    assert_eq!((frame.kind, frame.id), (Kind::Indication, ACTIVATE));
    assert!(frame.tlv(0x11).is_none());
    // The stalled UP no longer describes a session.
    assert!(e.prepare_output(&pending[0]).is_none());
    for p in pending {
        e.finish_output(p, Disposition::Obsolete);
    }
    for p in terminal {
        e.finish_output(p, Disposition::Submitted);
    }
    assert!(e.peer_stalled(A).is_empty());
    let diagnostics = e.diagnostics();
    assert_eq!(
        (diagnostics.client_stalls, diagnostics.client_losses),
        (1, 0)
    );
    assert_eq!(diagnostics.active_sessions, 0);
}
