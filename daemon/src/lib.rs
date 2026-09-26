// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
#![deny(unsafe_op_in_unsafe_fn)]
pub mod clock;
#[cfg(any(target_os = "linux", target_os = "android"))]
pub mod seccomp;
#[cfg(any(target_os = "linux", target_os = "android"))]
pub mod socket;
