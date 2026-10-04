// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
#![deny(unsafe_op_in_unsafe_fn)]
pub mod clock;
#[cfg(any(target_os = "linux", target_os = "android"))]
pub mod seccomp;
#[cfg(any(target_os = "linux", target_os = "android"))]
pub mod socket;

/// Engine-fenced submission ownership; Android adapter integrates it explicitly.
pub mod outbox;

/// Classified QRTR transport outcomes; unknown errors remain fail-closed.
pub mod transport;
