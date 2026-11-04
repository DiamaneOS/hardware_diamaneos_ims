// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! Another process: the IMS broker's Binder reports to imsdcmd
//! (daemon/src/broker_report.rs). Checks that only bounded, valid values
//! reach the engine.
#![no_main]

use arbitrary::Arbitrary;
use diamaneos_ims_dcm::protocol::PdnType;
use diamaneos_ims_runtime::broker_report::{network, request};
use libfuzzer_sys::fuzz_target;

#[derive(Arbitrary, Debug)]
struct Input {
    slot: i32,
    kind: u8,
    serial: i32,
    handle: i64,
    ipv4: Vec<u8>,
    ipv6: Vec<u8>,
    mtu: i32,
}

fuzz_target!(|i: Input| {
    let kind = match i.kind % 3 {
        0 => Some(PdnType::Ims),
        1 => Some(PdnType::Emergency),
        _ => None,
    };
    if let Some(r) = request(i.slot, kind, i.serial) {
        assert!((-1..=2).contains(&r.key.slot) && r.serial > 0);
        assert_eq!(Some(r.key.kind), kind);
    }
    if let Some(n) = network(i.handle, &i.ipv4, &i.ipv6, i.mtu) {
        assert!(n.valid());
        assert_eq!(n.handle, i.handle as u64);
        assert_ne!(n.handle, 0);
        assert!(i.mtu >= 0 && n.mtu <= 65535);
        assert_eq!(n.v4.map(|a| a.octets().to_vec()).unwrap_or_default(), i.ipv4);
        assert_eq!(n.v6.map(|a| a.octets().to_vec()).unwrap_or_default(), i.ipv6);
    }
});
