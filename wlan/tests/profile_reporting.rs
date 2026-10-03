// SPDX-License-Identifier: Apache-2.0
use diamaneos_ims_dcm::protocol::{Encoder, Kind};
use diamaneos_wlan_reporting::{
    session::{Observation, Session},
    Connected,
};

fn ack(p: &[u8]) -> Vec<u8> {
    vec![2, p[1], p[2], p[3], p[4], 7, 0, 2, 4, 0, 0, 0, 0, 0]
}
fn init(profile: u32, id: u64) -> Vec<u8> {
    let mut e = Encoder::new(Kind::Indication, 0, 0x45);
    e.tlv(1, &profile.to_le_bytes()).unwrap();
    e.tlv(0x13, &id.to_le_bytes()).unwrap();
    e.finish()
}
fn select(mask: u64, id: u64) -> Vec<u8> {
    let mut e = Encoder::new(Kind::Indication, 0, 0x3f);
    e.tlv(1, &mask.to_le_bytes()).unwrap();
    e.tlv(0x11, &id.to_le_bytes()).unwrap();
    e.finish()
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
            .unwrap()
            .with_default_route(true),
        ),
    }
}
fn ready() -> Session {
    let mut s = Session::new(1, 7).unwrap();
    s.observe(connected());
    for id in [0x27, 0x38, 0x20, 0x43, 0x34, 0x43, 0x20] {
        let p = s.poll(0).unwrap();
        assert_eq!(p[3], id);
        s.receive(&ack(&p));
    }
    assert!(s.settled());
    s
}
fn request(s: &mut Session) {
    s.receive(&init(35, 0));
    s.receive(&select(1_u64 << 63, 0));
}

#[test]
fn three_channels_are_distinct_and_profile_ack_cannot_consume_core_withdrawal() {
    let mut s = ready();
    request(&mut s);
    s.receive(&[4, 0, 0, 0x41, 0, 4, 0, 1, 1, 0, 1]);
    s.observe(Observation {
        enabled: false,
        network: None,
    });
    let [core, nat, profile] = s.poll_cycle(1);
    let (core, nat, profile) = (core.unwrap(), nat.unwrap(), profile.unwrap());
    assert_eq!((core[3], nat[3], profile[3]), (0x43, 0x42, 0x43));
    assert_ne!(&core[1..3], &profile[1..3]);
    assert_ne!(&nat[1..3], &profile[1..3]);
    s.receive(&ack(&profile));
    s.receive(&ack(&nat));
    assert!(s.poll(2).is_none());
    assert_eq!(s.profile_report_diagnostics().acknowledged, 1);
    s.receive(&ack(&core));
    assert_eq!(s.poll(3).unwrap()[3], 0x20);
}

#[test]
fn destruction_cancels_retry_and_old_ack_cannot_complete_reinitialization() {
    let mut s = ready();
    request(&mut s);
    let old = s.poll_cycle(0)[2].take().unwrap();
    s.receive(&select(0, 0));
    assert_eq!(s.profile_report_diagnostics().cancelled, 1);
    assert!(s.poll_cycle(2000)[2].is_none());
    request(&mut s);
    let new = s.poll_cycle(2001)[2].take().unwrap();
    assert_ne!(&old[1..3], &new[1..3]);
    s.receive(&ack(&old));
    assert_eq!(s.profile_report_diagnostics().acknowledged, 0);
    s.receive(&ack(&new));
    assert_eq!(s.profile_report_diagnostics().acknowledged, 1);
    assert!(s.poll_cycle(4001)[2].is_none());
}

#[test]
fn bounded_expiry_advances_despite_control_emission_and_does_not_fail_core() {
    let mut s = ready();
    request(&mut s);
    let first = s.poll_cycle(0)[2].take().unwrap();
    for now in [2000, 4000, 6000] {
        s.observe(Observation {
            enabled: false,
            network: None,
        });
        let [core, _, profile] = s.poll_cycle(now);
        let core = core.unwrap();
        s.receive(&ack(&core));
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
            assert_eq!(profile.unwrap(), first);
        } else {
            assert!(profile.is_none());
        }
    }
    assert_eq!(s.profile_report_diagnostics().error, -1);
    assert!(s.settled());
    assert!(!s.failed());
}

#[test]
fn rejection_and_unsupported_lifecycle_block_only_optional_reporting() {
    for malformed in [false, true] {
        let mut s = ready();
        request(&mut s);
        let p = s.poll_cycle(0)[2].take().unwrap();
        if malformed {
            s.receive(&select(1, 0));
            assert_eq!(s.profile_report_diagnostics().error, -4);
        } else {
            let mut response = ack(&p);
            response[10] = 1;
            response[12] = 5;
            s.receive(&response);
            assert_eq!(s.profile_report_diagnostics().error, 5);
        }
        assert!(s.poll_cycle(2000)[2].is_none());
        assert!(!s.failed());
        assert!(s.settled());
    }
}

#[test]
fn unbound_sessions_cannot_create_profile_reports() {
    let mut unbound = Session::new(1, 7).unwrap();
    unbound.observe(connected());
    request(&mut unbound);
    assert!(unbound.poll_cycle(0)[2].is_none());
}
