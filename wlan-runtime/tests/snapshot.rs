// SPDX-License-Identifier: Apache-2.0
// Observer snapshot checks. Made-up BSSIDs and documentation addresses only.
use diamaneos_wlan_runtime::{observation, SnapshotFields};

fn connected() -> SnapshotFields {
    SnapshotFields {
        enabled: true,
        connected: true,
        validated: true,
        default_route: true,
        bssid: [2, 0, 0, 0, 0, 1],
        has_ipv4: true,
        ipv4: [192, 0, 2, 10],
        ipv6_prefix: 64,
        dns4_count: 1,
        dns4: [192, 0, 2, 53, 0, 0, 0, 0],
        wifi_calling_slots: 1,
        ..Default::default()
    }
}

#[test]
fn connected_snapshot_becomes_one_observation() {
    let (o, slots) = observation(&connected()).unwrap();
    assert!(o.enabled && o.network.is_some());
    assert_eq!(slots, 1);
    let mut s = connected();
    s.has_ipv6 = true;
    s.ipv6 = "2001:db8::10".parse::<std::net::Ipv6Addr>().unwrap().octets();
    s.dns6_count = 2;
    s.dns6[..16].copy_from_slice(&"2001:db8::53".parse::<std::net::Ipv6Addr>().unwrap().octets());
    s.dns6[16..].copy_from_slice(&"2001:db8::54".parse::<std::net::Ipv6Addr>().unwrap().octets());
    assert!(observation(&s).unwrap().0.network.is_some());
}

#[test]
fn disconnected_snapshot_carries_no_network_and_ignores_link_fields() {
    let mut s = connected();
    s.connected = false;
    s.ipv6_prefix = 999;
    s.dns4_count = 7;
    let (o, _) = observation(&s).unwrap();
    assert!(o.enabled && o.network.is_none());
    s.enabled = false;
    assert!(!observation(&s).unwrap().0.enabled);
}

#[test]
fn out_of_range_fields_are_rejected() {
    let cases: [fn(&mut SnapshotFields); 11] = [
        |s| s.wifi_calling_slots = 4,
        |s| s.wifi_calling_slots = -1,
        |s| s.enabled = false,
        |s| s.ipv6_prefix = 129,
        |s| s.ipv6_prefix = -1,
        |s| s.dns4_count = 3,
        |s| s.dns6_count = -1,
        // Bytes beyond the counted resolvers must be zero.
        |s| s.dns4[7] = 1,
        |s| s.dns6[0] = 1,
        |s| s.has_ipv4 = false,
        |s| s.bssid = [0; 6],
    ];
    for change in cases {
        let mut s = connected();
        change(&mut s);
        assert!(observation(&s).is_none());
    }
}
