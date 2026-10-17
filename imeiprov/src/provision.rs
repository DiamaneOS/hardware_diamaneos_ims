// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! The `--write` run over an abstract modem, so host tests can drive it with a
//! fake one.
//!
//! Slot order: on a boot that starts from the modem's golden file system
//! (clean install, every slot switch) the modem serves subscription 0 on SIM
//! slot 1. On every later boot it serves subscription 1 there. Stock writes
//! `slot1` (traceability 0x24) to subscription 0, which gives the box order
//! only on later boots. So a first write stores the pair reversed and the next
//! boot writes it back.

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
    /// Subscription 0 read back as written.
    pub verified: bool,
}

impl Written {
    pub fn ok(&self) -> bool {
        self.sub0_ok && self.sub1_ok && self.verified
    }
}

/// Which pair a run wrote, logged as `order:`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    /// Subscription 0 had no IMEI, so this boot started from the golden file
    /// system. `slot2` went to subscription 0, so the SIM slots show the box
    /// order now.
    Reversed,
    /// Subscription 0 held `slot2`: an earlier boot wrote the reversed pair.
    /// Written back in the normal order, subscription 1 first, so subscription
    /// 0 keeps that marker until both are written.
    Corrected,
    /// The normal pair, as stock writes it at every boot.
    Normal,
    /// The reversed write failed, so the normal pair was written instead.
    Fallback(Written),
}

impl Order {
    pub fn as_str(self) -> &'static str {
        match self {
            Order::Reversed => "reversed",
            Order::Corrected => "corrected",
            Order::Normal => "normal",
            Order::Fallback(_) => "normal after a failed reversed write",
        }
    }
}

/// A `--write` run: the first read, the pair chosen and what was written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Run {
    pub before: Option<Imei>,
    pub order: Order,
    pub written: Written,
}

/// The `--write` outcome, published as `vendor.diamaneos.imei.status`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Running,
    /// The modem took the normal pair although it did not hold it before.
    /// After a clean install or a slot switch it pairs them with the SIM slots
    /// the other way round until the phone restarts.
    Written,
    Unchanged,
    Failed,
    /// First write, reversed pair ([`Order::Reversed`]). Box order now. The
    /// next boot corrects it.
    Reversed,
    /// The reversed pair from an earlier boot was put back in the normal order
    /// ([`Order::Corrected`]).
    Corrected,
}

impl Status {
    pub fn of(run: &Run, slot1: &Imei) -> Self {
        if !run.written.ok() {
            return Status::Failed;
        }
        match run.order {
            Order::Reversed => Status::Reversed,
            Order::Corrected => Status::Corrected,
            Order::Normal if run.before == Some(*slot1) => Status::Unchanged,
            Order::Normal | Order::Fallback(_) => Status::Written,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Status::Running => "running",
            Status::Written => "written",
            Status::Unchanged => "unchanged",
            Status::Failed => "failed",
            Status::Reversed => "reversed",
            Status::Corrected => "corrected",
        }
    }
}

/// Reads NV 550, then, only if that read worked, writes both subscriptions
/// and reads subscription 0 back.
///
/// Both are written whatever the first read shows, as stock tctd does at every
/// boot. The read cannot show subscription 1, so this is how a failed write of
/// it is retried on the next boot even when subscription 0 already matches.
/// Any failure in the reversed write falls back to the normal pair.
pub fn write_run<M: Modem>(
    modem: &mut M,
    slot1: &Imei,
    slot2: &Imei,
) -> Result<Run, M::ReadError> {
    let before = modem.read()?;
    let (order, written) = if before.is_none() {
        let reversed = write_pair(modem, [(0, slot2), (1, slot1)]);
        if reversed.ok() {
            (Order::Reversed, reversed)
        } else {
            let normal = write_pair(modem, [(0, slot1), (1, slot2)]);
            (Order::Fallback(reversed), normal)
        }
    } else if before == Some(*slot2) {
        (Order::Corrected, write_pair(modem, [(1, slot2), (0, slot1)]))
    } else {
        (Order::Normal, write_pair(modem, [(0, slot1), (1, slot2)]))
    };
    Ok(Run {
        before,
        order,
        written,
    })
}

/// Writes both subscriptions in the given order, then reads subscription 0
/// back.
fn write_pair<M: Modem>(modem: &mut M, writes: [(u8, &Imei); 2]) -> Written {
    let mut ok = [false; 2];
    let mut sub0 = None;
    for (subscription, imei) in writes {
        ok[usize::from(subscription)] = modem.write(subscription, imei);
        if subscription == 0 {
            sub0 = Some(*imei);
        }
    }
    let verified = matches!(modem.read(), Ok(Some(m)) if Some(m) == sub0);
    Written {
        sub0_ok: ok[0],
        sub1_ok: ok[1],
        verified,
    }
}
