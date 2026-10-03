// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! Bounded offline ownership/lifetime logic. No sockets or positive quality results.
use crate::{profile_notice::Notice, request, tlv, Error as WireError, DEFAULT_PROFILE_STATUS};
use diamaneos_ims_dcm::protocol::Frame;

const MAX_ENTRIES: usize = 40;

// No Debug/Display: the opaque measurement ID must remain private.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Key {
    profile: u8,
    measurement: u64,
}

struct Entry {
    key: Key,
    revision: u64,
    selected: bool,
    reported: bool,
}

/// An owned selected entry, including a fence against destruction/reinitialization.
/// The caller must still authenticate the current modem context and matching ACK.
#[derive(Clone, Copy)]
pub struct ReportToken {
    context_generation: u64,
    key: Key,
    revision: u64,
}

impl ReportToken {
    /// Truthful unavailability, not a measured result or successful offload.
    /// Revalidate with Registry::is_current immediately before transport/retry.
    pub fn inconclusive(self, transaction: u16) -> Result<Vec<u8>, WireError> {
        let mut body = Vec::with_capacity(32);
        tlv(&mut body, 1, &u32::from(self.key.profile).to_le_bytes());
        tlv(&mut body, 0x10, &1_u32.to_le_bytes()); // QUALITY_NOT_MET
        tlv(&mut body, 0x11, &3_u32.to_le_bytes()); // CQ_FAIL_INCONCLUSIVE
        if self.key.measurement != 0 {
            tlv(&mut body, 0x12, &self.key.measurement.to_le_bytes());
        }
        request(transaction, DEFAULT_PROFILE_STATUS, body)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Malformed,
    UnmappedSelection,
    Capacity,
    RevisionExhausted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Initialized,
    Duplicate,
    Selection,
}

/// One verified QRTR subscription context. Replace the entire registry on context
/// replacement. Forty is our capacity limit, not the firmware's total key space.
pub struct Registry {
    context_generation: u64,
    entries: Vec<Entry>,
    next_revision: u64,
}

impl Registry {
    /// Runtime-owned nonzero generation, globally unique across both subscriptions
    /// and never reused for a replacement QRTR context. This is separate from the
    /// wire's opaque measurement identity. Exhaustion must stop optional reporting.
    pub fn new(context_generation: u64) -> Option<Self> {
        (context_generation != 0).then_some(Self {
            context_generation,
            entries: Vec::with_capacity(MAX_ENTRIES),
            next_revision: 1,
        })
    }
    /// Call only after endpoint/version/generation and subscription binding checks.
    /// Retains only normalized identity/lifetime; never SIM bytes or thresholds.
    pub fn receive(&mut self, packet: &[u8]) -> Result<Change, Error> {
        let notice = Notice::parse(packet).ok_or(Error::Malformed)?;
        let frame = Frame::parse(packet).map_err(|_| Error::Malformed)?;
        match notice {
            Notice::Initialized(profile) => {
                let key = Key {
                    profile,
                    measurement: measurement(&frame, 0x13)?,
                };
                if self.entries.iter().any(|entry| entry.key == key) {
                    return Ok(Change::Duplicate);
                }
                if self.entries.len() == MAX_ENTRIES {
                    return Err(Error::Capacity);
                }
                let revision = self.next_revision;
                self.next_revision = revision.checked_add(1).ok_or(Error::RevisionExhausted)?;
                self.entries.push(Entry {
                    key,
                    revision,
                    selected: false,
                    reported: false,
                });
                Ok(Change::Initialized)
            }
            Notice::Selected(selection) => {
                // Unknown bits may imply an unsupported lifecycle. Reject before
                // mutation; a runtime must stop this optional channel on this error.
                if selection.has_unmapped_bits() {
                    return Err(Error::UnmappedSelection);
                }
                let id = measurement(&frame, 0x11)?;
                self.entries.retain(|entry| {
                    entry.key.measurement != id || selection.contains(entry.key.profile)
                });
                for entry in &mut self.entries {
                    if entry.key.measurement == id {
                        entry.selected = true;
                    }
                }
                Ok(Change::Selection)
            }
        }
    }

    pub fn report_due(&self) -> Option<ReportToken> {
        self.entries
            .iter()
            .find(|entry| entry.selected && !entry.reported)
            .map(|entry| ReportToken {
                context_generation: self.context_generation,
                key: entry.key,
                revision: entry.revision,
            })
    }

    pub fn is_current(&self, token: ReportToken) -> bool {
        self.context_generation == token.context_generation
            && self.entries.iter().any(|entry| {
                entry.key == token.key
                    && entry.revision == token.revision
                    && entry.selected
                    && !entry.reported
            })
    }

    /// Only after the runtime matches the pending transaction and modem generation.
    /// An old ACK cannot mark a destroyed/reinitialized entry as reported.
    pub fn acknowledged(&mut self, token: ReportToken) -> bool {
        if self.context_generation != token.context_generation {
            return false;
        }
        let Some(entry) = self.entries.iter_mut().find(|entry| {
            entry.key == token.key
                && entry.revision == token.revision
                && entry.selected
                && !entry.reported
        }) else {
            return false;
        };
        entry.reported = true;
        true
    }
}

fn measurement(frame: &Frame<'_>, tag: u8) -> Result<u64, Error> {
    match frame.tlv(tag) {
        None => Ok(0),
        Some(bytes) => Ok(u64::from_le_bytes(
            bytes.try_into().map_err(|_| Error::Malformed)?,
        )),
    }
}
