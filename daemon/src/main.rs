// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! Android adapter. The pure state machine and wire codec live in dcm/.
use binder::{
    BinderFeatures, DeathRecipient, ExceptionCode, IBinder, Interface, ProcessState, Status,
    Strong, ThreadState,
};
use diamaneos_ims_dcm::{
    engine::{Diagnostics, Effect, Engine, Key, Network, Request},
    protocol::PdnType as Kind,
    qrtr::*,
};
use diamaneos_ims_runtime::socket::Qrtr;
use std::{
    ffi::CStr,
    io,
    net::{Ipv4Addr, Ipv6Addr},
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        mpsc::{sync_channel, SyncSender, TryRecvError},
        Arc, Mutex,
    },
    time::Duration,
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
fn effects(
    socket: &Qrtr,
    broker: Option<&Strong<dyn IPdnBroker>>,
    out: Vec<Effect>,
) -> io::Result<()> {
    for e in out {
        match e {
            Effect::Send(peer, bytes) => socket.send(peer, &bytes)?,
            Effect::ReadTime {
                peer,
                txn,
                pdp,
                sequence,
            } => {
                let reply = diamaneos_ims_runtime::clock::now()
                    .ok()
                    .and_then(|t| {
                        diamaneos_ims_dcm::protocol::timezone_response(
                            txn,
                            pdp,
                            sequence,
                            t.fields,
                            t.utc_seconds,
                        )
                        .ok()
                    })
                    .unwrap_or_else(|| diamaneos_ims_dcm::protocol::response(txn, 0x32, 1, 0));
                socket.send(peer, &reply)?;
            }
            Effect::BringUp(r) => {
                if let Some(b) = broker {
                    b.bringUp(&request(r))
                        .map_err(|_| io::Error::other("broker unavailable"))?;
                }
            }
            Effect::Release(r) => {
                if let Some(b) = broker {
                    b.release(&request(r))
                        .map_err(|_| io::Error::other("broker unavailable"))?;
                }
            }
        }
    }
    Ok(())
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
    let node = number("ro.vendor.diamaneos.ims.modem_node")?;
    let slots = number("ro.vendor.diamaneos.ims.slots")?;
    let mut engine = Engine::new(node, slots).map_err(io::Error::other)?;
    let mut socket = Qrtr::bind()?;
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
    // Read-only build policy. A missing/misspelled policy is a configuration error,
    // not a silently degraded emergency path.
    if prop("ro.vendor.diamaneos.ims.emergency_pdn").as_deref() != Some("serve") {
        return Err(io::Error::other("emergency policy must be serve"));
    }
    socket.publish()?;
    // SAFETY: handler only stores an atomic flag; no allocation or I/O in signals.
    unsafe {
        libc::signal(libc::SIGINT, stop as *const () as libc::sighandler_t);
        libc::signal(libc::SIGTERM, stop as *const () as libc::sighandler_t);
        libc::signal(libc::SIGHUP, stop as *const () as libc::sighandler_t);
    }
    let mut broker: Option<(u64, Strong<dyn IPdnBroker>, DeathRecipient)> = None;
    let mut latest_registration = 0;
    while !STOP.load(Ordering::Relaxed) {
        if overflow.load(Ordering::Acquire) {
            return Err(io::Error::other("binder queue overflow"));
        }
        if prop("persist.vendor.diamaneos.ims.dcm_kill").as_deref() == Some("1") {
            break;
        }
        let enabled =
            prop("persist.vendor.diamaneos.ims.emergency_pdn_kill").as_deref() != Some("1");
        effects(
            &socket,
            broker.as_ref().map(|b| &b.1),
            engine.set_emergency_enabled(enabled),
        )?;
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
                    if let Some((_, old, _)) = &broker {
                        effects(&socket, Some(old), engine.broker_lost())?;
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
            effects(&socket, broker.as_ref().map(|b| &b.1), out)?;
        }
        if let Some((peer, bytes)) = socket.receive(Duration::from_millis(25))? {
            let out = if peer.port == CTRL_PORT && peer.node == socket.local().node {
                match Control::decode(&bytes) {
                    Some(c) if c.conflicts_with(socket.local()) => {
                        return Err(io::Error::other("competing DCM publisher"));
                    }
                    Some(c) if c.command == DEL_CLIENT => {
                        engine.peer_gone(c.deleted_client().unwrap())
                    }
                    Some(c) if c.command == BYE => engine.node_gone(c.words[0]),
                    _ => vec![],
                }
            } else {
                engine.receive(peer, &bytes)
            };
            effects(&socket, broker.as_ref().map(|b| &b.1), out)?;
        }
        *diagnostics
            .lock()
            .map_err(|_| io::Error::other("diagnostic state"))? = engine.diagnostics();
    }
    let _ = effects(&socket, broker.as_ref().map(|b| &b.1), engine.shutdown());
    Ok(())
}
fn main() {
    if run().is_err() {
        eprintln!("imsdcmd: service stopped after an operational failure");
        std::process::exit(1);
    }
}
