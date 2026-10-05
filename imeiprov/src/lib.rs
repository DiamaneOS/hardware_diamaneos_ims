// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! Pure IMEI decoding/validation and QMI NV message codec for `imeiprovd`.
//! No I/O, no sockets, no identifiers in any log or Debug output.
#![forbid(unsafe_code)]

pub mod imei;
pub mod message;
