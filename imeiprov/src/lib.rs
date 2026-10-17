// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! Pure IMEI decoding/validation, QMI NV message codec and the write run over an
//! abstract modem for `imeiprovd`. No I/O, no sockets, no identifiers in any log
//! or Debug output.
#![forbid(unsafe_code)]

pub mod imei;
pub mod message;
pub mod provision;
