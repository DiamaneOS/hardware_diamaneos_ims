// SPDX-License-Identifier: Apache-2.0
use diamaneos_wlan_reporting::{
    keepalive_instruction, keepalive_operation_failed,
    session::{Observation, Session},
    Connected,
};

fn instruction() -> Vec<u8> {
    vec![4, 0, 0, 0x41, 0, 4, 0, 1, 1, 0, 1]
}
fn ack(request: &[u8]) -> [u8; 14] {
    [
        2, request[1], request[2], request[3], request[4], 7, 0, 2, 4, 0, 0, 0, 0, 0,
    ]
}
fn connected() -> Observation {
    Observation {
        enabled: true,
        network: Some(
            Connected::new(
                [2, 0, 0, 0, 0, 1],
                Some("192.0.2.1".parse().unwrap()),
                None,
                true,
            )
            .unwrap(),
        ),
    }
}
fn ready() -> Session {
    let mut s = Session::new(1).unwrap();
    s.observe(connected());
    for id in [0x27, 0x20, 0x43, 0x34, 0x43, 0x20] {
        let p = s.poll(0).unwrap();
        assert_eq!(p[3], id);
        s.receive(&ack(&p));
    }
    assert!(s.settled());
    s
}

#[test]
fn known_instruction_and_failure_bytes_are_exact_and_bounded() {
    let p = instruction();
    assert!(keepalive_instruction(&p));
    // Full optional-field layout from the independent stock IDL descriptor.
    let mut full = vec![
        4, 0, 0, 0x41, 0, 47, 0, 1, 1, 0, 1, 0x10, 4, 0, 1, 113, 0, 203, 0x11, 16, 0,
    ];
    full.extend([0x20, 1, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
    full.extend([
        0x12, 2, 0, 0x94, 0x11, 0x13, 2, 0, 0x94, 0x11, 0x14, 4, 0, 20, 0, 0, 0,
    ]);
    assert_eq!(full.len(), 54);
    assert!(keepalive_instruction(&full));
    assert_eq!(
        keepalive_operation_failed(0x1234).unwrap(),
        [0, 0x34, 0x12, 0x42, 0, 7, 0, 1, 4, 0, 1, 0, 0, 0]
    );
    for n in 0..p.len() {
        assert!(!keepalive_instruction(&p[..n]));
    }
    let mut unknown = p.clone();
    unknown.extend([0x15, 1, 0, 0]);
    unknown[5] += 4;
    assert!(!keepalive_instruction(&unknown));
    let mut duplicate = p.clone();
    duplicate.extend([1, 1, 0, 1]);
    duplicate[5] += 4;
    assert!(!keepalive_instruction(&duplicate));
    let mut wrong = p.clone();
    wrong[0] = 0;
    assert!(!keepalive_instruction(&wrong));
    let mut malformed = p;
    malformed[8] = 2;
    assert!(!keepalive_instruction(&malformed));
}

#[test]
fn independent_reply_cannot_consume_status_or_block_network_loss() {
    let mut s = ready();
    s.receive(&instruction());
    let failure = s.poll_cycle(1)[1].take().unwrap();
    assert_eq!(failure[3], 0x42);
    s.observe(Observation {
        enabled: false,
        network: None,
    });
    let down = s.poll(2).unwrap();
    assert_eq!(down[3], 0x43);
    assert_ne!(&failure[1..3], &down[1..3]);
    // Keepalive completion does not advance the pending profile withdrawal.
    s.receive(&ack(&failure));
    assert!(s.poll(3).is_none());
    s.receive(&ack(&down));
    let station = s.poll(4).unwrap();
    assert_eq!(station[3], 0x20);
    assert_eq!(s.keepalive_diagnostics().acknowledged, 1);
}

#[test]
fn burst_capacity_is_fixed_and_unbound_instructions_are_ignored() {
    let mut unbound = Session::new(1).unwrap();
    unbound.observe(connected());
    unbound.receive(&instruction());
    assert_eq!(unbound.keepalive_diagnostics().sent, 0);
    for id in [0x27, 0x20, 0x43, 0x34, 0x43, 0x20] {
        let p = unbound.poll(0).unwrap();
        assert_eq!(p[3], id);
        unbound.receive(&ack(&p));
    }
    assert!(unbound.poll(1).is_none());
    let mut s = ready();
    for _ in 0..1000 {
        s.receive(&instruction());
    }
    for now in 0..8 {
        let p = s.poll_cycle(now)[1].take().unwrap();
        assert_eq!(p[3], 0x42);
        s.receive(&ack(&p));
    }
    assert!(s.poll_cycle(9).into_iter().all(|p| p.is_none()));
    assert_eq!(s.keepalive_diagnostics().sent, 8);
    assert_eq!(s.keepalive_diagnostics().dropped, 992);
    assert_eq!(s.keepalive_diagnostics().acknowledged, 8);
}

#[test]
fn timeout_or_rejection_is_explicit_without_breaking_connectivity_state() {
    for reject in [false, true] {
        let mut s = ready();
        s.receive(&instruction());
        let p = s.poll_cycle(0)[1].take().unwrap();
        if reject {
            let mut reply = ack(&p);
            reply[10] = 1;
            reply[12] = 5;
            s.receive(&reply);
            assert_eq!(s.keepalive_diagnostics().error, 5);
        } else {
            assert_eq!(s.poll_cycle(2000)[1].take().unwrap(), p);
            assert_eq!(s.poll_cycle(4000)[1].take().unwrap(), p);
            assert!(s.poll_cycle(6000).into_iter().all(|p| p.is_none()));
            assert_eq!(s.keepalive_diagnostics().error, -1);
        }
        s.receive(&instruction());
        assert!(s.poll_cycle(7000).into_iter().all(|p| p.is_none()));
        assert!(s.settled());
        assert!(!s.failed());
    }
}

#[test]
fn continuous_control_traffic_cannot_starve_reply_deadlines_or_terminal_expiry() {
    let mut s = ready();
    s.receive(&instruction());
    let original = s.poll_cycle(0)[1].take().unwrap();
    for now in [2000, 4000, 6000] {
        s.observe(Observation {
            enabled: false,
            network: None,
        });
        let [control, reply] = s.poll_cycle(now);
        let control = control.unwrap();
        assert_eq!(control[3], 0x43);
        s.receive(&ack(&control));
        for id in [0x20, 0x34] {
            let p = s.poll(now).unwrap();
            assert_eq!(p[3], id);
            s.receive(&ack(&p));
        }
        s.observe(connected());
        for id in [0x34, 0x43, 0x20] {
            let p = s.poll(now).unwrap();
            assert_eq!(p[3], id);
            s.receive(&ack(&p));
        }
        if now < 6000 {
            assert_eq!(reply.unwrap(), original);
        } else {
            assert!(reply.is_none());
        }
    }
    assert_eq!(s.keepalive_diagnostics().error, -1);
    assert!(s.settled());
}

#[test]
fn stale_ack_cannot_complete_the_next_queued_failure() {
    let mut s = ready();
    s.receive(&instruction());
    s.receive(&instruction());
    let first = s.poll_cycle(0)[1].take().unwrap();
    s.receive(&ack(&first));
    let second = s.poll_cycle(1)[1].take().unwrap();
    assert_ne!(&first[1..3], &second[1..3]);
    s.receive(&ack(&first));
    assert_eq!(s.keepalive_diagnostics().acknowledged, 1);
    assert_eq!(s.poll_cycle(2001)[1].take().unwrap(), second);
    s.receive(&ack(&second));
    assert_eq!(s.keepalive_diagnostics().acknowledged, 2);
}
