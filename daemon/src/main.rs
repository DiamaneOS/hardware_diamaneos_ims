// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! Android adapter. The pure state machine and wire codec live in dcm/.
use binder::{
    BinderFeatures, DeathRecipient, ExceptionCode, IBinder, Interface, ProcessState, Status,
    Strong, ThreadState,
};
use diamaneos_ims_dcm::{
    engine::{Diagnostics, Engine, Key, Network, Request},
    protocol::PdnType as Kind,
    qrtr::*,
};
use diamaneos_ims_runtime::{
    socket::{Publication, Qrtr},
    transport::{classify, exhausted, Fault},
};
mod android_dispatch;
use android_dispatch::{Broker, Dispatch};
use std::{
    ffi::CStr,
    io,
    net::{Ipv4Addr, Ipv6Addr},
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        mpsc::{sync_channel, SyncSender, TryRecvError},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use vendor_diamaneos_hardware_imsdcm::aidl::vendor::diamaneos::hardware::imsdcm::{
    IImsDcm::{BnImsDcm, IImsDcm},
    IPdnBroker::IPdnBroker,
    PdnFailure::PdnFailure,
    PdnInfo::PdnInfo,
    PdnRequest::PdnRequest,
    PdnType::PdnType,
};

const SERVICE: &str = "vendor.diamaneos.hardware.imsdcm.IImsDcm/default";
static STOP: AtomicBool = AtomicBool::new(false);
extern "C" fn stop(_: libc::c_int) {
    STOP.store(true, Ordering::Relaxed);
}
enum Event {
    Register(u64, Strong<dyn IPdnBroker>, DeathRecipient),
    Lost(u64),
    Report(u64, Request, Option<Network>),
}
struct Service {
    sender: SyncSender<Event>,
    epoch: Arc<AtomicU64>,
    overflow: Arc<AtomicBool>,
    broker_uid: AtomicU32,
    diagnostics: Arc<Mutex<Diagnostics>>,
}
impl Interface for Service {
    fn dump(&self, writer: &mut dyn io::Write, args: &[&CStr]) -> Result<(), binder::StatusCode> {
        // Existing Binder dump transaction, no new endpoint or modem operation.
        if ThreadState::get_calling_uid() != 0 {
            return Err(binder::StatusCode::PERMISSION_DENIED);
        }
        if !args.is_empty() {
            return Err(binder::StatusCode::BAD_VALUE);
        }
        // Never hold the state lock while writing a caller-provided descriptor.
        let snapshot = *self
            .diagnostics
            .lock()
            .map_err(|_| binder::StatusCode::FAILED_TRANSACTION)?;
        writeln!(writer, "{snapshot:?}").map_err(|_| binder::StatusCode::FAILED_TRANSACTION)
    }
}
fn denied() -> Status {
    Status::new_exception(ExceptionCode::SECURITY, None)
}
fn app_uid() -> Option<u32> {
    let uid = ThreadState::get_calling_uid();
    (10000..20000).contains(&uid).then_some(uid)
}

fn key(r: &PdnRequest) -> binder::Result<Request> {
    let kind = match r.r#type {
        PdnType::IMS => Kind::Ims,
        PdnType::EMERGENCY => Kind::Emergency,
        _ => return Err(Status::new_exception(ExceptionCode::ILLEGAL_ARGUMENT, None)),
    };
    if !(-1..=2).contains(&r.slot) || r.serial <= 0 {
        return Err(Status::new_exception(ExceptionCode::ILLEGAL_ARGUMENT, None));
    }
    Ok(Request {
        key: Key { slot: r.slot, kind },
        serial: r.serial,
    })
}
impl Service {
    fn enqueue(&self, e: Event) -> binder::Result<()> {
        if self.sender.try_send(e).is_err() {
            self.overflow.store(true, Ordering::Release);
            return Err(Status::new_exception(ExceptionCode::ILLEGAL_STATE, None));
        }
        Ok(())
    }
}
impl IImsDcm for Service {
    fn setBroker(&self, broker: &Strong<dyn IPdnBroker>) -> binder::Result<()> {
        let uid = app_uid().ok_or_else(denied)?;
        if let Err(old) =
            self.broker_uid
                .compare_exchange(0, uid, Ordering::AcqRel, Ordering::Acquire)
        {
            if old != uid {
                return Err(denied());
            }
        }
        // Only one authorized application domain can register. Every registration
        // gets an epoch, so a delayed death callback cannot disconnect its successor.
        let epoch = self.epoch.fetch_add(1, Ordering::AcqRel) + 1;
        let sender = self.sender.clone();
        let overflow = self.overflow.clone();
        let mut death = DeathRecipient::new(move || {
            if sender.try_send(Event::Lost(epoch)).is_err() {
                overflow.store(true, Ordering::Release);
            }
        });
        broker.as_binder().link_to_death(&mut death)?;
        self.enqueue(Event::Register(epoch, broker.clone(), death))
    }
    fn onPdnUp(&self, r: &PdnRequest, i: &PdnInfo) -> binder::Result<()> {
        if app_uid() != Some(self.broker_uid.load(Ordering::Acquire)) {
            return Err(denied());
        }
        let r = key(r)?;
        let v4 = match i.ipv4Address.as_slice() {
            [] => None,
            b if b.len() == 4 => Some(Ipv4Addr::from(<[u8; 4]>::try_from(b).unwrap())),
            _ => return Err(Status::new_exception(ExceptionCode::ILLEGAL_ARGUMENT, None)),
        };
        let v6 = match i.ipv6Address.as_slice() {
            [] => None,
            b if b.len() == 16 => Some(Ipv6Addr::from(<[u8; 16]>::try_from(b).unwrap())),
            _ => return Err(Status::new_exception(ExceptionCode::ILLEGAL_ARGUMENT, None)),
        };
        let n = Network {
            handle: i.networkHandle as u64,
            v4,
            v6,
            mtu: i.mtu as u32,
        };
        if i.networkHandle == 0 || i.mtu < 0 || !n.valid() {
            return Err(Status::new_exception(ExceptionCode::ILLEGAL_ARGUMENT, None));
        }
        self.enqueue(Event::Report(
            self.epoch.load(Ordering::Acquire),
            r,
            Some(n),
        ))
    }
    fn onPdnDown(&self, r: &PdnRequest) -> binder::Result<()> {
        if app_uid() != Some(self.broker_uid.load(Ordering::Acquire)) {
            return Err(denied());
        }
        self.enqueue(Event::Report(
            self.epoch.load(Ordering::Acquire),
            key(r)?,
            None,
        ))
    }
    fn onPdnFailed(&self, r: &PdnRequest, _why: PdnFailure) -> binder::Result<()> {
        self.onPdnDown(r)
    }
}
fn prop(name: &str) -> Option<String> {
    rustutils::android::system_properties::read(name)
        .ok()
        .flatten()
}
fn number(name: &str) -> io::Result<u32> {
    prop(name)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| io::Error::other("missing device configuration"))
}
fn request(r: Request) -> PdnRequest {
    PdnRequest {
        slot: r.key.slot,
        r#type: if r.key.kind == Kind::Ims {
            PdnType::IMS
        } else {
            PdnType::EMERGENCY
        },
        serial: r.serial,
    }
}
fn run() -> io::Result<()> {
    if unsafe { libc::getuid() } != 2990 {
        return Err(io::Error::other("wrong daemon uid"));
    }
    // Vendor Binder does not expose requesting-SID. The broker-only SELinux
    // neverallows are the first authentication layer; never operate without them.
    if std::fs::read_to_string("/sys/fs/selinux/enforce")?.trim() != "1" {
        return Err(io::Error::other("enforcing SELinux required"));
    }
    // Validate immutable policy before advertising Binder or QRTR services.
    if prop("ro.vendor.diamaneos.ims.emergency_pdn").as_deref() != Some("serve") {
        return Err(io::Error::other("emergency policy must be serve"));
    }
    let debug_controls = prop("ro.debuggable").as_deref() == Some("1");
    let node = number("ro.vendor.diamaneos.ims.modem_node")?;
    let slots = number("ro.vendor.diamaneos.ims.slots")?;
    let mut engine = Engine::new(node, slots).map_err(io::Error::other)?;
    let mut socket = Qrtr::bind_imsdcm()?;
    if socket.local().node == node {
        return Err(io::Error::other("modem node is local"));
    }
    let (tx, rx) = sync_channel(64);
    let overflow = Arc::new(AtomicBool::new(false));
    let diagnostics = Arc::new(Mutex::new(engine.diagnostics()));
    let service = BnImsDcm::new_binder(
        Service {
            sender: tx,
            epoch: Arc::new(AtomicU64::new(0)),
            overflow: overflow.clone(),
            broker_uid: AtomicU32::new(0),
            diagnostics: diagnostics.clone(),
        },
        BinderFeatures::default(),
    );
    diamaneos_ims_runtime::seccomp::install()?;
    ProcessState::set_thread_pool_max_thread_count(2);
    ProcessState::start_thread_pool();
    binder::add_service(SERVICE, service.as_binder())
        .map_err(|_| io::Error::other("binder registration failed"))?;
    // Initial collision is degradation, not a crash or a publication race.
    let mut published = match socket.publish() {
        Ok(Publication::Ready) => true,
        Ok(Publication::Conflict) => {
            engine.publisher_conflict();
            false
        }
        Err(error)
            if classify(&error) == Fault::Retry || error.kind() == io::ErrorKind::TimedOut =>
        {
            false
        }
        Err(error) => return Err(error),
    };
    let origin = Instant::now();
    const PUBLICATION_RETRY_DELAY_MS: u64 = 30_000;
    let mut next_publication = PUBLICATION_RETRY_DELAY_MS;
    let mut dispatch = Dispatch::default();
    // SAFETY: handler only stores an atomic flag; no allocation or I/O in signals.
    unsafe {
        libc::signal(libc::SIGINT, stop as *const () as libc::sighandler_t);
        libc::signal(libc::SIGTERM, stop as *const () as libc::sighandler_t);
        libc::signal(libc::SIGHUP, stop as *const () as libc::sighandler_t);
    }
    let mut broker: Option<Broker> = None;
    let mut latest_registration = 0;
    let operation = (|| -> io::Result<()> {
        while !STOP.load(Ordering::Relaxed) {
            let now = origin.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
            if !published && now >= next_publication {
                next_publication = now.saturating_add(PUBLICATION_RETRY_DELAY_MS);
                match socket.publish() {
                    Ok(Publication::Ready) => published = true,
                    Ok(Publication::Conflict) => engine.publisher_conflict(),
                    Err(error) if exhausted(&error) => engine.transport_exhausted(),
                    Err(error)
                        if classify(&error) == Fault::Retry
                            || error.kind() == io::ErrorKind::TimedOut => {}
                    Err(error) => return Err(error),
                }
            }
            if overflow.load(Ordering::Acquire) {
                return Err(io::Error::other("binder queue overflow"));
            }
            if debug_controls && prop("vendor.diamaneos.ims.dcm_kill").as_deref() == Some("1") {
                break;
            }
            let enabled = !debug_controls
                || prop("vendor.diamaneos.ims.emergency_pdn_kill").as_deref() != Some("1");
            let out = engine.set_emergency_enabled(enabled);
            dispatch.apply(&socket, &mut engine, broker.as_ref(), out, now)?;
            // Bound work per iteration so a broker flood cannot starve QRTR or signals.
            for _ in 0..32 {
                let e = match rx.try_recv() {
                    Ok(e) => e,
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        return Err(io::Error::other("binder disconnected"))
                    }
                };
                let out = match e {
                    Event::Register(epoch, b, d) => {
                        if epoch <= latest_registration {
                            continue;
                        }
                        latest_registration = epoch;
                        if broker.is_some() {
                            let out = engine.broker_lost();
                            dispatch.apply(&socket, &mut engine, broker.as_ref(), out, now)?;
                        }
                        broker = Some((epoch, b, d));
                        engine.broker_connected()
                    }
                    Event::Lost(epoch) => {
                        latest_registration = latest_registration.max(epoch);
                        if broker.as_ref().is_some_and(|b| b.0 == epoch) {
                            broker = None;
                            engine.broker_lost()
                        } else {
                            vec![]
                        }
                    }
                    Event::Report(epoch, r, n) => {
                        if broker.as_ref().is_some_and(|b| b.0 == epoch) {
                            engine.report(r, n)
                        } else {
                            vec![]
                        }
                    }
                };
                dispatch.apply(&socket, &mut engine, broker.as_ref(), out, now)?;
            }
            let incoming = match socket.receive(Duration::from_millis(25)) {
                Ok(packet) => packet,
                Err(error) => match classify(&error) {
                    Fault::Retry => {
                        if exhausted(&error) {
                            engine.transport_exhausted();
                        }
                        None
                    }
                    Fault::PeerGone | Fault::SocketReset => {
                        // A socket-wide receive failure has no destination peer.
                        // Withdraw this modem's state before replacing the socket.
                        dispatch.purge_all(&mut engine, broker.as_ref())?;
                        let out = engine.node_gone(node);
                        dispatch.apply(&socket, &mut engine, broker.as_ref(), out, now)?;
                        socket = socket.rebind_imsdcm()?;
                        published = false;
                        next_publication = now;
                        None
                    }
                    Fault::Unexpected => return Err(error),
                },
            };
            if let Some((peer, bytes)) = incoming {
                let out = if peer.port == CTRL_PORT && peer.node == socket.local().node {
                    match Control::decode(&bytes) {
                        Some(c) if c.conflicts_with(socket.local()) => {
                            engine.publisher_conflict();
                            vec![] // Preserve live clients; ownership is a separate kernel gate.
                        }
                        Some(c) if c.command == DEL_CLIENT => {
                            let lost = c.deleted_client().unwrap();
                            dispatch.purge_peer(&mut engine, broker.as_ref(), lost)?;
                            engine.peer_gone(lost)
                        }
                        Some(c) if c.command == BYE => {
                            dispatch.purge_node(&mut engine, broker.as_ref(), c.words[0])?;
                            engine.node_gone(c.words[0])
                        }
                        _ => vec![],
                    }
                } else {
                    engine.receive(peer, &bytes)
                };
                dispatch.apply(&socket, &mut engine, broker.as_ref(), out, now)?;
            }
            // Revoke the failed broker's positive outputs before attempting a send.
            if let Some(epoch) = dispatch.broker_lost.take() {
                if broker.as_ref().is_some_and(|current| current.0 == epoch) {
                    broker = None;
                    let out = engine.broker_lost();
                    dispatch.apply(&socket, &mut engine, None, out, now)?;
                }
            }
            dispatch.flush(&socket, &mut engine, broker.as_ref(), now)?;
            if let Some(epoch) = dispatch.broker_lost.take() {
                if broker.as_ref().is_some_and(|current| current.0 == epoch) {
                    broker = None;
                    let out = engine.broker_lost();
                    dispatch.apply(&socket, &mut engine, None, out, now)?;
                }
            }
            if std::mem::take(&mut dispatch.socket_reset) {
                dispatch.purge_all(&mut engine, broker.as_ref())?;
                let out = engine.node_gone(node);
                dispatch.apply(&socket, &mut engine, broker.as_ref(), out, now)?;
                socket = socket.rebind_imsdcm()?;
                published = false;
                next_publication = now;
            }
            engine.publication_active(published);
            *diagnostics
                .lock()
                .map_err(|_| io::Error::other("diagnostic state"))? = engine.diagnostics();
        }
        Ok(())
    })();
    let cleanup = dispatch.shutdown(&mut engine, broker.as_ref());
    operation.and(cleanup)
}
fn main() {
    if run().is_err() {
        eprintln!("imsdcmd: service stopped after an operational failure");
        std::process::exit(1);
    }
}
