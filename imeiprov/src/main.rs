// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! imeiprovd: one-shot IMEI provisioning for DiamaneOS on the Fairphone 6.
//!
//! Reads the two IMEIs from the read-only `traceability` partition, validates
//! them, and compares them with the modem's current NV value. With `--write` it
//! provisions the modem's NV item 550 over the TCL QMI service and verifies by
//! reading back. `--write` publishes its outcome in [`STATUS_PROPERTY`], so the
//! phone process can offer a restart after a first write. It opens no non-QRTR
//! sockets (besides the log and property sockets), writes no MAC or region data
//! and no persistent file, and logs only booleans and counters -- never an
//! identifier.
//!
//! Protocol and partition facts are documented in `src/message.rs`, `src/imei.rs`
//! and `src/trace.rs` (reverse-engineered from stock `tctd`).

mod client;
mod seccomp;
mod trace;

use client::Client;
use diamaneos_imeiprov::imei::{check_pair, Imei};
use diamaneos_imeiprov::message;
use diamaneos_imeiprov::provision::{self, Modem, Order, Status, Written};
use std::time::Duration;

/// QRTR service id of the Fairphone TCL QMI service (TCT_QMI), from the stock
/// service object at 0x13e30.
const SERVICE_ID: u32 = 0x2ff;
/// Stock does not filter by instance. 0 matches the modem's advertised service.
const SERVICE_INSTANCE: u32 = 0;
/// Dedicated vendor UID (config.fs). Root is allowed only for lab dry-runs.
const EXPECTED_UID: u32 = 2994;

/// `--write` outcome ([`Status`]). Only this domain sets it and the phone
/// process reads it.
const STATUS_PROPERTY: &str = "vendor.diamaneos.imei.status";

const LOOKUP_TIMEOUT: Duration = Duration::from_secs(60);
const RPC_TIMEOUT: Duration = Duration::from_secs(2);
const RPC_TRIES: u32 = 3;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Check,
    Write,
}

fn main() {
    std::process::exit(run());
}

fn run() -> i32 {
    let (mode, modem_node) = match parse_mode() {
        Some(m) => m,
        None => {
            error("usage: imeiprovd --check NODE | --write NODE");
            return 2;
        }
    };

    // SAFETY: getuid has no arguments and no side effects.
    let uid = unsafe { libc::getuid() };
    if uid != EXPECTED_UID && uid != 0 {
        error("refusing to run under an unexpected uid");
        return 1;
    }
    if !selinux_enforcing() {
        error("refusing to run without enforcing SELinux");
        return 1;
    }

    let Some(modem_node) = modem_node else {
        return 2;
    };
    let report = (mode == Mode::Write).then(StatusReport::start);

    // Read and validate the traceability IMEIs (read-only).
    let (slot1, slot2) = match trace::read_imeis(trace::PATH) {
        Ok(pair) => pair,
        Err(trace::Error::Io(e)) => {
            error(&format!("traceability read failed: {:?}", e.kind()));
            return 1;
        }
        Err(trace::Error::Decode(e)) => {
            // The error is a bare variant (Length, NotDigit, ...) and carries no digits.
            error(&format!("traceability IMEI invalid: {e:?}"));
            return 1;
        }
    };
    let pair = check_pair(&slot1, &slot2);
    info(&format!(
        "trace: slot1_valid={} slot2_valid={} distinct={}",
        pair.slot1_valid, pair.slot2_valid, pair.distinct
    ));

    // Confine the process before any modem communication.
    if seccomp::install().is_err() {
        warn("seccomp filter not installed; continuing confined by SELinux only");
    }

    let client = match Client::bind() {
        Ok(c) => c,
        Err(_) => {
            error("QRTR bind failed");
            return 1;
        }
    };
    let server = match client.lookup(SERVICE_ID, SERVICE_INSTANCE, modem_node, LOOKUP_TIMEOUT) {
        Ok(Some(peer)) => peer,
        Ok(None) => {
            error("TCT QMI service not found");
            return 1;
        }
        Err(_) => {
            error("QRTR lookup failed");
            return 1;
        }
    };

    let mut modem = TclModem {
        client: &client,
        server,
        txn: 1,
    };
    if mode == Mode::Write && pair.usable() {
        let run = match provision::write_run(&mut modem, &slot1, &slot2) {
            Ok(run) => run,
            Err(e) => return read_failed(&e),
        };
        log_modem(run.before, &slot1, &slot2);
        if let Order::Fallback(reversed) = run.order {
            log_written("reversed write", &reversed);
        }
        info(&format!("order: {}", run.order.as_str()));
        log_written("write", &run.written);
        if let Some(report) = report {
            report.finish(Status::of(&run, &slot1));
        }
        return if run.written.ok() { 0 } else { 1 };
    }
    match modem.read() {
        Ok(before) => log_modem(before, &slot1, &slot2),
        Err(e) => return read_failed(&e),
    }
    if mode == Mode::Check {
        return 0;
    }
    error("write refused: traceability IMEIs did not validate");
    1
}

/// Sets [`STATUS_PROPERTY`] to `running`, then to the result, or to `failed`
/// when the run ends without one.
struct StatusReport {
    reported: bool,
}

impl StatusReport {
    fn start() -> Self {
        report_status(Status::Running);
        Self { reported: false }
    }

    fn finish(mut self, status: Status) {
        report_status(status);
        self.reported = true;
    }
}

impl Drop for StatusReport {
    fn drop(&mut self) {
        if !self.reported {
            report_status(Status::Failed);
        }
    }
}

fn report_status(status: Status) {
    if set_property(STATUS_PROPERTY, status.as_str()) {
        info(&format!("status: {}", status.as_str()));
    } else {
        error("status: property not set");
    }
}

fn log_modem(value: Option<Imei>, slot1: &Imei, slot2: &Imei) {
    match value {
        Some(m) => info(&format!(
            "modem: read_ok=true provisioned=true luhn_valid={} matches_slot1={} matches_slot2={}",
            m.is_luhn_valid(),
            m == *slot1,
            m == *slot2
        )),
        None => info("modem: read_ok=true provisioned=false"),
    }
}

fn log_written(what: &str, written: &Written) {
    info(&format!(
        "{what}: sub0_ok={} sub1_ok={} verified={}",
        written.sub0_ok, written.sub1_ok, written.verified
    ));
}

fn read_failed(e: &ReadFail) -> i32 {
    error(&format!("modem: read_ok=false reason={e}"));
    1
}

/// NV 550 over the TCL QMI service.
struct TclModem<'a> {
    client: &'a Client,
    server: client::Peer,
    txn: u16,
}

impl Modem for TclModem<'_> {
    type ReadError = ReadFail;

    fn read(&mut self) -> Result<Option<Imei>, ReadFail> {
        read_modem_imei(self.client, self.server, &mut self.txn)
    }

    fn write(&mut self, subscription: u8, imei: &Imei) -> bool {
        write_slot(self.client, self.server, imei, subscription, &mut self.txn)
    }
}

/// The IMEI modes take the modem's QRTR node (ro.vendor.diamaneos.ims.modem_node,
/// which imeiprovd.rc passes): only that node may serve the TCL QMI service.
fn parse_mode() -> Option<(Mode, Option<u32>)> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [a, node] if a == "--check" => Some((Mode::Check, Some(node.parse().ok()?))),
        [a, node] if a == "--write" => Some((Mode::Write, Some(node.parse().ok()?))),
        _ => None,
    }
}

/// Why a modem NV read failed. Carries error kinds and NV status codes only,
/// never NV contents.
enum ReadFail {
    Build,
    Transport(std::io::ErrorKind),
    NoReply,
    Parse(message::MessageError),
}

impl std::fmt::Display for ReadFail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReadFail::Build => f.write_str("build"),
            ReadFail::Transport(kind) => write!(f, "transport:{kind:?}"),
            ReadFail::NoReply => f.write_str("no_reply"),
            ReadFail::Parse(e) => write!(f, "parse:{e:?}"),
        }
    }
}

/// Read NV 550. `Ok(None)` means the item was never written.
fn read_modem_imei(
    client: &Client,
    server: client::Peer,
    txn: &mut u16,
) -> Result<Option<Imei>, ReadFail> {
    let id = next_txn(txn);
    let request = message::build_imei_read_request(id).map_err(|_| ReadFail::Build)?;
    let response = client
        .transact(server, &request, RPC_TIMEOUT, RPC_TRIES)
        .map_err(|e| ReadFail::Transport(e.kind()))?
        .ok_or(ReadFail::NoReply)?;
    message::parse_imei_read_response(&response, id)
        .map_err(ReadFail::Parse)
}

fn write_slot(client: &Client, server: client::Peer, imei: &Imei, sub: u8, txn: &mut u16) -> bool {
    let id = next_txn(txn);
    let Ok(request) = message::build_imei_write_request(id, imei, sub) else {
        error(&format!("write sub{sub}: request not built"));
        return false;
    };
    match client.transact(server, &request, RPC_TIMEOUT, RPC_TRIES) {
        Ok(Some(response)) => match message::parse_write_response(&response, id) {
            Ok(()) => true,
            Err(e) => {
                error(&format!("write sub{sub}: {e:?}"));
                false
            }
        },
        Ok(None) => {
            error(&format!("write sub{sub}: no reply"));
            false
        }
        Err(e) => {
            error(&format!("write sub{sub}: transport {:?}", e.kind()));
            false
        }
    }
}

fn next_txn(txn: &mut u16) -> u16 {
    let id = *txn;
    *txn = txn.wrapping_add(1).max(1);
    id
}

fn selinux_enforcing() -> bool {
    std::fs::read_to_string("/sys/fs/selinux/enforce")
        .map(|s| s.trim() == "1")
        .unwrap_or(false)
}

#[cfg(target_os = "android")]
fn set_property(name: &str, value: &str) -> bool {
    use std::ffi::CString;
    extern "C" {
        fn __system_property_set(
            name: *const libc::c_char,
            value: *const libc::c_char,
        ) -> libc::c_int;
    }
    let (Ok(name), Ok(value)) = (CString::new(name), CString::new(value)) else {
        return false;
    };
    // SAFETY: both pointers reference live NUL-terminated C strings for the
    // duration of the synchronous call. bionic copies them into its request.
    unsafe { __system_property_set(name.as_ptr(), value.as_ptr()) == 0 }
}

#[cfg(not(target_os = "android"))]
fn set_property(_name: &str, _value: &str) -> bool {
    false
}

fn info(msg: &str) {
    emit(4, msg);
}
fn warn(msg: &str) {
    emit(5, msg);
}
fn error(msg: &str) {
    emit(6, msg);
}

#[cfg(target_os = "android")]
fn emit(priority: i32, msg: &str) {
    use std::ffi::CString;
    extern "C" {
        fn __android_log_write(
            prio: libc::c_int,
            tag: *const libc::c_char,
            text: *const libc::c_char,
        ) -> libc::c_int;
    }
    if let Ok(text) = CString::new(msg) {
        // SAFETY: both pointers reference live NUL-terminated C strings for the
        // duration of the synchronous call. liblog copies the text.
        unsafe {
            __android_log_write(
                priority as libc::c_int,
                c"imeiprovd".as_ptr(),
                text.as_ptr(),
            );
        }
    }
}

#[cfg(not(target_os = "android"))]
fn emit(_priority: i32, msg: &str) {
    eprintln!("imeiprovd: {msg}");
}
