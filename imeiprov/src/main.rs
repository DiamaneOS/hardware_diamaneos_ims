// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! imeiprovd: one-shot IMEI provisioning for DiamaneOS on the Fairphone 6.
//!
//! Reads the two IMEIs from the read-only `traceability` partition, validates
//! them, and compares them with the modem's current NV value. With `--write` it
//! provisions the modem's NV item 550 over the TCL QMI service and verifies by
//! reading back. With `--bt-address` it instead reads the factory Bluetooth
//! address from the same partition, validates it and sets it in one property
//! (`ro.vendor.diamaneos.bt.factory_address`), which only this domain may set;
//! the IMEI modes set no property. With `--wlan-mac` it reads the factory Wi-Fi
//! MAC, validates it and writes the WLAN driver's MAC file to a RAM-backed path
//! only this domain may write ([`WLAN_MAC_FILE`]). It opens no non-QRTR sockets
//! (besides the log and property sockets), writes no region data and no
//! persistent file, and logs only booleans and counters -- never an identifier.
//!
//! Protocol and partition facts are documented in `src/message.rs`, `src/imei.rs`,
//! `src/bdaddr.rs`, `src/wlanmac.rs` and `src/trace.rs` (reverse-engineered from
//! stock `tctd` and `setwlanmac.sh`).

mod client;
mod seccomp;
mod trace;

use client::Client;
use diamaneos_imeiprov::bdaddr::BdAddr;
use diamaneos_imeiprov::imei::{check_pair, Imei};
use diamaneos_imeiprov::message;
use diamaneos_imeiprov::wlanmac::WlanMac;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::time::Duration;

/// QRTR service id of the Fairphone TCL QMI service (TCT_QMI), from the stock
/// service object at 0x13e30.
const SERVICE_ID: u32 = 0x2ff;
/// Stock does not filter by instance; 0 matches the modem's advertised service.
const SERVICE_INSTANCE: u32 = 0;
/// Dedicated vendor UID (config.fs). Root is allowed only for lab dry-runs.
const EXPECTED_UID: u32 = 2994;

/// Set once per boot by `--bt-address` (ro.: init refuses any later change).
/// The device's Bluetooth init rc copies it to the property its HCI
/// implementation reads.
const BT_ADDRESS_PROPERTY: &str = "ro.vendor.diamaneos.bt.factory_address";

/// Written once per boot by `--wlan-mac`. The tree is in RAM (tmpfs), created
/// by init before this runs (`imeiprovd.rc`); the device's ueventd.rc lists
/// `/mnt/vendor/wlan_mac/` as a firmware directory, so ueventd serves this file
/// when the driver asks for `wlan/qca_cld/qca6750/wlan_mac.bin`.
const WLAN_MAC_FILE: &str = "/mnt/vendor/wlan_mac/wlan/qca_cld/qca6750/wlan_mac.bin";

const LOOKUP_TIMEOUT: Duration = Duration::from_secs(60);
const RPC_TIMEOUT: Duration = Duration::from_secs(2);
const RPC_TRIES: u32 = 3;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Check,
    Write,
    BtAddress,
    WlanMac,
}

fn main() {
    std::process::exit(run());
}

fn run() -> i32 {
    let mode = match parse_mode() {
        Some(m) => m,
        None => {
            error("usage: imeiprovd --check | --write | --bt-address | --wlan-mac");
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

    if mode == Mode::BtAddress {
        return provide_bt_address();
    }
    if mode == Mode::WlanMac {
        return provide_wlan_mac();
    }

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
    match read_modem_imei(&client, server, &mut txn) {
        Ok(Some(m)) => info(&format!(
            "modem: read_ok=true provisioned=true luhn_valid={} matches_slot1={} matches_slot2={}",
            m.is_luhn_valid(),
            m == slot1,
            m == slot2
        )),
        Ok(None) => info("modem: read_ok=true provisioned=false"),
        Err(e) => {
            error(&format!("modem: read_ok=false reason={e}"));
            return 1;
        }
    }

    if mode == Mode::Check {
        return 0;
    }

    // --- write path ---
    if !pair.usable() {
        error("write refused: traceability IMEIs did not validate");
        return 1;
    }
    // Only write after a successful read. Write both subscriptions even when NV
    // 550 already matches slot 1: the stock read message carries no subscription
    // selector, so it cannot show slot 2, and a boot whose slot 2 write failed
    // would otherwise never retry it. Stock tctd also writes both at every boot;
    // the write is idempotent. Slot 2 is confirmed on the phone (*#06#).
    let sub0_ok = write_slot(&client, server, &slot1, 0, &mut txn);
    let sub1_ok = write_slot(&client, server, &slot2, 1, &mut txn);
    let verify_ok = matches!(read_modem_imei(&client, server, &mut txn), Ok(Some(m)) if m == slot1);
    info(&format!(
        "write: sub0_ok={sub0_ok} sub1_ok={sub1_ok} verify_slot1={verify_ok}"
    ));
    if sub0_ok && sub1_ok && verify_ok {
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
        [a] if a == "--bt-address" => Some(Mode::BtAddress),
        [a] if a == "--wlan-mac" => Some(Mode::WlanMac),
        _ => None,
    }
}

/// `--bt-address`: read the factory Bluetooth address (6 bytes, read-only),
/// validate it and set [`BT_ADDRESS_PROPERTY`]. If it is missing or invalid,
/// nothing is set and the HCI implementation keeps its stored or generated
/// address, as before. Logs the outcome only, never the address. No seccomp
/// filter: the process reads one window of a root-owned partition, makes one
/// property call and exits; SELinux confines it.
fn provide_bt_address() -> i32 {
    let Ok(raw) = trace::read_bt_address(trace::PATH) else {
        warn("bt address: missing");
        return 1;
    };
    let Ok(address) = BdAddr::from_trace(&raw) else {
        warn("bt address: invalid");
        return 1;
    };
    if set_property(BT_ADDRESS_PROPERTY, &address.property_value()) {
        info("bt address: set");
        0
    } else {
        error("bt address: property not set");
        1
    }
}

/// `--wlan-mac`: read the factory Wi-Fi MAC (6 bytes, read-only), validate it
/// and write the driver's MAC file ([`WLAN_MAC_FILE`]). If it is missing or
/// invalid, no file is written and the driver uses its firmware's address, as
/// before. The file is created new (never replaced, never through a link),
/// readable by its owner only (ueventd reads it with its own capabilities) and
/// never read back here. Logs the outcome only, never the MAC. No seccomp
/// filter, as for `--bt-address`; SELinux confines it.
fn provide_wlan_mac() -> i32 {
    let Ok(raw) = trace::read_wlan_mac(trace::PATH) else {
        warn("wlan mac: missing");
        return 1;
    };
    let Ok(mac) = WlanMac::from_trace(&raw) else {
        warn("wlan mac: invalid");
        return 1;
    };
    match write_new_file(WLAN_MAC_FILE, mac.mac_file().as_bytes()) {
        Ok(()) => {
            info("wlan mac: written");
            0
        }
        Err(e) => {
            error(&format!("wlan mac: file not written: {:?}", e.kind()));
            1
        }
    }
}

/// Create `path` (it must not exist; O_EXCL also refuses a symbolic link),
/// mode 0400, and write `contents`.
fn write_new_file(path: &str, contents: &[u8]) -> std::io::Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o400)
        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
        .open(path)?;
    file.write_all(contents)
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
    // duration of the synchronous call; bionic copies them into its request.
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
