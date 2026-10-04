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
        // Buffer or memory exhaustion is momentary; the owner retries the same work.
        Some(libc::EAGAIN | libc::EINTR | libc::ENOBUFS | libc::ENOMEM) => Fault::Retry,
        Some(libc::EPIPE | libc::ENODEV | libc::ECONNRESET | libc::EHOSTUNREACH) => Fault::PeerGone,
        Some(libc::ENETRESET) => Fault::SocketReset,
        _ => Fault::Unexpected,
    }
}
/// Retryable exhaustion, counted separately from ordinary flow control.
pub fn exhausted(error: &io::Error) -> bool {
    matches!(error.raw_os_error(), Some(libc::ENOBUFS | libc::ENOMEM))
}
