// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! Linux UAPI QRTR control packets, little-endian; no socket operations.
use crate::engine::Peer;
pub const SERVICE: u32 = 0x302;
pub const INSTANCE: u32 = 1; // IDL major 1, instance zero.
pub const CTRL_PORT: u32 = 0xffff_fffe;
pub const NEW_SERVER: u32 = 4;
pub const DEL_SERVER: u32 = 5;
pub const DEL_CLIENT: u32 = 6;
pub const BYE: u32 = 3;
pub const NEW_LOOKUP: u32 = 10;
pub const DEL_LOOKUP: u32 = 11;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Control {
    pub command: u32,
    pub words: [u32; 4],
}
impl Control {
    pub fn decode(b: &[u8]) -> Option<Self> {
        if b.len() != 20 {
            return None;
        }
        let mut w = [0; 5];
        for (i, c) in b.chunks_exact(4).enumerate() {
            w[i] = u32::from_le_bytes(c.try_into().ok()?);
        }
        Some(Self {
            command: w[0],
            words: [w[1], w[2], w[3], w[4]],
        })
    }
    pub fn encode(self) -> [u8; 20] {
        let mut b = [0; 20];
        for (i, w) in std::iter::once(self.command).chain(self.words).enumerate() {
            b[i * 4..i * 4 + 4].copy_from_slice(&w.to_le_bytes());
        }
        b
    }
    pub fn lookup(command: u32) -> Self {
        Self {
            command,
            words: [SERVICE, INSTANCE, 0, 0],
        }
    }
    pub fn server(command: u32, peer: Peer) -> Self {
        Self {
            command,
            words: [SERVICE, INSTANCE, peer.node, peer.port],
        }
    }
    pub fn deleted_client(self) -> Option<Peer> {
        (self.command == DEL_CLIENT).then_some(Peer {
            node: self.words[0],
            port: self.words[1],
        })
    }
    pub fn lookup_complete(self) -> bool {
        self.command == NEW_SERVER && self.words == [0; 4]
    }
    /// Another publisher on this node. The daemon's own record, including a
    /// stale one from before a rebind of the fixed port, is not a conflict.
    pub fn conflicts_with(self, local: Peer) -> bool {
        self.conflicting_server() && self.words[2] == local.node && self.words[3] != local.port
    }
    /// A record on another node cannot take the local reserved role; it is
    /// counted, not treated as a conflict.
    pub fn remote_server(self, local: Peer) -> bool {
        self.conflicting_server() && self.words[2] != local.node
    }
    pub fn conflicting_server(self) -> bool {
        self.command == NEW_SERVER
            && self.words[0] == SERVICE
            && self.words[1] == INSTANCE
            && self.words[3] != 0
    }
}
