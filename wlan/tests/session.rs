// SPDX-License-Identifier: Apache-2.0
use diamaneos_wlan_reporting::{
    session::{Observation, Session},
    Connected,
};
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
fn ack(request: &[u8]) -> [u8; 14] {
    [
        2, request[1], request[2], request[3], request[4], 7, 0, 2, 4, 0, 0, 0, 0, 0,
    ]
}
fn ready(s: &mut Session) {
    for id in [0x27, 0x20, 0x34, 0x20] {
        let p = s.poll(0).unwrap();
        assert_eq!(p[3], id);
        s.receive(&ack(&p));
    }
    assert!(s.poll(0).is_none());
}
#[test]
fn bind_clear_and_ack_before_announcing_available() {
    assert!(Session::new(0).is_none());
    assert!(Session::new(3).is_none());
    let mut a = Session::new(1).unwrap();
    let mut b = Session::new(2).unwrap();
    a.observe(connected());
    b.observe(connected());
    let pa = a.poll(0).unwrap();
    let pb = b.poll(0).unwrap();
    assert_eq!(&pa[10..14], &1_u32.to_le_bytes());
    assert_eq!(&pb[10..14], &2_u32.to_le_bytes());
    a.receive(&ack(&pa));
    b.receive(&ack(&pb));
    let clear = a.poll(0).unwrap();
    assert_eq!(&clear[clear.len() - 8..clear.len() - 4], &[0, 0, 0, 0]);
    assert!(a.poll(1).is_none());
    a.receive(&ack(&clear));
    for id in [0x34, 0x20] {
        let p = a.poll(2).unwrap();
        assert_eq!(p[3], id);
        a.receive(&ack(&p));
    }
    assert!(a.poll(3).is_none());
    // The second subscription cannot inherit the first client's acknowledgements.
    assert_eq!(b.poll(0).unwrap()[3], 0x20);
}
#[test]
fn disconnect_during_pending_up_does_not_retry_stale_up() {
    let mut s = Session::new(1).unwrap();
    s.observe(connected());
    for _ in 0..3 {
        let p = s.poll(0).unwrap();
        s.receive(&ack(&p));
    }
    let up = s.poll(0).unwrap();
    s.observe(Observation {
        enabled: true,
        network: None,
    });
    let replacement = s.poll(2000).unwrap();
    assert_eq!(replacement[3], 0x34);
    assert_ne!(&replacement[1..3], &up[1..3]);
    s.receive(&ack(&up));
    assert!(s.poll(2001).is_none());
    s.receive(&ack(&replacement));
    let down = s.poll(2002).unwrap();
    assert_eq!(down[3], 0x20);
    assert_eq!(&down[down.len() - 8..down.len() - 4], &[0, 0, 0, 0]);
}
#[test]
fn bounded_retries_and_negative_ack_fail_explicitly() {
    let mut s = Session::new(1).unwrap();
    let first = s.poll(0).unwrap();
    let mut unrelated = ack(&first);
    unrelated[1] = 99;
    s.receive(&unrelated);
    assert_eq!(s.poll(2000).unwrap(), first);
    assert_eq!(s.poll(4000).unwrap(), first);
    assert!(s.poll(6000).is_none());
    assert!(s.failed());
    assert_eq!(s.diagnostics().stage, 6);
    assert_eq!(s.diagnostics().operation, 0x27);
    assert_eq!(s.diagnostics().error, -1);
    assert!(s.poll(u64::MAX).is_none());
    let mut s = Session::new(1).unwrap();
    let p = s.poll(0).unwrap();
    let mut no = ack(&p);
    no[10] = 1;
    no[12] = 5;
    s.receive(&no);
    assert!(s.failed());
    assert_eq!(s.diagnostics().error, 5);
}

#[test]
fn diagnostics_distinguish_pending_and_acknowledged_status() {
    let mut s = Session::new(1).unwrap();
    s.observe(connected());
    assert_eq!(s.diagnostics().stage, 0);
    for next_stage in [1, 2, 3, 5] {
        let p = s.poll(0).unwrap();
        s.receive(&ack(&p));
        assert_eq!(s.diagnostics().stage, next_stage);
    }
    s.observe(Observation {
        enabled: false,
        network: None,
    });
    assert_eq!(s.diagnostics().stage, 2);
    for _ in 0..2 {
        let p = s.poll(1).unwrap();
        s.receive(&ack(&p));
    }
    assert_eq!(s.diagnostics().stage, 4);
    assert_eq!(s.diagnostics().error, 0);
}
#[test]
fn new_modem_session_rebinds_and_clears_before_replaying() {
    let mut old = Session::new(1).unwrap();
    old.observe(connected());
    ready(&mut old);
    let mut restarted = Session::new(1).unwrap();
    restarted.observe(connected());
    let bind = restarted.poll(0).unwrap();
    assert_eq!(bind[3], 0x27);
    restarted.receive(&ack(&bind));
    assert_eq!(restarted.poll(1).unwrap()[3], 0x20);
}

#[test]
fn resolver_change_replaces_pending_status_and_is_acknowledged() {
    let mut s = Session::new(1).unwrap();
    let mut old = connected();
    old.network = Some(
        old.network
            .unwrap()
            .with_dns([Some("192.0.2.53".parse().unwrap()), None], [None; 2])
            .unwrap(),
    );
    s.observe(old.clone());
    ready(&mut s);
    let mut next = old.clone();
    next.network = Some(
        next.network
            .unwrap()
            .with_dns([Some("192.0.2.54".parse().unwrap()), None], [None; 2])
            .unwrap(),
    );
    s.observe(next);
    let switch = s.poll(1).unwrap();
    s.receive(&ack(&switch));
    let stale = s.poll(2).unwrap();
    s.observe(old);
    let replacement = s.poll(2002).unwrap();
    assert_ne!(&stale[1..3], &replacement[1..3]);
    s.receive(&ack(&stale));
    assert!(!s.settled());
    s.receive(&ack(&replacement));
    let status = s.poll(2003).unwrap();
    s.receive(&ack(&status));
    assert!(s.settled());
}

#[test]
fn default_route_change_is_reported_without_changing_link_identity() {
    let mut s = Session::new(1).unwrap();
    let mut observation = connected();
    s.observe(observation.clone());
    ready(&mut s);
    observation.network = observation.network.map(|n| n.with_default_route(true));
    s.observe(observation.clone());
    assert!(!s.settled());
    let switch = s.poll(1).unwrap();
    s.receive(&ack(&switch));
    let update = s.poll(2).unwrap();
    assert_eq!(&update[update.len() - 4..], &[0x24, 1, 0, 1]);
    s.receive(&ack(&update));
    assert!(s.settled());
    s.observe(observation);
    assert!(s.poll(3).is_none());
}
