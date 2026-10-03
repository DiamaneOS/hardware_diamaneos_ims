// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! Private unavailable-measurement transport state. No positive quality policy.
use crate::{
    profiles::{Registry, ReportToken},
    response_for, Error, DEFAULT_PROFILE_STATUS,
};

const DEADLINE_MS: u64 = 2_000;
const MAX_ATTEMPTS: u8 = 3;

struct Pending {
    token: ReportToken,
    transaction: u16,
    packet: Vec<u8>,
    deadline: u64,
    attempts: u8,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Diagnostics {
    pub sent: u64,
    pub acknowledged: u64,
    pub cancelled: u64,
    /// Positive: QMI error; -1: timeout; -2: transaction exhaustion;
    /// -3: malformed notice; -4: unmapped selection; -5: capacity;
    /// -6: revision exhaustion; -7: local encoding failure.
    pub error: i32,
}

pub(crate) struct Reports {
    registry: Registry,
    pending: Option<Pending>,
    blocked: bool,
    diagnostics: Diagnostics,
}

impl Reports {
    pub(crate) fn new(context_generation: u64) -> Option<Self> {
        Some(Self {
            registry: Registry::new(context_generation)?,
            pending: None,
            blocked: false,
            diagnostics: Diagnostics::default(),
        })
    }

    pub(crate) fn diagnostics(&self) -> Diagnostics {
        self.diagnostics
    }

    fn fail(&mut self, error: i32) {
        self.pending = None;
        self.blocked = true;
        self.diagnostics.error = error;
    }

    fn cancel_obsolete(&mut self) {
        if self
            .pending
            .as_ref()
            .is_some_and(|p| !self.registry.is_current(p.token))
        {
            self.pending = None;
            self.diagnostics.cancelled = self.diagnostics.cancelled.saturating_add(1);
        }
    }

    // Called only after current endpoint/generation and subscription binding checks.
    pub(crate) fn receive(&mut self, bytes: &[u8]) -> bool {
        let notice = bytes.len() >= 5
            && bytes[0] == 4
            && matches!(u16::from_le_bytes([bytes[3], bytes[4]]), 0x3f | 0x45);
        if notice {
            if self.blocked {
                return true;
            }
            if let Err(error) = self.registry.receive(bytes) {
                use crate::profiles::Error as E;
                self.fail(match error {
                    E::Malformed => -3,
                    E::UnmappedSelection => -4,
                    E::Capacity => -5,
                    E::RevisionExhausted => -6,
                });
            }
            self.cancel_obsolete();
            return true;
        }
        let Some(pending) = &self.pending else {
            return false;
        };
        match response_for(bytes, pending.transaction, DEFAULT_PROFILE_STATUS) {
            Ok(()) => {
                let token = pending.token;
                self.pending = None;
                if self.registry.acknowledged(token) {
                    self.diagnostics.acknowledged = self.diagnostics.acknowledged.saturating_add(1);
                }
                true
            }
            Err(Error::ModemFailure(error)) => {
                self.fail(i32::from(error));
                true
            }
            _ => false,
        }
    }

    pub(crate) fn poll(&mut self, now: u64, next_transaction: &mut u16) -> Option<Vec<u8>> {
        self.cancel_obsolete();
        if self.blocked {
            return None;
        }
        if let Some(pending) = &mut self.pending {
            if now < pending.deadline {
                return None;
            }
            if pending.attempts >= MAX_ATTEMPTS {
                self.fail(-1);
                return None;
            }
            pending.attempts += 1;
            pending.deadline = now.saturating_add(DEADLINE_MS);
            return Some(pending.packet.clone());
        }
        let token = self.registry.report_due()?;
        let transaction = *next_transaction;
        let Some(next) = transaction.checked_add(1) else {
            self.fail(-2);
            return None;
        };
        let packet = match token.inconclusive(transaction) {
            Ok(packet) => packet,
            Err(_) => {
                self.fail(-7);
                return None;
            }
        };
        *next_transaction = next;
        self.pending = Some(Pending {
            token,
            transaction,
            packet: packet.clone(),
            deadline: now.saturating_add(DEADLINE_MS),
            attempts: 1,
        });
        self.diagnostics.sent = self.diagnostics.sent.saturating_add(1);
        Some(packet)
    }
}
