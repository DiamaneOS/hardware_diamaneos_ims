// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! Android delivery adapter. The engine alone creates freshness/lifetime fences.
use binder::{DeathRecipient, StatusCode, Strong};
use diamaneos_ims_dcm::{
    engine::{Effect, Engine, Peer, Request},
    outgoing::{Disposition, Retained},
};
use diamaneos_ims_runtime::{
    outbox::{Attempt, Finished, Outbox},
    socket::Qrtr,
    transport::{classify, exhausted, Fault},
};
use std::{collections::VecDeque, io};
use vendor_diamaneos_hardware_imsdcm::aidl::vendor::diamaneos::hardware::imsdcm::IPdnBroker::IPdnBroker;

pub type Broker = (u64, Strong<dyn IPdnBroker>, DeathRecipient);
enum Work {
    Packet(Retained),
    Release { epoch: u64, request: Request },
}
#[derive(Default)]
pub struct Dispatch {
    queue: Outbox<Work>,
    pub broker_lost: Option<u64>,
    pub socket_reset: bool,
}
/// Both broker calls are oneway, so only transport errors return. A dead object
/// loses the broker. Any other error lost this one delivery while the
/// registration stays valid: the broker only registers again after daemon death,
/// so neither exiting nor forgetting it would recover. False means that case.
fn callback(result: binder::Result<()>, lost: &mut Option<u64>, epoch: u64) -> bool {
    match result {
        Ok(()) => true,
        Err(error) if error.transaction_error() == StatusCode::DEAD_OBJECT => {
            *lost = Some(epoch);
            true
        }
        Err(_) => false,
    }
}
impl Dispatch {
    fn release(
        &mut self,
        engine: &mut Engine,
        broker: Option<&Broker>,
        epoch: u64,
        request: Request,
    ) {
        if let Some((current, callback_binder, _)) = broker {
            if *current == epoch
                && !callback(
                    callback_binder.release(&super::request(request)),
                    &mut self.broker_lost,
                    epoch,
                )
            {
                engine.release_failed();
            }
        }
    }
    fn finish_purged(&mut self, engine: &mut Engine, broker: Option<&Broker>, work: Vec<Work>) {
        for item in work {
            match item {
                Work::Packet(packet) => engine.finish_output(packet, Disposition::PeerLost),
                Work::Release { epoch, request } => self.release(engine, broker, epoch, request),
            }
        }
    }
    pub fn purge_all(&mut self, engine: &mut Engine, broker: Option<&Broker>) {
        let work = self.queue.drain();
        self.finish_purged(engine, broker, work);
    }
    pub fn purge_peer(&mut self, engine: &mut Engine, broker: Option<&Broker>, peer: Peer) {
        let work = self.queue.purge(peer);
        self.finish_purged(engine, broker, work);
    }
    pub fn purge_node(&mut self, engine: &mut Engine, broker: Option<&Broker>, node: u32) {
        for peer in self
            .queue
            .peers()
            .into_iter()
            .filter(|peer| peer.node == node)
        {
            self.purge_peer(engine, broker, peer);
        }
    }
    /// Always dispose all owned output and attempt every remaining release,
    /// including when the operational loop exits through an error.
    pub fn shutdown(&mut self, engine: &mut Engine, broker: Option<&Broker>) {
        self.purge_all(engine, broker);
        for effect in engine.shutdown() {
            if let Effect::Release(request) = effect {
                if let Some((epoch, _, _)) = broker {
                    self.release(engine, broker, *epoch, request);
                }
            }
        }
    }
    pub fn apply(
        &mut self,
        socket: &Qrtr,
        engine: &mut Engine,
        broker: Option<&Broker>,
        effects: Vec<Effect>,
        now: u64,
    ) -> io::Result<()> {
        let mut pending: VecDeque<_> = effects.into();
        while let Some(effect) = pending.pop_front() {
            match effect {
                Effect::Send(peer, packet) => {
                    if !engine.peer_is_tracked(peer) {
                        if !packet.is_unadmitted_reply() {
                            continue;
                        }
                        match socket.send(peer, &packet) {
                            Ok(()) => (),
                            Err(error) => match classify(&error) {
                                // There is no admitted session to retire or
                                // lifetime to queue; caller may retry its request.
                                Fault::Retry | Fault::PeerGone => {
                                    if exhausted(&error) {
                                        engine.transport_exhausted();
                                    }
                                }
                                Fault::SocketReset => self.socket_reset = true,
                                Fault::Unexpected => return Err(error),
                            },
                        }
                        continue;
                    }
                    let cost = packet.wire_size_bound();
                    let Some(packet) = engine.retain_output(peer, packet) else {
                        continue;
                    };
                    // A batch of network replacements can invalidate queued UPs
                    // before flushing. They must not exhaust a healthy peer's budget.
                    let obsolete = self.queue.discard_obsolete(peer, |work| match work {
                        Work::Packet(packet) => engine.prepare_output(packet).is_some(),
                        Work::Release { .. } => true,
                    });
                    for work in obsolete {
                        if let Work::Packet(packet) = work {
                            engine.finish_output(packet, Disposition::Obsolete);
                        }
                    }
                    if let Err(rejected) = self.queue.enqueue(peer, Work::Packet(packet), cost, now)
                    {
                        self.finish_purged(engine, broker, vec![rejected.token]);
                        let work = self.queue.purge(peer);
                        self.finish_purged(engine, broker, work);
                        pending.extend(engine.peer_gone(peer));
                    }
                }
                Effect::BringUp(request) => {
                    if !engine.request_is_current(request) {
                        continue;
                    }
                    if let Some((epoch, b, _)) = broker {
                        if self.broker_lost == Some(*epoch) {
                            continue;
                        }
                        if !callback(
                            b.bringUp(&super::request(request)),
                            &mut self.broker_lost,
                            *epoch,
                        ) {
                            pending.extend(engine.bring_up_failed(request));
                        }
                    }
                }
                Effect::Release(request) => {
                    if let Some((epoch, _, _)) = broker {
                        self.release(engine, broker, *epoch, request);
                    }
                }
                Effect::ReleaseAfter { peer, request } => {
                    let epoch = broker.map_or(0, |b| b.0);
                    if let Err(rejected) =
                        self.queue
                            .enqueue(peer, Work::Release { epoch, request }, 0, now)
                    {
                        self.finish_purged(engine, broker, vec![rejected.token]);
                        let work = self.queue.purge(peer);
                        self.finish_purged(engine, broker, work);
                        pending.extend(engine.peer_gone(peer));
                    }
                }
                Effect::ReadTime {
                    peer,
                    txn,
                    pdp,
                    sequence,
                } => {
                    let sample = diamaneos_ims_runtime::clock::now().ok();
                    let effect = engine.calendar_reply(
                        peer,
                        txn,
                        pdp,
                        sequence,
                        sample.map(|sample| (sample.fields, sample.utc_seconds)),
                    );
                    pending.push_front(effect);
                }
            }
        }
        Ok(())
    }
    pub fn flush(
        &mut self,
        socket: &Qrtr,
        engine: &mut Engine,
        broker: Option<&Broker>,
        now: u64,
    ) -> io::Result<()> {
        // An observed death must invalidate the engine before any deferred UP
        // can be sent. A failed old epoch must not block its replacement.
        if broker.is_some_and(|current| self.broker_lost == Some(current.0)) {
            return Ok(());
        }
        // Match the bounded Binder-input quantum. A full valid response burst
        // must not be limited to one packet per25ms; peers remain round-robin.
        const MAX_SUBMISSIONS_PER_CYCLE: usize = 32;
        for _ in 0..MAX_SUBMISSIONS_PER_CYCLE {
            if self.queue.peer_count() == 0 {
                break;
            }
            let mut fatal = None;
            let mut broker_lost = None;
            let mut socket_reset = false;
            let mut exhaustion = false;
            let mut release_failed = false;
            let finished = self.queue.attempt(
                now,
                |_, work| match work {
                    Work::Packet(packet) => engine.prepare_output(packet).is_some(),
                    Work::Release { epoch, .. } => {
                        broker.is_some_and(|current| current.0 == *epoch)
                    }
                },
                |_, work| match work {
                    Work::Packet(packet) => {
                        let Some(bytes) = engine.prepare_output(packet) else {
                            return Attempt::Obsolete;
                        };
                        match socket.send(packet.peer(), &bytes) {
                            Ok(()) => Attempt::Submitted,
                            Err(error) => match classify(&error) {
                                Fault::Retry => {
                                    exhaustion = exhausted(&error);
                                    Attempt::Backpressured
                                }
                                Fault::PeerGone => Attempt::PeerLost,
                                Fault::SocketReset => {
                                    socket_reset = true;
                                    Attempt::PeerLost
                                }
                                Fault::Unexpected => {
                                    fatal = Some(error);
                                    Attempt::Backpressured
                                }
                            },
                        }
                    }
                    Work::Release { epoch, request } => {
                        let Some((current, b, _)) = broker else {
                            return Attempt::Obsolete;
                        };
                        if current != epoch {
                            return Attempt::Obsolete;
                        }
                        release_failed = !callback(
                            b.release(&super::request(*request)),
                            &mut broker_lost,
                            *epoch,
                        );
                        Attempt::Submitted
                    }
                },
            );
            if exhaustion {
                engine.transport_exhausted();
            }
            if release_failed {
                engine.release_failed();
            }
            if broker_lost.is_some() {
                self.broker_lost = broker_lost;
            }
            self.socket_reset |= socket_reset;
            if let Some(error) = fatal {
                return Err(error);
            }
            match finished {
                Some(Finished::Submitted(Work::Packet(packet))) => {
                    engine.finish_output(packet, Disposition::Submitted)
                }
                Some(Finished::Obsolete(Work::Packet(packet))) => {
                    engine.finish_output(packet, Disposition::Obsolete)
                }
                Some(Finished::PeerLost(peer, work)) => {
                    self.finish_purged(engine, broker, work);
                    let effects = engine.peer_gone(peer);
                    self.apply(socket, engine, broker, effects, now)?;
                }
                _ => (),
            }
            if self.broker_lost.is_some() || self.socket_reset {
                break;
            }
        }
        Ok(())
    }
}
