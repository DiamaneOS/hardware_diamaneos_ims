// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! imeiprovd: one-shot IMEI provisioning for DiamaneOS on the Fairphone 6.
//!
//! Reads the two IMEIs from the read-only `traceability` partition, validates
//! them, and compares them with the modem's current NV value. With `--write` it
//! provisions the modem's NV item 550 over the TCL QMI service and verifies by
//! reading back. It sets no properties, opens no non-QRTR sockets, writes no MAC
//! or region data, and logs only booleans and counters -- never an identifier.
//!
//! Protocol and partition facts are documented in `src/message.rs`, `src/imei.rs`
//! and `src/trace.rs` (reverse-engineered from stock `tctd`).

mod client;
mod seccomp;
mod trace;

use client::Client;
use diamaneos_imeiprov::imei::{check_pair, Imei};
use diamaneos_imeiprov::message;
use std::time::Duration;

/// QRTR service id of the Fairphone TCL QMI service (TCT_QMI), from the stock
/// service object at 0x13e30.
const SERVICE_ID: u32 = 0x2ff;
/// Stock does not filter by instance; 0 matches the modem's advertised service.
const SERVICE_INSTANCE: u32 = 0;
/// Dedicated vendor UID (config.fs). Root is allowed only for lab dry-runs.
const EXPECTED_UID: u32 = 2992;

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
    let mode = match parse_mode() {
        Some(m) => m,
        None => {
            error("usage: imeiprovd --check | --write");
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

    // Read and validate the traceability IMEIs (read-only).
    let (slot1, slot2) = match trace::read_imeis(trace::PATH) {
        Ok(pair) => pair,
        Err(trace::Error::Io(_)) => {
            error("traceability read failed");
            return 1;
        }
        Err(trace::Error::Decode(_)) => {
            error("traceability IMEI not 15 ASCII digits");
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
    let server = match client.lookup(SERVICE_ID, SERVICE_INSTANCE, LOOKUP_TIMEOUT) {
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

    let mut txn: u16 = 1;
    let modem = read_modem_imei(&client, server, &mut txn);
    match &modem {
        Some(m) => info(&format!(
            "modem: read_ok=true luhn_valid={} matches_slot1={} matches_slot2={}",
            m.is_luhn_valid(),
            *m == slot1,
            *m == slot2
        )),
        None => info("modem: read_ok=false"),
    }

    if mode == Mode::Check {
        return 0;
    }

    // --- write path ---
    if !pair.usable() {
        error("write refused: traceability IMEIs did not validate");
        return 1;
    }
    // Only write when the readable (default-subscription) value differs or could
    // not be read. The stock read message carries no subscription selector, so
    // slot 2 is provisioned alongside slot 1 and confirmed on the phone (*#06#).
    let need_write = !matches!(&modem, Some(m) if *m == slot1);
    if !need_write {
        info("write: nothing to do (slot1 already matches modem)");
        return 0;
    }

    let sub0_ok = write_slot(&client, server, &slot1, 0, &mut txn);
    let sub1_ok = write_slot(&client, server, &slot2, 1, &mut txn);
    let verify_ok = matches!(read_modem_imei(&client, server, &mut txn), Some(m) if m == slot1);
    info(&format!(
        "write: sub0_ok={sub0_ok} sub1_ok={sub1_ok} verify_slot1={verify_ok}"
    ));
    if sub0_ok && verify_ok {
        0
    } else {
        1
    }
}

fn parse_mode() -> Option<Mode> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [a] if a == "--check" => Some(Mode::Check),
        [a] if a == "--write" => Some(Mode::Write),
        _ => None,
    }
}

fn read_modem_imei(client: &Client, server: client::Peer, txn: &mut u16) -> Option<Imei> {
    let id = next_txn(txn);
    let request = message::build_imei_read_request(id).ok()?;
    let response = client
        .transact(server, &request, RPC_TIMEOUT, RPC_TRIES)
        .ok()??;
    message::parse_imei_read_response(&response, id).ok()
}

fn write_slot(client: &Client, server: client::Peer, imei: &Imei, sub: u8, txn: &mut u16) -> bool {
    let id = next_txn(txn);
    let Ok(request) = message::build_imei_write_request(id, imei, sub) else {
        return false;
    };
    let Ok(Some(response)) = client.transact(server, &request, RPC_TIMEOUT, RPC_TRIES) else {
        return false;
    };
    message::parse_write_response(&response, id).is_ok()
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
        // duration of the synchronous call; liblog copies the text.
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
