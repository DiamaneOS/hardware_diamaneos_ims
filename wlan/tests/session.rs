// SPDX-License-Identifier: Apache-2.0
use diamaneos_wlan_reporting::{
    session::{Observation, Session},
    Connected, BIND_SUBSCRIPTION, DATA_SETTINGS, DEFAULT_PROFILE_STATUS, INDICATION_REGISTRATION,
    WLAN_STATUS,
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
    for id in [
        BIND_SUBSCRIPTION,
        INDICATION_REGISTRATION,
        DEFAULT_PROFILE_STATUS,
        WLAN_STATUS,
        DATA_SETTINGS,
        DEFAULT_PROFILE_STATUS,
        WLAN_STATUS,
    ] {
        let p = s.poll(0).unwrap();
        assert_eq!(u16::from(p[3]), id);
        s.receive(&ack(&p));
    }
    assert!(s.poll(0).is_none());
}
#[test]
fn bind_clear_and_ack_before_announcing_available() {
    assert!(Session::new(0, 1).is_none());
    assert!(Session::new(3, 1).is_none());
    assert!(Session::new(1, 0).is_none());
    let mut a = Session::new(1, 1).unwrap();
    let mut b = Session::new(2, 2).unwrap();
    a.observe(connected());
    b.observe(connected());
    let pa = a.poll(0).unwrap();
    let pb = b.poll(0).unwrap();
    assert_eq!(&pa[10..14], &1_u32.to_le_bytes());
    assert_eq!(&pb[10..14], &2_u32.to_le_bytes());
    a.receive(&ack(&pa));
    b.receive(&ack(&pb));
    let registration = a.poll(0).unwrap();
    assert_eq!(registration[3], 0x38);
    a.receive(&ack(&registration));
    let clear = a.poll(0).unwrap();
    assert_eq!(u16::from(clear[3]), DEFAULT_PROFILE_STATUS);
    assert_eq!(&clear[clear.len() - 4..], &1_u32.to_le_bytes());
    assert!(a.poll(1).is_none());
    a.receive(&ack(&clear));
    for id in [
        WLAN_STATUS,
        DATA_SETTINGS,
        DEFAULT_PROFILE_STATUS,
        WLAN_STATUS,
    ] {
        let p = a.poll(2).unwrap();
        assert_eq!(u16::from(p[3]), id);
        a.receive(&ack(&p));
    }
    assert!(a.poll(3).is_none());
    // The second subscription cannot inherit the first client's acknowledgements.
    let registration = b.poll(0).unwrap();
    assert_eq!(registration[3], 0x38);
    b.receive(&ack(&registration));
    assert_eq!(u16::from(b.poll(0).unwrap()[3]), DEFAULT_PROFILE_STATUS);
}
#[test]
fn disconnect_during_pending_up_does_not_retry_stale_up() {
    let mut s = Session::new(1, 1).unwrap();
    s.observe(connected());
    for _ in 0..5 {
        let p = s.poll(0).unwrap();
        s.receive(&ack(&p));
    }
    let up = s.poll(0).unwrap();
    s.observe(Observation {
        enabled: true,
        network: None,
    });
    let replacement = s.poll(2000).unwrap();
    assert_eq!(replacement[3], 0x43);
    assert_eq!(&replacement[replacement.len() - 4..], &[1, 0, 0, 0]);
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
    let mut s = Session::new(1, 1).unwrap();
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
    let mut s = Session::new(1, 1).unwrap();
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
    let mut s = Session::new(1, 1).unwrap();
    s.observe(connected());
    assert_eq!(s.diagnostics().stage, 0);
    for next_stage in [8, 7, 1, 2, 7, 3, 5] {
        let p = s.poll(0).unwrap();
        s.receive(&ack(&p));
        assert_eq!(s.diagnostics().stage, next_stage);
    }
    s.observe(Observation {
        enabled: false,
        network: None,
    });
    assert_eq!(s.diagnostics().stage, 7);
    for _ in 0..3 {
        let p = s.poll(1).unwrap();
        s.receive(&ack(&p));
    }
    assert_eq!(s.diagnostics().stage, 4);
    assert_eq!(s.diagnostics().error, 0);
}
#[test]
fn new_modem_session_rebinds_and_clears_before_replaying() {
    let mut old = Session::new(1, 1).unwrap();
    old.observe(connected());
    ready(&mut old);
    let mut restarted = Session::new(1, 2).unwrap();
    restarted.observe(connected());
    let bind = restarted.poll(0).unwrap();
    assert_eq!(bind[3], 0x27);
    restarted.receive(&ack(&bind));
    let registration = restarted.poll(1).unwrap();
    assert_eq!(registration[3], 0x38);
    restarted.receive(&ack(&registration));
    assert_eq!(
        u16::from(restarted.poll(2).unwrap()[3]),
        DEFAULT_PROFILE_STATUS
    );
}

#[test]
fn resolver_replacement_during_either_positive_ack_uses_only_latest_metadata() {
    for pending_message in [0x43, 0x20] {
        let mut s = Session::new(1, 1).unwrap();
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
        let mut stale = s.poll(2).unwrap();
        assert_eq!(stale[3], 0x43); // Cached switch: profile, then station.
        if pending_message == 0x20 {
            s.receive(&ack(&stale));
            assert!(!s.settled());
            stale = s.poll(2).unwrap();
        }
        assert_eq!(stale[3], pending_message);
        s.observe(old);
        let replacement = s.poll(2002).unwrap();
        assert_eq!(replacement[3], 0x43);
        assert_ne!(&stale[1..3], &replacement[1..3]);
        s.receive(&ack(&stale));
        assert!(!s.settled());
        s.receive(&ack(&replacement));
        assert!(!s.settled());
        let station = s.poll(2004).unwrap();
        assert_eq!(station[3], 0x20);
        // The final station report restores the current resolver, not stale .54.
        assert_eq!(&station[23..30], &[0x13, 4, 0, 53, 2, 0, 192]);
        s.receive(&ack(&station));
        assert!(s.settled());
    }
}

#[test]
fn default_route_change_is_reported_without_changing_link_identity() {
    let mut s = Session::new(1, 1).unwrap();
    let mut observation = connected();
    s.observe(observation.clone());
    ready(&mut s);
    observation.network = observation.network.map(|n| n.with_default_route(true));
    s.observe(observation.clone());
    assert!(!s.settled());
    let update = s.poll(2).unwrap();
    assert_eq!(update[3], 0x43); // No redundant administrative notification.
    assert_eq!(&update[update.len() - 4..], &[0, 0, 0, 0]);
    s.receive(&ack(&update));
    assert!(!s.settled());
    let station = s.poll(2).unwrap();
    assert_eq!(station[3], 0x20);
    assert_eq!(&station[station.len() - 4..], &[0x24, 1, 0, 1]);
    s.receive(&ack(&station));
    assert!(s.settled());
    s.observe(observation);
    assert!(s.poll(3).is_none());
}

#[test]
fn default_profile_requires_connected_validated_default_network() {
    for validated in [false, true] {
        for default_route in [false, true] {
            let mut s = Session::new(1, 1).unwrap();
            s.observe(Observation {
                enabled: true,
                network: Some(
                    Connected::new(
                        [2, 0, 0, 0, 0, 1],
                        Some("192.0.2.1".parse().unwrap()),
                        None,
                        validated,
                    )
                    .unwrap()
                    .with_default_route(default_route),
                ),
            });
            for id in [
                BIND_SUBSCRIPTION,
                INDICATION_REGISTRATION,
                DEFAULT_PROFILE_STATUS,
                WLAN_STATUS,
                DATA_SETTINGS,
            ] {
                let request = s.poll(0).unwrap();
                assert_eq!(u16::from(request[3]), id);
                if id == 0x43 {
                    // Startup reconciliation never reports a connected profile.
                    assert_eq!(&request[request.len() - 4..], &[1, 0, 0, 0]);
                }
                s.receive(&ack(&request));
            }
            let profile = s.poll(1).unwrap();
            assert_eq!(profile[3], 0x43);
            assert_eq!(
                &profile[profile.len() - 4..],
                &u32::from(!(validated && default_route)).to_le_bytes()
            );
            assert!(!s.settled());
            s.receive(&ack(&profile));
            assert!(!s.settled());
            let station = s.poll(2).unwrap();
            assert_eq!(station[3], 0x20);
            s.receive(&ack(&station));
            assert!(s.settled());
        }
    }
}

#[test]
fn loss_during_either_positive_ack_never_retries_stale_availability() {
    for pending_message in [0x43, 0x20] {
        let mut s = Session::new(1, 1).unwrap();
        let mut observation = connected();
        observation.network = observation.network.map(|n| n.with_default_route(true));
        s.observe(observation);
        for _ in 0..5 {
            let request = s.poll(0).unwrap();
            s.receive(&ack(&request));
        }
        let mut positive = s.poll(0).unwrap();
        assert_eq!(positive[3], 0x43);
        assert_eq!(&positive[positive.len() - 4..], &[0, 0, 0, 0]);
        if pending_message == 0x20 {
            s.receive(&ack(&positive));
            assert!(!s.settled());
            positive = s.poll(0).unwrap();
        }
        assert_eq!(positive[3], pending_message);
        s.observe(Observation {
            enabled: false,
            network: None,
        });
        let replacement = s.poll(2000).unwrap();
        assert_eq!(replacement[3], 0x43);
        assert_ne!(&positive[1..3], &replacement[1..3]);
        s.receive(&ack(&positive));
        assert!(s.poll(2001).is_none());
        s.receive(&ack(&replacement));
        let down = s.poll(2002).unwrap();
        assert_eq!(down[3], 0x20);
        s.receive(&ack(&down));
        let switch = s.poll(2003).unwrap();
        assert_eq!(switch[3], 0x34);
        assert_eq!(&switch[switch.len() - 1..], &[0]);
        s.receive(&ack(&switch));
        assert!(s.settled());
        assert_eq!(s.diagnostics().stage, 4);
    }
}

#[test]
fn rejection_of_either_positive_step_never_settles_available() {
    for rejected_message in [0x43, 0x20] {
        let mut s = Session::new(1, 1).unwrap();
        s.observe(connected());
        for _ in 0..5 {
            let request = s.poll(0).unwrap();
            s.receive(&ack(&request));
        }
        let mut request = s.poll(1).unwrap();
        assert_eq!(request[3], 0x43);
        if rejected_message == 0x20 {
            s.receive(&ack(&request));
            assert!(!s.settled());
            request = s.poll(1).unwrap();
        }
        assert_eq!(request[3], rejected_message);
        let mut rejected = ack(&request);
        rejected[10] = 1;
        rejected[12] = 5;
        s.receive(&rejected);
        assert!(s.failed());
        assert!(!s.settled());
        assert_eq!(s.diagnostics().operation, rejected_message as u16);
        assert_eq!(s.diagnostics().error, 5);
        assert!(s.poll(10_000).is_none());
    }
}

#[test]
fn withdrawal_revokes_profile_before_station_and_administrative_switch() {
    for subscription in [1, 2] {
        let mut s = Session::new(subscription, 1).unwrap();
        s.observe(connected());
        ready(&mut s);
        s.observe(Observation {
            enabled: false,
            network: None,
        });
        for (id, stage) in [(0x43, 7), (0x20, 3), (0x34, 2)] {
            let packet = s.poll(1).unwrap();
            assert_eq!(packet[3], id);
            assert_eq!(s.diagnostics().stage, stage);
            assert!(!s.settled());
            if id == 0x43 {
                assert_eq!(&packet[packet.len() - 4..], &[1, 0, 0, 0]);
            }
            s.receive(&ack(&packet));
        }
        assert!(s.settled());
        assert_eq!(s.diagnostics().stage, 4);
    }
}

#[test]
fn rejected_withdrawal_profile_stops_before_station_or_switch_notification() {
    let mut s = Session::new(1, 1).unwrap();
    s.observe(connected());
    ready(&mut s);
    s.observe(Observation {
        enabled: false,
        network: None,
    });
    let packet = s.poll(1).unwrap();
    assert_eq!(packet[3], 0x43);
    let mut rejected = ack(&packet);
    rejected[10] = 1;
    rejected[12] = 5;
    s.receive(&rejected);
    assert!(s.failed());
    assert!(!s.settled());
    assert!(s.poll(10000).is_none());
}

#[test]
fn connection_loss_preserves_the_acknowledged_administrative_switch() {
    for subscription in [1, 2] {
        let mut s = Session::new(subscription, 1).unwrap();
        s.observe(connected());
        ready(&mut s);
        s.observe(Observation {
            enabled: true,
            network: None,
        });
        for id in [0x43, 0x20] {
            let packet = s.poll(1).unwrap();
            assert_eq!(packet[3], id);
            s.receive(&ack(&packet));
        }
        assert!(s.poll(2).is_none());
        assert!(s.settled());
        s.observe(connected());
        for id in [0x43, 0x20] {
            let packet = s.poll(3).unwrap();
            assert_eq!(packet[3], id);
            s.receive(&ack(&packet));
        }
        assert!(s.settled());
    }
}

#[test]
fn networkless_adapter_enable_is_reported_once_before_connection() {
    let mut s = Session::new(1, 1).unwrap();
    s.observe(connected());
    ready(&mut s);
    for enabled in [false, true] {
        s.observe(Observation {
            enabled,
            network: None,
        });
        for id in [0x43, 0x20, 0x34] {
            let packet = s.poll(1).unwrap();
            assert_eq!(packet[3], id);
            if id == 0x34 {
                assert_eq!(*packet.last().unwrap(), u8::from(enabled));
            }
            s.receive(&ack(&packet));
        }
        assert!(s.settled());
    }
    s.observe(connected());
    for id in [0x43, 0x20] {
        let packet = s.poll(2).unwrap();
        assert_eq!(packet[3], id);
        s.receive(&ack(&packet));
    }
    assert!(s.settled());
}

#[test]
fn superseded_unacknowledged_switch_is_reconciled_with_a_new_transaction() {
    let mut s = Session::new(1, 1).unwrap();
    s.observe(connected());
    ready(&mut s);
    s.observe(Observation {
        enabled: false,
        network: None,
    });
    for id in [0x43, 0x20] {
        let packet = s.poll(1).unwrap();
        assert_eq!(packet[3], id);
        s.receive(&ack(&packet));
    }
    let disable = s.poll(1).unwrap();
    assert_eq!(disable[3], 0x34);
    s.observe(connected());
    // The unacknowledged disable may have applied. Do not trust the old "on".
    let enable = s.poll(2001).unwrap();
    assert_eq!(enable[3], 0x34);
    assert_eq!(*enable.last().unwrap(), 1);
    assert_ne!(&enable[1..3], &disable[1..3]);
    s.receive(&ack(&disable));
    assert!(s.poll(2002).is_none());
    s.receive(&ack(&enable));
    for id in [0x43, 0x20] {
        let packet = s.poll(2003).unwrap();
        assert_eq!(packet[3], id);
        s.receive(&ack(&packet));
    }
    assert!(s.settled());
}

#[test]
fn startup_revokes_profile_before_station_and_waits_for_authenticated_observation() {
    for subscription in [1, 2] {
        let mut session = Session::new(subscription, u64::from(subscription)).unwrap();
        for message in [
            BIND_SUBSCRIPTION,
            INDICATION_REGISTRATION,
            DEFAULT_PROFILE_STATUS,
            WLAN_STATUS,
        ] {
            let packet = session.poll(0).unwrap();
            assert_eq!(u16::from(packet[3]), message);
            session.receive(&ack(&packet));
        }
        assert_eq!(session.diagnostics().stage, 9);
        assert!(!session.settled());
        assert!(session.poll(10_000).is_none());
        session.observe(Observation {
            enabled: true,
            network: None,
        });
        for message in [0x43, 0x20, 0x34] {
            let packet = session.poll(10_001).unwrap();
            assert_eq!(u16::from(packet[3]), message);
            session.receive(&ack(&packet));
        }
        assert!(session.settled());
    }
}
