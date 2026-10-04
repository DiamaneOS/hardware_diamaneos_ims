// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! Single-owner state machine. Adapters execute effects in order. No I/O or timers.
use crate::outgoing::{
    self, Disposition, Fence, Packet, PeerLife, Retained, SidLife, SidStamp, UpSnapshot,
};
use crate::protocol::{self as p, Activation, Family, Frame, Kind, PdnType};
use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

// The authenticated stock DCM contract allocates PDP IDs in this inclusive range.
const FIRST_SESSION_ID: u8 = 20;
const LAST_SESSION_ID: u8 = 98;
pub const MAX_SESSIONS: usize = (LAST_SESSION_ID - FIRST_SESSION_ID + 1) as usize;
// Downstream containment budgets: keep normal IMS admission from consuming the
// entire ID pool. Reservation does not bypass the independent four-client limit.
const EMERGENCY_RESERVE: usize = 8;
const MAX_CLIENTS: usize = 4;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Peer {
    pub node: u32,
    pub port: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Key {
    pub slot: i32,
    pub kind: PdnType,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Request {
    pub key: Key,
    pub serial: i32,
}
// No Debug implementation for anything carrying address or protocol data.
#[derive(Clone, PartialEq, Eq)]
pub struct Network {
    pub handle: u64,
    pub v4: Option<Ipv4Addr>,
    pub v6: Option<Ipv6Addr>,
    pub mtu: u32,
}
impl Network {
    pub fn valid(&self) -> bool {
        self.handle != 0
            && self.mtu <= 65535
            && self.v4.is_none_or(|a| {
                !(a.is_unspecified()
                    || a.is_loopback()
                    || a.is_multicast()
                    || a.is_link_local()
                    || a.is_broadcast())
            })
            && self.v6.is_none_or(|a| {
                !(a.is_unspecified()
                    || a.is_loopback()
                    || a.is_multicast()
                    || a.is_unicast_link_local())
            })
    }
    fn address(&self, f: Family) -> Option<IpAddr> {
        match f {
            Family::V4 => self.v4.map(IpAddr::V4),
            Family::V6 => self.v6.map(IpAddr::V6),
        }
    }
}
pub enum Effect {
    Send(Peer, Packet),
    BringUp(Request),
    Release(Request),
    /// The peer's accepted deactivate reply precedes broker teardown; the
    /// adapter queues this barrier between that reply and the terminal result.
    ReleaseAfter {
        peer: Peer,
        request: Request,
    },
    ReadTime {
        peer: Peer,
        txn: u16,
        pdp: u8,
        sequence: u32,
    },
}
struct Session {
    peer: Peer,
    id: u8,
    activation: Activation,
    address: Option<IpAddr>,
    submitted_address: Option<IpAddr>,
    last_network: Option<Network>,
}
struct Group {
    request: Request,
    network: Option<Network>,
}
/// Bounded lifecycle metadata only: no addresses, APNs, handles or peer identities.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Diagnostics {
    pub broker_connected: bool,
    pub active_sessions: usize,
    pub active_groups: usize,
    pub sessions_by_slot: [usize; 4], // unspecified, then slots 0..2
    // Same slot indexing; counts only, never session or peer identifiers.
    // Requests include duplicates and admission failures; releases count
    // removed sessions, not successful bearer activations or shared releases.
    pub activation_requests_by_slot: [u64; 4],
    // Explicit PDN deactivation (0x21) and instance destruction (0x33).
    pub modem_releases_by_slot: [u64; 4],
    pub modem_instance_destructions_by_slot: [u64; 4],
    pub missing_family_releases_by_slot: [u64; 4],
    pub requests: u64,
    pub malformed: u64,
    pub last_request: u16,
    pub up_reports: u64,
    pub down_reports: u64,
    pub stale_reports: u64,
    pub broker_registrations: u64,
    pub broker_losses: u64,
    pub client_losses: u64,
    pub modem_losses: u64,
    pub publisher_conflicts: u64,
    pub publication_active: bool,
    // QRTR buffer or memory exhaustion; the affected work is retried, not failed.
    pub transport_exhaustion: u64,
    // Oneway calls a live broker did not receive (not broker deaths).
    pub broker_call_failures: u64,
    // Failed attempts to replace the reserved-port socket; each is retried.
    pub socket_rebind_failures: u64,
    // Records of this service on other nodes; they are not local conflicts.
    pub remote_publisher_records: u64,
}
/// Device configuration fixes the modem node and number of slots; no first-packet trust.
pub struct Engine {
    modem_node: u32,
    slots: u32,
    emergency_enabled: bool,
    broker: bool,
    serial: i32,
    sessions: Vec<Session>,
    groups: BTreeMap<Key, Group>,
    indications: BTreeMap<Peer, PeerLife>,
    sid_life: [SidLife; MAX_SESSIONS],
    next_peer_epoch: u64,
    diagnostics: Diagnostics,
}
impl Engine {
    pub fn new(modem_node: u32, slots: u32) -> Result<Self, &'static str> {
        if modem_node == u32::MAX || !(1..=3).contains(&slots) {
            return Err("invalid device configuration");
        }
        Ok(Self {
            modem_node,
            slots,
            emergency_enabled: true,
            broker: false,
            serial: 0,
            sessions: vec![],
            groups: BTreeMap::new(),
            indications: BTreeMap::new(),
            sid_life: [SidLife::default(); MAX_SESSIONS],
            next_peer_epoch: 1,
            diagnostics: Diagnostics::default(),
        })
    }
    pub fn publisher_conflict(&mut self) {
        self.diagnostics.publisher_conflicts =
            self.diagnostics.publisher_conflicts.saturating_add(1);
    }
    pub fn remote_publisher(&mut self) {
        self.diagnostics.remote_publisher_records =
            self.diagnostics.remote_publisher_records.saturating_add(1);
    }
    pub fn publication_active(&mut self, active: bool) {
        self.diagnostics.publication_active = active;
    }
    pub fn socket_rebind_failed(&mut self) {
        self.diagnostics.socket_rebind_failures =
            self.diagnostics.socket_rebind_failures.saturating_add(1);
    }
    pub fn transport_exhausted(&mut self) {
        self.diagnostics.transport_exhaustion =
            self.diagnostics.transport_exhaustion.saturating_add(1);
    }
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }
    pub fn diagnostics(&self) -> Diagnostics {
        let mut result = self.diagnostics;
        result.broker_connected = self.broker;
        result.active_sessions = self.sessions.len();
        result.active_groups = self.groups.len();
        for session in &self.sessions {
            // Activation validates the slot before creating a session.
            result.sessions_by_slot[(session.activation.slot + 1) as usize] += 1;
        }
        result
    }
    fn next_serial(&mut self) -> Option<i32> {
        self.serial = self.serial.checked_add(1)?;
        Some(self.serial)
    }
    fn sequence(&mut self, peer: Peer) -> u16 {
        let state = self
            .indications
            .get_mut(&peer)
            .expect("admitted modem peer");
        state.sequence = state.sequence.wrapping_add(1);
        state.sequence
    }
    fn sid_index(id: u8) -> Option<usize> {
        (FIRST_SESSION_ID..=LAST_SESSION_ID)
            .contains(&id)
            .then_some(usize::from(id.saturating_sub(FIRST_SESSION_ID)))
    }
    fn stamp(&self, id: u8) -> SidStamp {
        SidStamp {
            id,
            incarnation: self.sid_life[Self::sid_index(id).expect("owned SID")].incarnation,
        }
    }
    fn reply(&self, peer: Peer, bytes: Vec<u8>, id: Option<u8>) -> Effect {
        Effect::Send(
            peer,
            Packet::reply(
                bytes,
                id.map(|id| self.stamp(id)),
                !self.peer_is_tracked(peer),
            ),
        )
    }
    /// A typed read-only calendar completion for the already validated request.
    pub fn calendar_reply(
        &self,
        peer: Peer,
        transaction: u16,
        pdp: u8,
        sequence: u32,
        sample: Option<([u16; 8], u64)>,
    ) -> Effect {
        let bytes = sample
            .and_then(|(fields, seconds)| {
                p::timezone_response(transaction, pdp, sequence, fields, seconds).ok()
            })
            .unwrap_or_else(|| p::response(transaction, 0x32, 1, 0));
        self.reply(peer, bytes, None)
    }
    /// The caller must retain/submit/discard returned effects before another
    /// engine transition, so retired IDs can be pinned before reuse.
    pub fn retain_output(&mut self, peer: Peer, packet: Packet) -> Option<Retained> {
        // One-shot replies to unadmitted peers have no reusable lifetime and
        // must be submitted immediately, never deferred across peer changes.
        let peer_epoch = self.indications.get(&peer)?.epoch;
        let stamp = match &packet.fence {
            Fence::Reply(sid) => *sid,
            Fence::Up(snapshot) => Some(snapshot.sid),
            Fence::Terminal(sid) => Some(*sid),
        };
        if let Some(stamp) = stamp {
            let life = &mut self.sid_life[Self::sid_index(stamp.id)?];
            if life.incarnation != stamp.incarnation || life.owner != Some(peer) {
                return None;
            }
            life.pins = life.pins.checked_add(1)?;
        }
        Some(Retained {
            peer,
            peer_epoch: Some(peer_epoch),
            packet,
            pinned: stamp,
        })
    }
    pub fn peer_is_tracked(&self, peer: Peer) -> bool {
        self.indications.contains_key(&peer)
    }
    pub fn request_is_current(&self, request: Request) -> bool {
        self.broker
            && self
                .groups
                .get(&request.key)
                .is_some_and(|group| group.request == request)
    }
    pub fn prepare_output(&self, output: &Retained) -> Option<Vec<u8>> {
        if self.indications.get(&output.peer).map(|p| p.epoch) != output.peer_epoch {
            return None;
        }
        if let Some(stamp) = output.pinned {
            let life = self.sid_life[Self::sid_index(stamp.id)?];
            if life.incarnation != stamp.incarnation || life.owner != Some(output.peer) {
                return None;
            }
        }
        match &output.packet.fence {
            Fence::Reply(_) => Some(output.packet.bytes.clone()),
            Fence::Terminal(stamp) => (!self.sessions.iter().any(|s| s.id == stamp.id))
                .then(|| output.packet.bytes.clone()),
            Fence::Up(UpSnapshot {
                sid,
                request,
                address,
                network,
                cookie,
                sequence,
                force_initial,
            }) => {
                let session = self
                    .sessions
                    .iter()
                    .find(|s| s.id == sid.id && s.peer == output.peer)?;
                let group = self.groups.get(&Self::group_key(&session.activation))?;
                if group.request != *request
                    || group.network.as_ref() != Some(network)
                    || session.activation.cookie != *cookie
                    || session.address != Some(*address)
                    || group.network.as_ref()?.address(session.activation.family) != Some(*address)
                {
                    return None;
                }
                Some(outgoing::up_wire(
                    *sequence,
                    sid.id,
                    &session.activation,
                    *address,
                    !*force_initial && session.submitted_address.is_some(),
                ))
            }
        }
    }
    pub fn finish_output(&mut self, output: Retained, result: Disposition) {
        if matches!(result, Disposition::Submitted) && self.prepare_output(&output).is_some() {
            if let Fence::Up(UpSnapshot { sid, address, .. }) = output.packet.fence {
                if self.prepare_output_ref(&output.peer, sid, address) {
                    if let Some(session) = self
                        .sessions
                        .iter_mut()
                        .find(|s| s.id == sid.id && s.peer == output.peer)
                    {
                        session.submitted_address = Some(address);
                    }
                }
            }
        }
        if let Some(stamp) = output.pinned {
            if let Some(index) = Self::sid_index(stamp.id) {
                let life = &mut self.sid_life[index];
                if life.incarnation == stamp.incarnation && life.owner == Some(output.peer) {
                    life.pins = life
                        .pins
                        .checked_sub(1)
                        .expect("one retained-output finish");
                }
            }
        }
    }
    fn prepare_output_ref(&self, peer: &Peer, sid: SidStamp, address: IpAddr) -> bool {
        Self::sid_index(sid.id)
            .is_some_and(|index| self.sid_life[index].incarnation == sid.incarnation)
            && self
                .sessions
                .iter()
                .any(|s| s.id == sid.id && s.peer == *peer && s.address == Some(address))
    }

    fn group_key(a: &Activation) -> Key {
        Key {
            slot: a.slot,
            kind: a.pdn_type,
        }
    }
    pub fn broker_connected(&mut self) -> Vec<Effect> {
        self.diagnostics.broker_registrations =
            self.diagnostics.broker_registrations.saturating_add(1);
        // Replacement is a new epoch: old reports cannot revive a released request.
        self.broker = true;
        let keys: Vec<_> = self.groups.keys().copied().collect();
        let mut out = vec![];
        for key in keys {
            if let Some(serial) = self.next_serial() {
                let g = self.groups.get_mut(&key).unwrap();
                g.request.serial = serial;
                g.network = None;
                out.push(Effect::BringUp(g.request));
            }
        }
        for s in &mut self.sessions {
            s.address = None;
            s.submitted_address = None;
            s.last_network = None;
        }
        out
    }
    pub fn broker_lost(&mut self) -> Vec<Effect> {
        self.diagnostics.broker_losses = self.diagnostics.broker_losses.saturating_add(1);
        self.broker = false;
        let peers: Vec<_> = self.indications.keys().copied().collect();
        let mut out = vec![];
        for peer in peers {
            out.extend(self.fail_matching(|s| s.peer == peer));
        }
        out
    }
    /// A oneway bring-up the live broker did not receive can never be answered
    /// by a report. Complete it toward the modem like an unavailable network.
    pub fn bring_up_failed(&mut self, request: Request) -> Vec<Effect> {
        self.diagnostics.broker_call_failures =
            self.diagnostics.broker_call_failures.saturating_add(1);
        if !self.request_is_current(request) {
            return vec![];
        }
        self.fail_matching(|s| Self::group_key(&s.activation) == request.key)
    }
    /// The group is already gone. The broker adopts its held request again on
    /// the next bring-up for the same slot and type.
    pub fn release_failed(&mut self) {
        self.diagnostics.broker_call_failures =
            self.diagnostics.broker_call_failures.saturating_add(1);
    }
    pub fn set_emergency_enabled(&mut self, enabled: bool) -> Vec<Effect> {
        self.emergency_enabled = enabled;
        if enabled {
            vec![]
        } else {
            self.fail_matching(|s| s.activation.pdn_type == PdnType::Emergency)
        }
    }
    pub fn peer_gone(&mut self, peer: Peer) -> Vec<Effect> {
        if self.sessions.iter().any(|s| s.peer == peer) || self.indications.contains_key(&peer) {
            self.diagnostics.client_losses = self.diagnostics.client_losses.saturating_add(1);
        }
        self.sessions.retain(|s| s.peer != peer);
        self.indications.remove(&peer);
        self.release_unused()
    }
    pub fn node_gone(&mut self, node: u32) -> Vec<Effect> {
        if node == self.modem_node {
            self.diagnostics.modem_losses = self.diagnostics.modem_losses.saturating_add(1);
        }
        self.sessions.retain(|s| s.peer.node != node);
        self.indications.retain(|p, _| p.node != node);
        self.release_unused()
    }
    pub fn shutdown(&mut self) -> Vec<Effect> {
        self.sessions.clear();
        self.indications.clear();
        self.release_unused()
    }
    fn release_unused(&mut self) -> Vec<Effect> {
        let keys: Vec<_> = self
            .groups
            .keys()
            .filter(|k| {
                !self
                    .sessions
                    .iter()
                    .any(|s| Self::group_key(&s.activation) == **k)
            })
            .copied()
            .collect();
        keys.into_iter()
            .filter_map(|k| self.groups.remove(&k).map(|g| Effect::Release(g.request)))
            .collect()
    }
    fn fail_matching(&mut self, predicate: impl Fn(&Session) -> bool) -> Vec<Effect> {
        let ids: Vec<_> = self
            .sessions
            .iter()
            .filter(|s| predicate(s))
            .map(|s| s.id)
            .collect();
        let mut out = vec![];
        for id in ids {
            let i = self.sessions.iter().position(|s| s.id == id).unwrap();
            let s = self.sessions.remove(i);
            let seq = self.sequence(s.peer);
            out.push(Effect::Send(
                s.peer,
                Packet::terminal(
                    p::indication(seq, s.id, &s.activation, None, false),
                    self.stamp(s.id),
                ),
            ));
        }
        out.extend(self.release_unused());
        out
    }
    pub fn report(&mut self, request: Request, network: Option<Network>) -> Vec<Effect> {
        if !self.broker
            || !self
                .groups
                .get(&request.key)
                .is_some_and(|g| g.request == request)
        {
            self.diagnostics.stale_reports = self.diagnostics.stale_reports.saturating_add(1);
            return vec![];
        }
        let Some(network) = network.filter(Network::valid) else {
            self.diagnostics.down_reports = self.diagnostics.down_reports.saturating_add(1);
            return self.fail_matching(|s| Self::group_key(&s.activation) == request.key);
        };
        self.diagnostics.up_reports = self.diagnostics.up_reports.saturating_add(1);
        self.groups.get_mut(&request.key).unwrap().network = Some(network.clone());
        let mut out = vec![];
        let ids: Vec<_> = self
            .sessions
            .iter()
            .filter(|s| Self::group_key(&s.activation) == request.key)
            .map(|s| s.id)
            .collect();
        for id in ids {
            let i = self.sessions.iter().position(|s| s.id == id).unwrap();
            let s = &self.sessions[i];
            let address = network.address(s.activation.family);
            if address.is_some()
                && address == s.address
                && s.last_network.as_ref() == Some(&network)
            {
                continue;
            }
            let peer = s.peer;
            let change = s.address.is_some() && address.is_some();
            let a = s.activation.clone();
            let seq = self.sequence(peer);
            out.push(Effect::Send(
                peer,
                match address {
                    Some(address) => Packet::up(
                        p::indication(seq, id, &a, Some(address), change),
                        UpSnapshot {
                            sid: self.stamp(id),
                            request,
                            address,
                            network: network.clone(),
                            cookie: a.cookie,
                            sequence: seq,
                            force_initial: false,
                        },
                    ),
                    None => {
                        Packet::terminal(p::indication(seq, id, &a, None, false), self.stamp(id))
                    }
                },
            ));
            if address.is_some() {
                self.sessions[i].address = address;
                self.sessions[i].last_network = Some(network.clone());
            } else {
                let slot = (a.slot + 1) as usize;
                self.diagnostics.missing_family_releases_by_slot[slot] =
                    self.diagnostics.missing_family_releases_by_slot[slot].saturating_add(1);
                self.sessions.remove(i);
            }
        }
        out.extend(self.release_unused());
        out
    }
    /// No reply to non-modem peers, indications, or frames too short to identify.
    pub fn receive(&mut self, peer: Peer, bytes: &[u8]) -> Vec<Effect> {
        if peer.node != self.modem_node || peer.port == 0 || peer.port >= 0xffff_fffe {
            return vec![];
        }
        let frame = match Frame::parse(bytes) {
            Ok(f) => f,
            Err(_) => {
                self.diagnostics.malformed = self.diagnostics.malformed.saturating_add(1);
                if bytes.len() >= 7 && bytes.len() <= p::MAX_DATAGRAM && bytes[0] == 0 {
                    return vec![self.reply(
                        peer,
                        p::response(
                            u16::from_le_bytes([bytes[1], bytes[2]]),
                            u16::from_le_bytes([bytes[3], bytes[4]]),
                            1,
                            0x3a,
                        ),
                        None,
                    )];
                }
                return vec![];
            }
        };
        if frame.kind != Kind::Request {
            return vec![];
        }
        if p::validate_request(&frame).is_err() {
            self.diagnostics.malformed = self.diagnostics.malformed.saturating_add(1);
            return vec![self.reply(peer, p::response(frame.txn, frame.id, 1, 0x3a), None)];
        }
        self.diagnostics.requests = self.diagnostics.requests.saturating_add(1);
        self.diagnostics.last_request = frame.id;
        match frame.id {
            p::ACTIVATE => self.activate(peer, &frame),
            p::DEACTIVATE => self.deactivate(peer, &frame),
            0x22 => vec![], // Observed stock GET_IP_ADDRESS handler sends no reply.
            // These handlers are enabled only with their validated, bounded fields.
            0x23 => vec![self.reply(peer, p::response(frame.txn, frame.id, 0, 0), None)],
            0x33 => {
                // Width/mandatory validation above; scope destruction to this client.
                let instance = p::u32_value(frame.required(1).unwrap()).unwrap();
                let counts = &mut self.diagnostics.modem_instance_destructions_by_slot;
                self.sessions.retain(|s| {
                    let remove = s.peer == peer && s.activation.instance.unwrap_or(0) == instance;
                    if remove {
                        let slot = (s.activation.slot + 1) as usize;
                        counts[slot] = counts[slot].saturating_add(1);
                    }
                    !remove
                });
                let mut out = vec![self.reply(peer, p::response(frame.txn, frame.id, 0, 0), None)];
                out.extend(self.release_unused());
                out
            }
            0x32 => vec![Effect::ReadTime {
                peer,
                txn: frame.txn,
                pdp: p::u8_value(frame.required(1).unwrap()).unwrap(),
                sequence: p::u32_value(frame.required(2).unwrap()).unwrap(),
            }],
            0x2e | 0x34 => vec![self.reply(peer, p::response(frame.txn, frame.id, 0, 0), None)],
            // Undecoded commands never report successful execution.
            0x25..=0x2a | 0x2c | 0x2d | 0x31 => {
                vec![self.reply(peer, p::response(frame.txn, frame.id, 1, 0), None)]
            }
            _ => vec![self.reply(peer, p::response(frame.txn, frame.id, 1, 0x3a), None)],
        }
    }
    fn activate(&mut self, peer: Peer, f: &Frame<'_>) -> Vec<Effect> {
        let a = match Activation::decode(f, self.slots) {
            Ok(a) => a,
            Err(e) => {
                return vec![self.reply(
                    peer,
                    p::response(f.txn, f.id, 1, if e == p::Error::Value { 0 } else { 0x3a }),
                    None,
                )]
            }
        };
        let slot = (a.slot + 1) as usize;
        self.diagnostics.activation_requests_by_slot[slot] =
            self.diagnostics.activation_requests_by_slot[slot].saturating_add(1);
        if a.pdn_type == PdnType::Emergency && !self.emergency_enabled {
            return vec![self.reply(peer, p::response(f.txn, f.id, 1, 0), None)];
        }
        if !self.indications.contains_key(&peer) && self.indications.len() >= MAX_CLIENTS {
            return vec![self.reply(peer, p::response(f.txn, f.id, 1, 5), None)];
        }
        if let Some(i) = self
            .sessions
            .iter()
            .position(|s| s.peer == peer && s.activation.same_pdn(&a))
        {
            let id = self.sessions[i].id;
            let address = self.sessions[i].address;
            self.sessions[i].activation.cookie = a.cookie;
            let mut out = vec![self.reply(peer, p::activation_response(f.txn, id, &a), Some(id))];
            if let Some(ip) = address {
                let seq = self.sequence(peer);
                out.push(Effect::Send(
                    peer,
                    Packet::up(
                        p::indication(seq, id, &a, Some(ip), false),
                        UpSnapshot {
                            sid: self.stamp(id),
                            request: self.groups[&Self::group_key(&a)].request,
                            address: ip,
                            network: self.groups[&Self::group_key(&a)]
                                .network
                                .clone()
                                .expect("current address network"),
                            cookie: a.cookie,
                            sequence: seq,
                            force_initial: true,
                        },
                    ),
                ));
            }
            return out;
        }
        let normal = self
            .sessions
            .iter()
            .filter(|s| s.activation.pdn_type == PdnType::Ims)
            .count();
        if self.sessions.len() >= MAX_SESSIONS
            || (a.pdn_type == PdnType::Ims && normal >= MAX_SESSIONS - EMERGENCY_RESERVE)
        {
            return vec![self.reply(peer, p::response(f.txn, f.id, 1, 5), None)];
        }
        let Some(id) = (FIRST_SESSION_ID..=LAST_SESSION_ID).find(|id| {
            !self.sessions.iter().any(|s| s.id == *id)
                && self.sid_life[Self::sid_index(*id).expect("bounded SID")].pins == 0
        }) else {
            return vec![self.reply(peer, p::response(f.txn, f.id, 1, 5), None)];
        };
        let index = Self::sid_index(id).expect("allocated SID");
        let Some(incarnation) = self.sid_life[index].incarnation.checked_add(1) else {
            return vec![self.reply(peer, p::response(f.txn, f.id, 1, 5), None)];
        };
        self.sid_life[index] = SidLife {
            incarnation,
            owner: Some(peer),
            pins: 0,
        };
        if !self.indications.contains_key(&peer) {
            let epoch = self.next_peer_epoch;
            let Some(next) = epoch.checked_add(1) else {
                return vec![self.reply(peer, p::response(f.txn, f.id, 1, 5), None)];
            };
            self.next_peer_epoch = next;
            self.indications
                .insert(peer, PeerLife { sequence: 0, epoch });
        }
        let key = Self::group_key(&a);
        let mut out = vec![self.reply(peer, p::activation_response(f.txn, id, &a), Some(id))];
        if !self.groups.contains_key(&key) {
            let Some(serial) = self.next_serial() else {
                return vec![self.reply(peer, p::response(f.txn, f.id, 1, 5), None)];
            };
            let request = Request { key, serial };
            self.groups.insert(
                key,
                Group {
                    request,
                    network: None,
                },
            );
            if self.broker {
                out.push(Effect::BringUp(request));
            }
        }
        self.sessions.push(Session {
            peer,
            id,
            activation: a,
            address: None,
            submitted_address: None,
            last_network: None,
        });
        if let Some(network) = self.groups[&key].network.clone() {
            out.extend(self.report(self.groups[&key].request, Some(network)));
        }
        out
    }
    fn deactivate(&mut self, peer: Peer, f: &Frame<'_>) -> Vec<Effect> {
        let decoded = f
            .required(1)
            .and_then(p::u8_value)
            .and_then(|id| f.optional_u32(0x10).map(|instance| (id, instance)));
        let (id, instance) = match decoded {
            Ok(v) => v,
            Err(_) => return vec![self.reply(peer, p::response(f.txn, f.id, 1, 0x3a), None)],
        };
        let found = self.sessions.iter().position(|s| {
            s.peer == peer
                && s.id == id
                && instance.is_none_or(|v| s.activation.instance.unwrap_or(0) == v)
        });
        let mut e = p::Encoder::new(Kind::Response, f.txn, f.id);
        let _ = e.tlv(
            2,
            if found.is_some() {
                &[0, 0, 0, 0]
            } else {
                &[1, 0, 0x1c, 0]
            },
        );
        let _ = e.tlv(0x10, &[250]);
        if let Some(v) = instance {
            let _ = e.tlv(0x11, &v.to_le_bytes());
        }
        let mut out = vec![self.reply(peer, e.finish(), found.map(|i| self.sessions[i].id))];
        if let Some(i) = found {
            let slot = (self.sessions[i].activation.slot + 1) as usize;
            self.diagnostics.modem_releases_by_slot[slot] =
                self.diagnostics.modem_releases_by_slot[slot].saturating_add(1);
            let session = self.sessions.remove(i);
            out.extend(
                self.release_unused()
                    .into_iter()
                    .map(|effect| match effect {
                        Effect::Release(request) => Effect::ReleaseAfter { peer, request },
                        other => other,
                    }),
            );
            // The deactivate response acknowledges the request; the modem also
            // expects a terminal PDP indication for the original context. Stock
            // emits this after its data-service teardown, before a replacement
            // activation. Revoke ownership and release an unused broker request
            // first. Never include an address or notify another owner's context.
            let seq = self.sequence(peer);
            out.push(Effect::Send(
                peer,
                Packet::terminal(
                    p::indication(seq, session.id, &session.activation, None, false),
                    self.stamp(session.id),
                ),
            ));
        }
        out
    }
}
