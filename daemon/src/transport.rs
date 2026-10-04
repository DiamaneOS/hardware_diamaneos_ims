// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! Error categories established against the pinned Qualcomm QRTR kernel paths.
use std::io;
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    Retry,
    PeerGone,
    SocketReset,
    Unexpected,
}
pub fn classify(error: &io::Error) -> Fault {
    match error.raw_os_error() {
        Some(libc::EAGAIN | libc::EINTR) => Fault::Retry,
        Some(libc::EPIPE | libc::ENODEV | libc::ECONNRESET) => Fault::PeerGone,
        Some(libc::ENETRESET) => Fault::SocketReset,
        _ => Fault::Unexpected,
    }
}
