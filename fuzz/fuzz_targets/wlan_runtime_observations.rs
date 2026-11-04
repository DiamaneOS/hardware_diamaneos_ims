// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! Another process: the Wi-Fi observer's Binder calls into wlanreportd
//! (wlan-runtime/src/lib.rs). Snapshot checks, registration, sequenced
//! snapshots, death and the lease clock, then the per-SIM filter.
#![no_main]

use arbitrary::Arbitrary;
use diamaneos_wlan_reporting::session::Observation;
use diamaneos_wlan_reporting::Connected;
use diamaneos_wlan_runtime::{for_subscription, observation, Observations, SnapshotFields};
use libfuzzer_sys::fuzz_target;
use std::net::Ipv4Addr;

#[derive(Arbitrary, Debug)]
struct Snap {
    enabled: bool,
    connected: bool,
    validated: bool,
    default_route: bool,
    bssid: [u8; 6],
    has_ipv4: bool,
    ipv4: [u8; 4],
    has_ipv6: bool,
    ipv6: [u8; 16],
    ipv6_prefix: i32,
    dns4_count: i32,
    dns4: [u8; 8],
    dns6_count: i32,
    dns6: [u8; 32],
    wifi_calling_slots: i32,
}

#[derive(Arbitrary, Debug)]
enum Op {
    Register,
    Observe { generation: u8, sequence: u64, at: u32, snapshot: Snap },
    Update { generation: u8, sequence: u64, at: u32, enabled: bool, connected: bool },
    Lost(u8),
    Current { at: u32, slots: u32, subscription: u32 },
}

fuzz_target!(|ops: Vec<Op>| {
    let mut o = Observations::default();
    let mut latest = 0u64;
    for op in ops.into_iter().take(1024) {
        match op {
            Op::Register => {
                if let Some(g) = o.register() {
                    assert!(g > latest);
                    latest = g;
                    assert!(o.is_current(g));
                }
            }
            Op::Update { generation, sequence, at, enabled, connected } => {
                let network = connected.then(|| {
                    Connected::new([2, 0, 0, 0, 0, 1], Some(Ipv4Addr::new(192, 0, 2, 7)), None, true)
                        .unwrap()
                });
                let accepted = o.update(
                    u64::from(generation),
                    sequence,
                    u64::from(at),
                    Observation { enabled, network },
                );
                if accepted {
                    assert!(o.is_current(u64::from(generation)));
                }
            }
            Op::Observe { generation, sequence, at, snapshot: n } => {
                let fields = SnapshotFields {
                    enabled: n.enabled,
                    connected: n.connected,
                    validated: n.validated,
                    default_route: n.default_route,
                    bssid: n.bssid,
                    has_ipv4: n.has_ipv4,
                    ipv4: n.ipv4,
                    has_ipv6: n.has_ipv6,
                    ipv6: n.ipv6,
                    ipv6_prefix: n.ipv6_prefix,
                    dns4_count: n.dns4_count,
                    dns4: n.dns4,
                    dns6_count: n.dns6_count,
                    dns6: n.dns6,
                    wifi_calling_slots: n.wifi_calling_slots,
                };
                if let Some((value, slots)) = observation(&fields) {
                    assert!(slots <= 3);
                    assert_eq!(value.network.is_some(), n.connected);
                    if n.connected {
                        assert!(n.enabled && (0..=128).contains(&n.ipv6_prefix));
                        assert!(n.has_ipv4 || n.has_ipv6);
                    }
                    let _ = o.update(u64::from(generation), sequence, u64::from(at), value);
                }
            }
            Op::Lost(g) => {
                o.lost(u64::from(g));
                if u64::from(g) == latest {
                    assert!(!o.is_current(latest));
                }
            }
            Op::Current { at, slots, subscription } => {
                if let Some(current) = o.current(u64::from(at)) {
                    let seen = for_subscription(&current, slots, subscription);
                    assert_eq!(seen.enabled, current.enabled);
                    if seen.network.is_some() {
                        assert!(current.network.is_some());
                        assert!((1..=32).contains(&subscription));
                        assert!(slots & (1 << (subscription - 1)) != 0);
                    }
                }
            }
        }
    }
});
