// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! Private ownership/freshness tokens for deferred modem output. No Debug: wire
//! buffers and UP snapshots may contain addresses or opaque caller cookies.
use crate::{
    engine::{Network, Peer, Request},
    protocol::Activation,
};
use std::{net::IpAddr, ops::Deref};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct SidStamp {
    pub id: u8,
    pub incarnation: u64,
}

pub(crate) struct UpSnapshot {
    pub sid: SidStamp,
    pub request: Request,
    pub address: IpAddr,
    pub network: Network,
    pub cookie: Option<u32>,
    pub sequence: u16,
    pub force_initial: bool,
}
pub(crate) enum Fence {
    Reply(Option<SidStamp>),
    Up(UpSnapshot),
    Terminal(SidStamp),
}
pub struct Packet {
    pub(crate) fence: Fence,
    pub(crate) bytes: Vec<u8>,
    pub(crate) unadmitted_reply: bool,
}
impl Deref for Packet {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.bytes
    }
}
impl Packet {
    pub(crate) fn reply(bytes: Vec<u8>, sid: Option<SidStamp>, unadmitted_reply: bool) -> Self {
        Self {
            fence: Fence::Reply(sid),
            bytes,
            unadmitted_reply,
        }
    }
    pub(crate) fn up(bytes: Vec<u8>, snapshot: UpSnapshot) -> Self {
        Self {
            fence: Fence::Up(snapshot),
            bytes,
            unadmitted_reply: false,
        }
    }
    pub(crate) fn terminal(bytes: Vec<u8>, sid: SidStamp) -> Self {
        Self {
            fence: Fence::Terminal(sid),
            bytes,
            unadmitted_reply: false,
        }
    }
    pub fn wire_size_bound(&self) -> usize {
        crate::protocol::MAX_DATAGRAM
    }
    /// Only a response created before admission may take the one-shot path.
    /// Losing an admitted peer never converts its pending UP into such a reply.
    pub fn is_unadmitted_reply(&self) -> bool {
        self.unadmitted_reply
    }
}

/// Owned by the submission queue. Finish exactly once on send, discard or purge.
/// Rejected ownership must also be finished. This value deliberately has no Clone.
pub struct Retained {
    pub(crate) peer: Peer,
    pub(crate) peer_epoch: Option<u64>,
    pub(crate) packet: Packet,
    pub(crate) pinned: Option<SidStamp>,
}
impl Retained {
    pub fn peer(&self) -> Peer {
        self.peer
    }
    pub fn wire_size_bound(&self) -> usize {
        self.packet.wire_size_bound()
    }
}
#[derive(Clone, Copy)]
pub enum Disposition {
    Submitted,
    Obsolete,
    PeerLost,
}

#[derive(Clone, Copy, Default)]
pub(crate) struct SidLife {
    pub incarnation: u64,
    pub owner: Option<Peer>,
    pub pins: u16,
}
#[derive(Clone, Copy)]
pub(crate) struct PeerLife {
    pub sequence: u16,
    pub epoch: u64,
}

// Activation is used by the engine's prepare path; keep construction there so
// an adapter cannot manufacture a different request or substitute an address.
pub(crate) fn up_wire(
    sequence: u16,
    id: u8,
    activation: &Activation,
    address: IpAddr,
    change: bool,
) -> Vec<u8> {
    crate::protocol::indication(sequence, id, activation, Some(address), change)
}
