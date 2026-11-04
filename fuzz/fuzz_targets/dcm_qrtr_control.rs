// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! Kernel input: QRTR control packets from the local name service
//! (dcm/src/qrtr.rs), as read by imsdcmd, imeiprovd and wlanreportd.
//! Input: 16 bytes of local and sender addresses, then the packet.
#![no_main]

use diamaneos_ims_dcm::engine::Peer;
use diamaneos_ims_dcm::qrtr::{lookup_input, Control, Lookup, CTRL_PORT, DEL_CLIENT, NEW_SERVER};
use libfuzzer_sys::fuzz_target;

fn word(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes(b[i * 4..i * 4 + 4].try_into().unwrap())
}

fuzz_target!(|data: &[u8]| {
    if data.len() < 16 {
        return;
    }
    let local = Peer { node: word(data, 0), port: word(data, 1) };
    let mut from = Peer { node: word(data, 2), port: word(data, 3) };
    // Bias half the inputs to the control port of the local node.
    if from.port & 1 == 0 {
        from = Peer { node: local.node, port: CTRL_PORT };
    }
    let bytes = &data[16..];

    let control = Control::decode(bytes);
    assert_eq!(control.is_some(), bytes.len() == 20);
    if let Some(c) = control {
        assert_eq!(&c.encode()[..], bytes);
        assert_eq!(c.deleted_client().is_some(), c.command == DEL_CLIENT);
        if c.conflicts_with(local) || c.remote_server(local) {
            assert!(c.conflicting_server());
        }
        assert!(!(c.conflicts_with(local) && c.remote_server(local)));
        if c.lookup_complete() {
            assert_eq!(c.command, NEW_SERVER);
        }
    }
    match lookup_input(local, from, bytes) {
        Lookup::Conflict | Lookup::Complete => {
            assert_eq!(from, Peer { node: local.node, port: CTRL_PORT });
            assert!(control.is_some());
        }
        Lookup::Ignore => assert!(from.node == local.node && from.port != CTRL_PORT),
        Lookup::Keep => {}
    }
});
