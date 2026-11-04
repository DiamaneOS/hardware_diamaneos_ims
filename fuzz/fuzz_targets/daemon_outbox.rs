// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! imsdcmd's bounded per-peer submission queue (daemon/src/outbox.rs) under
//! arbitrary peers, sizes, send results and clocks. Checks that no token is
//! lost or duplicated and that memory stays within the documented bounds.
#![no_main]

use arbitrary::Arbitrary;
use diamaneos_ims_dcm::engine::{Peer, MAX_SESSIONS};
use diamaneos_ims_dcm::protocol::MAX_DATAGRAM;
use diamaneos_ims_runtime::outbox::{Attempt, Finished, Outbox};
use libfuzzer_sys::fuzz_target;
use std::collections::BTreeSet;

#[derive(Arbitrary, Debug)]
enum Op {
    Enqueue { peer: u8, bytes: u16, terminal: bool, eligible: bool },
    Attempt { result: u8 },
    Purge(u8),
    Discard(u8),
    Advance(u16),
    Drain,
}

fn peer(p: u8) -> Peer {
    Peer { node: 3, port: u32::from(p % 6) }
}

fuzz_target!(|ops: Vec<Op>| {
    let mut outbox: Outbox<(u64, bool)> = Outbox::default();
    let mut live = BTreeSet::new();
    let mut next = 0u64;
    let mut now = 0u64;
    let take = |live: &mut BTreeSet<u64>, id: u64| assert!(live.remove(&id), "token {id} twice");
    for op in ops.into_iter().take(2048) {
        match op {
            Op::Enqueue { peer: p, bytes, terminal, eligible } => {
                let id = next;
                next += 1;
                let size = usize::from(bytes) % (MAX_DATAGRAM + 8);
                let result = if terminal {
                    outbox.enqueue_terminal(peer(p), (id, eligible), size, now)
                } else {
                    outbox.enqueue(peer(p), (id, eligible), size, now)
                };
                match result {
                    Ok(()) => {
                        assert!(size <= MAX_DATAGRAM);
                        live.insert(id);
                    }
                    Err(rejected) => assert_eq!(rejected.token.0, id),
                }
            }
            Op::Attempt { result } => {
                let outcome = outbox.attempt(
                    now,
                    |_, t| t.1,
                    |_, _| match result % 4 {
                        0 => Attempt::Submitted,
                        1 => Attempt::Obsolete,
                        2 => Attempt::Backpressured,
                        _ => Attempt::PeerLost,
                    },
                );
                match outcome {
                    Some(Finished::Submitted(t)) | Some(Finished::Obsolete(t)) => {
                        take(&mut live, t.0)
                    }
                    Some(Finished::PeerLost(_, ts)) | Some(Finished::Expired(_, ts)) => {
                        for t in ts {
                            take(&mut live, t.0);
                        }
                    }
                    None => {}
                }
            }
            Op::Purge(p) => {
                for t in outbox.purge(peer(p)) {
                    take(&mut live, t.0);
                }
            }
            Op::Discard(p) => {
                for t in outbox.discard_obsolete(peer(p), |t| t.1) {
                    assert!(!t.1);
                    take(&mut live, t.0);
                }
            }
            Op::Advance(ms) => now = now.saturating_add(u64::from(ms)),
            Op::Drain => {
                for t in outbox.drain() {
                    take(&mut live, t.0);
                }
            }
        }
        // Four peers, each at most 4 * MAX_SESSIONS items plus the terminal reserve.
        assert!(outbox.peer_count() <= 4);
        assert!(live.len() <= 4 * (5 * MAX_SESSIONS));
    }
    for t in outbox.drain() {
        take(&mut live, t.0);
    }
    assert!(live.is_empty(), "tokens lost by the queue");
});
