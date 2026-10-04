// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! Bounded per-peer FIFO ownership. The engine supplies and revalidates tokens;
//! this queue never interprets or logs payloads and never silently loses tokens.
use diamaneos_ims_dcm::{
    engine::{Peer, MAX_SESSIONS},
    protocol::MAX_DATAGRAM,
};
use std::collections::{BTreeMap, VecDeque};

// Four admitted modem clients. Allow a pending UP plus deactivate reply,
// broker-release barrier and terminal result for every owned session.
const MAX_PEERS: usize = 4;
const MAX_ITEMS_PER_PEER: usize = 4 * MAX_SESSIONS;
const MAX_BYTES_PER_PEER: usize = MAX_ITEMS_PER_PEER * MAX_DATAGRAM;
const SUBMISSION_DEADLINE_MS: u64 = 2_000;

struct Entry<T> {
    token: T,
    bytes: usize,
    deadline_ms: u64,
}
struct PeerQueue<T> {
    items: VecDeque<Entry<T>>,
    bytes: usize,
}

/// The caller receives rejected ownership back and must finish/invalidate it.
pub struct Rejected<T> {
    pub token: T,
}
pub enum Attempt {
    Submitted,
    Obsolete,
    Backpressured,
    PeerLost,
}
pub enum Finished<T> {
    Submitted(T),
    Obsolete(T),
    PeerLost(Peer, Vec<T>),
}

pub struct Outbox<T> {
    peers: BTreeMap<Peer, PeerQueue<T>>,
    last_peer: Option<Peer>,
}
impl<T> Default for Outbox<T> {
    fn default() -> Self {
        Self {
            peers: BTreeMap::new(),
            last_peer: None,
        }
    }
}
impl<T> Outbox<T> {
    pub fn enqueue(
        &mut self,
        peer: Peer,
        token: T,
        bytes: usize,
        now_ms: u64,
    ) -> Result<(), Rejected<T>> {
        if bytes > MAX_DATAGRAM
            || (!self.peers.contains_key(&peer) && self.peers.len() == MAX_PEERS)
        {
            return Err(Rejected { token });
        }
        let queue = self.peers.entry(peer).or_insert_with(|| PeerQueue {
            items: VecDeque::new(),
            bytes: 0,
        });
        if queue.items.len() == MAX_ITEMS_PER_PEER || bytes > MAX_BYTES_PER_PEER - queue.bytes {
            return Err(Rejected { token });
        }
        queue.bytes += bytes;
        queue.items.push_back(Entry {
            token,
            bytes,
            deadline_ms: now_ms.saturating_add(SUBMISSION_DEADLINE_MS),
        });
        Ok(())
    }

    /// One fair head per call. Backpressure retains the exact head and original
    /// deadline; it cannot reorder this peer or starve the next peer's turn.
    pub fn attempt(
        &mut self,
        now_ms: u64,
        mut eligible: impl FnMut(Peer, &T) -> bool,
        mut submit: impl FnMut(Peer, &T) -> Attempt,
    ) -> Option<Finished<T>> {
        let peer = self
            .peers
            .keys()
            .copied()
            .find(|p| self.last_peer.is_none_or(|last| *p > last))
            .or_else(|| self.peers.keys().next().copied())?;
        self.last_peer = Some(peer);
        let head = self.peers[&peer]
            .items
            .front()
            .expect("nonempty peer queue");
        // A superseded UP does not poison a newer valid successor merely by
        // reaching its old deadline. Only still-eligible work can expire a peer.
        let obsolete = !eligible(peer, &head.token);
        if !obsolete && now_ms >= head.deadline_ms {
            return Some(Finished::PeerLost(peer, self.purge(peer)));
        }
        let outcome = if obsolete {
            Attempt::Obsolete
        } else {
            submit(peer, &head.token)
        };
        match outcome {
            Attempt::Backpressured => None,
            Attempt::PeerLost => Some(Finished::PeerLost(peer, self.purge(peer))),
            state @ (Attempt::Submitted | Attempt::Obsolete) => {
                let queue = self.peers.get_mut(&peer).expect("selected queue");
                let entry = queue.items.pop_front().expect("selected head");
                queue.bytes -= entry.bytes;
                if queue.items.is_empty() {
                    self.peers.remove(&peer);
                }
                Some(match state {
                    Attempt::Submitted => Finished::Submitted(entry.token),
                    _ => Finished::Obsolete(entry.token),
                })
            }
        }
    }
    pub fn purge(&mut self, peer: Peer) -> Vec<T> {
        self.peers
            .remove(&peer)
            .map(|queue| queue.items.into_iter().map(|e| e.token).collect())
            .unwrap_or_default()
    }
    /// Remove superseded output anywhere in this peer's FIFO without reordering
    /// valid work. Return ownership to the engine before retrying admission.
    pub fn discard_obsolete(&mut self, peer: Peer, mut eligible: impl FnMut(&T) -> bool) -> Vec<T> {
        let Some(queue) = self.peers.get_mut(&peer) else {
            return vec![];
        };
        let mut obsolete = vec![];
        let mut kept = VecDeque::new();
        while let Some(entry) = queue.items.pop_front() {
            if eligible(&entry.token) {
                kept.push_back(entry);
            } else {
                queue.bytes -= entry.bytes;
                obsolete.push(entry.token);
            }
        }
        queue.items = kept;
        if queue.items.is_empty() {
            self.peers.remove(&peer);
        }
        obsolete
    }
    pub fn peers(&self) -> Vec<Peer> {
        self.peers.keys().copied().collect()
    }
    pub fn drain(&mut self) -> Vec<T> {
        std::mem::take(&mut self.peers)
            .into_values()
            .flat_map(|queue| queue.items.into_iter().map(|e| e.token))
            .collect()
    }
    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }
}
