// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
#![forbid(unsafe_code)]
pub mod engine;
pub mod protocol;

pub mod qrtr;

/// Deferred-output ownership tokens; only Engine can create their fences.
pub mod outgoing;
