// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! Read-only access to the Fairphone `traceability` partition.
//!
//! The partition stores each IMEI as 15 ASCII digits. Stock `tctd` (`start`
//! @0xbb18-0xbb58) reads slot 1 at offset 0x24 and slot 2 at offset 0x263, each
//! 15 bytes. This module opens the block device O_RDONLY and reads only those
//! two windows; it never reads the region/CU byte (0x19d) or the MAC fields, and
//! it never writes.

use diamaneos_imeiprov::imei::{Error as ImeiError, Imei};
use std::fs::File;
use std::io;
use std::os::unix::fs::{FileExt, OpenOptionsExt};

/// `bootdevice` symlink path, as used by stock `tctd`.
pub const PATH: &str = "/dev/block/bootdevice/by-name/traceability";

const SLOT1_OFFSET: u64 = 0x24;
const SLOT2_OFFSET: u64 = 0x263;
const ASCII_LEN: usize = 15;

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Decode(ImeiError),
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}

/// Open read-only and decode both IMEIs. The two 15-byte ASCII windows are the
/// only bytes read.
pub fn read_imeis(path: &str) -> Result<(Imei, Imei), Error> {
    // read(true) opens O_RDONLY; O_CLOEXEC avoids leaking the fd. No write mode.
    let file = File::options()
        .read(true)
        .custom_flags(libc::O_CLOEXEC)
        .open(path)?;
    let slot1 = read_one(&file, SLOT1_OFFSET)?;
    let slot2 = read_one(&file, SLOT2_OFFSET)?;
    Ok((slot1, slot2))
}

fn read_one(file: &File, offset: u64) -> Result<Imei, Error> {
    let mut buf = [0u8; ASCII_LEN];
    file.read_exact_at(&mut buf, offset)?;
    Imei::from_ascii(&buf).map_err(Error::Decode)
}
