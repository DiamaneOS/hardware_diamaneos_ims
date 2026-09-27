// SPDX-License-Identifier: Apache-2.0
use diamaneos_wlan_reporting::{session::Observation, Connected};
use diamaneos_wlan_runtime::Observations;
fn network() -> Observation {
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
#[test]
fn stale_epoch_sequence_and_death_cannot_replace_current_observation() {
    let mut s = Observations::default();
    assert!(s.current(0).is_none());
    let a = s.register().unwrap();
    assert!(s.update(a, 1, 100, network()));
    assert!(!s.update(a, 1, 101, network()));
    assert!(!s.update(a, 2, 99, network()));
    let b = s.register().unwrap();
    assert_ne!(a, b);
    assert!(!s.update(a, 2, 102, network()));
    assert!(s.update(b, 1, 103, network()));
    s.lost(a);
    assert!(s.current(104).unwrap().network.is_some());
    s.lost(b);
    assert!(s.current(104).unwrap().network.is_none());
    assert!(!s.update(b, 2, 105, network()));
}
#[test]
fn lease_expiry_withdraws_but_does_not_invent_wifi_switch_state() {
    let mut s = Observations::default();
    let e = s.register().unwrap();
    s.update(e, 1, 100, network());
    assert!(s.current(30_099).unwrap().network.is_some());
    let expired = s.current(30_100).unwrap();
    assert!(expired.enabled);
    assert!(expired.network.is_none());
    assert!(!s.update(e, 2, 30_101, network()));
    let e = s.register().unwrap();
    assert!(s.current(30_101).unwrap().network.is_none());
    assert!(s.update(e, 1, 30_102, network()));
    assert!(s.current(30_103).unwrap().network.is_some());
}
