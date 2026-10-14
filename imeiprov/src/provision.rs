// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! The `--write` run over an abstract modem, so host tests can drive it with a
//! fake one.

use crate::imei::Imei;

/// NV 550 on the modem.
pub trait Modem {
    type ReadError;
    /// `Ok(None)`: never written. The read has no subscription selector and
    /// shows subscription 0 only.
    fn read(&mut self) -> Result<Option<Imei>, Self::ReadError>;
    /// Writes one subscription (0 or 1). True if the modem reports it done.
    fn write(&mut self, subscription: u8, imei: &Imei) -> bool;
}

/// What a write run did, as logged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Written {
    pub sub0_ok: bool,
    pub sub1_ok: bool,
    pub verify_slot1: bool,
}

impl Written {
    pub fn ok(&self) -> bool {
        self.sub0_ok && self.sub1_ok && self.verify_slot1
    }
}

/// Reads NV 550, then, only if that read worked, writes both subscriptions
/// and reads subscription 0 back. Returns the first read and what was written.
///
/// Both are written whatever the first read shows, as stock tctd does at every
/// boot. The read cannot show subscription 1, so this is how a failed write of
/// it is retried on the next boot even when subscription 0 already matches.
pub fn write_run<M: Modem>(
    modem: &mut M,
    slot1: &Imei,
    slot2: &Imei,
) -> Result<(Option<Imei>, Written), M::ReadError> {
    let before = modem.read()?;
    let sub0_ok = modem.write(0, slot1);
    let sub1_ok = modem.write(1, slot2);
    let verify_slot1 = matches!(modem.read(), Ok(Some(m)) if m == *slot1);
    Ok((
        before,
        Written {
            sub0_ok,
            sub1_ok,
            verify_slot1,
        },
    ))
}
