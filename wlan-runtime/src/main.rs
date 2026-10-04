// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
use binder::{
    BinderFeatures, DeathRecipient, ExceptionCode, IBinder, Interface, ProcessState, SpIBinder,
    Status, ThreadState,
};
use de_diamaneos_wlan::aidl::de::diamaneos::wlan::{
    IReporter::{BnReporter, IReporter},
    ReporterStatus::ReporterStatus,
    Snapshot::Snapshot,
};
use diamaneos_ims_dcm::{
    engine::Peer,
    qrtr::{Control, BYE, CTRL_PORT, DEL_SERVER, NEW_LOOKUP, NEW_SERVER},
};
use diamaneos_ims_runtime::{
    seccomp,
    socket::Qrtr,
    transport::{classify, Fault},
};
use diamaneos_wlan_reporting::{
    session::{Observation, Session},
    Connected, DSD_SERVICE,
};
use diamaneos_wlan_runtime::Observations;
use std::{
    io,
    net::{Ipv4Addr, Ipv6Addr},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

const SERVICE: &str = "de.diamaneos.wlan.IReporter/default";
// Downstream scheduling/containment choices, not carrier registration timers.
const SESSION_RENEWAL_DELAY_MS: u64 = 30_000;
const MAX_SESSION_RENEWALS: u8 = 3;
const MAX_RECEIVED_PACKETS_PER_CYCLE: usize = 8;
const REPORT_LOOP_PERIOD_MS: u64 = 250;
// One 2-second protocol deadline plus scheduling margin; never block init exit
// indefinitely trying to reconcile unavailable modem state.
const SHUTDOWN_WITHDRAWAL_BUDGET_MS: u64 = 2_500;
const SHUTDOWN_POLL_PERIOD_MS: u64 = 50;
static STOP: AtomicBool = AtomicBool::new(false);
// Globally unique in this process across both subscriptions and replacements.
static NEXT_PROFILE_CONTEXT: AtomicU64 = AtomicU64::new(1);
extern "C" fn stop(_: libc::c_int) {
    STOP.store(true, Ordering::Relaxed);
}
struct Shared {
    observations: Observations,
    uid: Option<u32>,
    death: Option<DeathRecipient>,
    lifetime: Option<SpIBinder>,
    status: ReporterStatus,
}
struct Service {
    state: Arc<Mutex<Shared>>,
    origin: Instant,
}
impl Interface for Service {}
fn denied() -> Status {
    Status::new_exception(ExceptionCode::SECURITY, None)
}
fn invalid() -> Status {
    Status::new_exception(ExceptionCode::ILLEGAL_ARGUMENT, None)
}
fn elapsed(origin: Instant) -> u64 {
    origin.elapsed().as_millis().min(u64::MAX as u128) as u64
}
impl IReporter for Service {
    fn getStatus(&self, generation: i64) -> binder::Result<ReporterStatus> {
        let s = self.state.lock().map_err(|_| denied())?;
        if generation <= 0
            || s.uid != Some(ThreadState::get_calling_uid())
            || !s.observations.is_current(generation as u64)
        {
            return Err(denied());
        }
        Ok(ReporterStatus {
            primaryStage: s.status.primaryStage,
            primaryOperation: s.status.primaryOperation,
            primaryError: s.status.primaryError,
            secondaryStage: s.status.secondaryStage,
            secondaryOperation: s.status.secondaryOperation,
            secondaryError: s.status.secondaryError,
            primaryKeepaliveError: s.status.primaryKeepaliveError,
            secondaryKeepaliveError: s.status.secondaryKeepaliveError,
            primaryProfileError: s.status.primaryProfileError,
            secondaryProfileError: s.status.secondaryProfileError,
        })
    }
    fn registerObserver(&self, lifetime: &SpIBinder) -> binder::Result<i64> {
        let uid = ThreadState::get_calling_uid();
        if !(10000..20000).contains(&uid) {
            return Err(denied());
        }
        let mut s = self.state.lock().map_err(|_| denied())?;
        if s.uid.is_some_and(|old| old != uid) {
            return Err(denied());
        }
        let generation = s
            .observations
            .register()
            .filter(|v| *v <= i64::MAX as u64)
            .ok_or_else(denied)?;
        let state = self.state.clone();
        let mut death = DeathRecipient::new(move || {
            if let Ok(mut s) = state.lock() {
                s.observations.lost(generation);
            }
        });
        let mut lifetime = lifetime.clone();
        lifetime.link_to_death(&mut death)?;
        s.uid = Some(uid);
        s.death = Some(death);
        s.lifetime = Some(lifetime);
        Ok(generation as i64)
    }
    fn observe(&self, generation: i64, sequence: i64, snapshot: &Snapshot) -> binder::Result<()> {
        if generation <= 0 || sequence <= 0 {
            return Err(invalid());
        }
        let network = if snapshot.connected {
            if !snapshot.enabled || !(0..=128).contains(&snapshot.ipv6Prefix) {
                return Err(invalid());
            }
            let v4 = snapshot.hasIpv4.then(|| Ipv4Addr::from(snapshot.ipv4));
            let v6 = snapshot
                .hasIpv6
                .then(|| (Ipv6Addr::from(snapshot.ipv6), snapshot.ipv6Prefix as u8));
            if !(0..=2).contains(&snapshot.dns4Count) || !(0..=2).contains(&snapshot.dns6Count) {
                return Err(invalid());
            }
            let mut dns4 = [None; 2];
            let mut dns6 = [None; 2];
            for (index, bytes) in snapshot.dns4.chunks_exact(4).enumerate() {
                if index < snapshot.dns4Count as usize {
                    dns4[index] = Some(Ipv4Addr::new(bytes[0], bytes[1], bytes[2], bytes[3]));
                } else if bytes.iter().any(|b| *b != 0) {
                    return Err(invalid());
                }
            }
            for (index, bytes) in snapshot.dns6.chunks_exact(16).enumerate() {
                if index < snapshot.dns6Count as usize {
                    let mut address = [0; 16];
                    address.copy_from_slice(bytes);
                    dns6[index] = Some(Ipv6Addr::from(address));
                } else if bytes.iter().any(|b| *b != 0) {
                    return Err(invalid());
                }
            }
            Some(
                Connected::new(snapshot.bssid, v4, v6, snapshot.validated)
                    .map(|network| network.with_default_route(snapshot.defaultRoute))
                    .and_then(|network| network.with_dns(dns4, dns6))
                    .map_err(|_| invalid())?,
            )
        } else {
            None
        };
        let mut s = self.state.lock().map_err(|_| denied())?;
        if s.uid != Some(ThreadState::get_calling_uid()) {
            return Err(denied());
        }
        if !s.observations.update(
            generation as u64,
            sequence as u64,
            elapsed(self.origin),
            Observation {
                enabled: snapshot.enabled,
                network,
            },
        ) {
            return Err(invalid());
        }
        Ok(())
    }
}

struct Client {
    socket: Qrtr,
    endpoint: Option<Peer>,
    state: Session,
    subscription: u32,
    retry_at: u64,
    failures: u8,
    last_observation: Option<Observation>,
    lookup_pending: bool,
    // A replacement socket failed to bind; retry each cycle with this renewal count.
    rebind: Option<u8>,
}
impl Client {
    fn new(subscription: u32, node: u32) -> io::Result<Self> {
        let socket = Qrtr::bind()?;
        if socket.local().node == node {
            return Err(io::Error::other("local modem node"));
        }
        Ok(Self {
            socket,
            endpoint: None,
            state: {
                let generation = NEXT_PROFILE_CONTEXT
                    .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                        value.checked_add(1)
                    })
                    .map_err(|_| io::Error::other("profile context exhausted"))?;
                Session::new(subscription, generation)
                    .ok_or_else(|| io::Error::other("profile context invalid"))?
            },
            subscription,
            retry_at: 0,
            failures: 0,
            last_observation: None,
            lookup_pending: true,
            rebind: None,
        })
    }

    /// The old socket and session stay owned until a replacement binds, so a
    /// failed bind is retried next cycle instead of stopping the reporter.
    /// Nonzero renewal counts carry over with their observation.
    fn reconnect(&mut self, node: u32, failures: u8) -> bool {
        match Self::new(self.subscription, node) {
            Ok(mut next) => {
                if failures != 0 {
                    next.failures = failures;
                    next.last_observation = self.last_observation.take();
                }
                *self = next;
                true
            }
            Err(_) => {
                self.rebind = Some(failures);
                false
            }
        }
    }
    fn step(&mut self, node: u32, observation: Option<&Observation>, now: u64) -> io::Result<()> {
        if let Some(failures) = self.rebind {
            if !self.reconnect(node, failures) {
                return Ok(());
            }
        }
        if self.last_observation.as_ref() != observation {
            self.failures = 0;
            self.last_observation = observation.cloned();
        }
        if self.state.failed() {
            if self.retry_at == 0 {
                self.retry_at = now.saturating_add(SESSION_RENEWAL_DELAY_MS);
            }
            if now >= self.retry_at
                && self.failures < MAX_SESSION_RENEWALS
                && !self.reconnect(node, self.failures + 1)
            {
                return Ok(());
            }
        }
        if let Some(observation) = observation {
            self.state.observe(observation.clone());
        }
        if self.lookup_pending {
            let lookup = Control {
                command: NEW_LOOKUP,
                words: [DSD_SERVICE, 1, 0, 0],
            }
            .encode();
            match self.socket.send(
                Peer {
                    node: self.socket.local().node,
                    port: CTRL_PORT,
                },
                &lookup,
            ) {
                Ok(()) => self.lookup_pending = false,
                Err(error) => match classify(&error) {
                    Fault::Retry => return Ok(()),
                    Fault::PeerGone | Fault::SocketReset => {
                        self.reconnect(node, 0);
                        return Ok(());
                    }
                    Fault::Unexpected => return Err(error),
                },
            }
        }
        // Bound input work so indication floods cannot starve expiry or the other SIM.
        for _ in 0..MAX_RECEIVED_PACKETS_PER_CYCLE {
            let (peer, bytes) = match self.socket.receive(Duration::ZERO) {
                Ok(Some(packet)) => packet,
                Ok(None) => break,
                Err(error) => match classify(&error) {
                    Fault::Retry => break,
                    Fault::PeerGone | Fault::SocketReset => {
                        self.reconnect(node, 0);
                        return Ok(());
                    }
                    Fault::Unexpected => return Err(error),
                },
            };
            if peer
                == (Peer {
                    node: self.socket.local().node,
                    port: CTRL_PORT,
                })
            {
                if let Some(c) = Control::decode(&bytes) {
                    if c.command == NEW_SERVER
                        && c.words[0] == DSD_SERVICE
                        && c.words[1] == 1
                        && c.words[2] == node
                        && c.words[3] != 0
                        && c.words[3] != CTRL_PORT
                    {
                        let candidate = Peer {
                            node,
                            port: c.words[3],
                        };
                        if self.endpoint.is_some_and(|old| old != candidate) {
                            return Err(io::Error::other("ambiguous DSD endpoint"));
                        }
                        self.endpoint = Some(candidate);
                    }
                    let gone = c.command == BYE && c.words[0] == node
                        || c.command == DEL_SERVER
                            && c.words[0] == DSD_SERVICE
                            && c.words[1] == 1
                            && self.endpoint
                                == Some(Peer {
                                    node: c.words[2],
                                    port: c.words[3],
                                });
                    if gone {
                        self.reconnect(node, 0);
                        break;
                    }
                }
            } else if Some(peer) == self.endpoint {
                self.state.receive(&bytes);
            }
        }
        if let Some(peer) = self.endpoint {
            for packet in self.state.poll_cycle(now).into_iter().flatten() {
                match self.socket.send(peer, &packet) {
                    Ok(()) => (),
                    Err(error) => match classify(&error) {
                        // Session already owns this pending packet/token. Do not
                        // enqueue a second copy or reset the other subscription.
                        Fault::Retry => break,
                        Fault::PeerGone | Fault::SocketReset => {
                            self.reconnect(node, 0);
                            break;
                        }
                        Fault::Unexpected => return Err(error),
                    },
                }
            }
        }
        Ok(())
    }
}
fn run() -> io::Result<()> {
    if unsafe { libc::getuid() } != 2991 {
        return Err(io::Error::other("wrong uid"));
    }
    if std::fs::read_to_string("/sys/fs/selinux/enforce")?.trim() != "1" {
        return Err(io::Error::other("SELinux required"));
    }
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 3 {
        return Err(io::Error::other("device configuration required"));
    }
    let node: u32 = args[1]
        .parse()
        .map_err(|_| io::Error::other("invalid modem node"))?;
    let slots: u32 = args[2]
        .parse()
        .map_err(|_| io::Error::other("invalid slots"))?;
    if !(1..=2).contains(&slots) {
        return Err(io::Error::other("unsupported slots"));
    }
    let origin = Instant::now();
    let shared = Arc::new(Mutex::new(Shared {
        observations: Observations::default(),
        uid: None,
        death: None,
        lifetime: None,
        status: ReporterStatus::default(),
    }));
    let service = BnReporter::new_binder(
        Service {
            state: shared.clone(),
            origin,
        },
        BinderFeatures::default(),
    );
    seccomp::install()?;
    ProcessState::set_thread_pool_max_thread_count(2);
    ProcessState::start_thread_pool();
    binder::add_service(SERVICE, service.as_binder())
        .map_err(|_| io::Error::other("service registration"))?;
    let mut clients = (1..=slots)
        .map(|s| Client::new(s, node))
        .collect::<io::Result<Vec<_>>>()?;
    // Handler only stores an atomic flag. SIGTERM/SIGINT gets a bounded
    // withdrawal attempt. Default init restart uses SIGKILL, which cannot run
    // this handler; abrupt loss and modem failure cannot promise withdrawal.
    unsafe {
        libc::signal(libc::SIGTERM, stop as *const () as libc::sighandler_t);
        libc::signal(libc::SIGINT, stop as *const () as libc::sighandler_t);
    }
    while !STOP.load(Ordering::Relaxed) {
        let now = elapsed(origin);
        let observation = shared
            .lock()
            .map_err(|_| io::Error::other("observer state"))?
            .observations
            .current(now);
        // Reconcile startup modem state even before an observer is available.
        // Session waits after clears; only authenticated observations can send
        // switch state or replay positive connectivity.
        for client in &mut clients {
            client.step(node, observation.as_ref(), now)?;
        }
        let mut s = shared
            .lock()
            .map_err(|_| io::Error::other("observer state"))?;
        for (index, client) in clients.iter().enumerate() {
            let d = client.state.diagnostics();
            // An absent endpoint must not look like an active bind transaction.
            let stage = if client.endpoint.is_none() {
                -1
            } else {
                d.stage
            };

            if index == 0 {
                s.status.primaryStage = stage;
                s.status.primaryOperation = i32::from(d.operation);
                s.status.primaryError = d.error;
                s.status.primaryKeepaliveError = client.state.keepalive_diagnostics().error;
                s.status.primaryProfileError = client.state.profile_report_diagnostics().error;
            } else {
                s.status.secondaryStage = stage;
                s.status.secondaryOperation = i32::from(d.operation);
                s.status.secondaryError = d.error;
                s.status.secondaryKeepaliveError = client.state.keepalive_diagnostics().error;
                s.status.secondaryProfileError = client.state.profile_report_diagnostics().error;
            }
        }
        drop(s);
        std::thread::sleep(Duration::from_millis(REPORT_LOOP_PERIOD_MS));
    }
    let current = shared
        .lock()
        .map_err(|_| io::Error::other("observer state"))?
        .observations
        .current(elapsed(origin));
    if let Some(current) = current {
        let withdrawn = Observation {
            enabled: current.enabled,
            network: None,
        };
        let deadline = elapsed(origin).saturating_add(SHUTDOWN_WITHDRAWAL_BUDGET_MS);
        while elapsed(origin) < deadline {
            for client in &mut clients {
                client.step(node, Some(&withdrawn), elapsed(origin))?;
            }
            if clients.iter().all(|c| c.state.settled()) {
                break;
            }
            std::thread::sleep(Duration::from_millis(SHUTDOWN_POLL_PERIOD_MS));
        }
    }
    Ok(())
}
fn main() {
    if run().is_err() {
        eprintln!("wlanreportd: stopped after operational failure");
        std::process::exit(1);
    }
}
