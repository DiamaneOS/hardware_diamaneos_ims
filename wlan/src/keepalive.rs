// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! Bounded failure completions. No packet endpoints, sockets or operation success.
use crate::{
    keepalive_instruction, keepalive_operation_failed, response_for, Error,
    NAT_KEEPALIVE_OPERATION_STATUS,
};

const MAX_QUEUED: u8 = 8;
const DEADLINE_MS: u64 = 2_000;
const MAX_ATTEMPTS: u8 = 3;

struct Pending {
    transaction: u16,
    packet: Vec<u8>,
    deadline: u64,
    attempts: u8,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exhausted_transaction_space_never_wraps_or_emits_a_zero_id() {
        let mut replies = Failures::default();
        assert!(replies.receive(&[4, 0, 0, 0x41, 0, 4, 0, 1, 1, 0, 1]));
        let mut next = u16::MAX;
        assert!(replies.poll(0, &mut next).is_none());
        assert_eq!(next, u16::MAX);
        assert_eq!(replies.diagnostics().error, -2);
        assert!(replies.poll(u64::MAX, &mut next).is_none());
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Diagnostics {
    pub sent: u64,
    pub acknowledged: u64,
    pub dropped: u64,
    /// 0: no error; positive: QMI error; -1: timeout; -2: transaction exhaustion;
    /// -4: a valid instruction was discarded because the bounded queue was full.
    pub error: i32,
}

#[derive(Default)]
pub(crate) struct Failures {
    queued: u8,
    pending: Option<Pending>,
    blocked: bool,
    diagnostics: Diagnostics,
}

impl Failures {
    pub(crate) fn diagnostics(&self) -> Diagnostics {
        self.diagnostics
    }

    // Only called after runtime peer validation and successful subscription bind.
    // A queue entry contains no instruction fields; each accepted entry gets the
    // same truthful failure status, for start and stop alike.
    pub(crate) fn receive(&mut self, bytes: &[u8]) -> bool {
        if keepalive_instruction(bytes) {
            if !self.blocked && self.queued < MAX_QUEUED {
                self.queued += 1;
            } else {
                self.diagnostics.dropped = self.diagnostics.dropped.saturating_add(1);
                // Preserve an existing terminal error; overflow is a sticky
                // degradation indication until this modem context is replaced.
                if !self.blocked && self.diagnostics.error == 0 {
                    self.diagnostics.error = -4;
                }
            }
            return true;
        }
        let Some(pending) = &self.pending else {
            return false;
        };
        match response_for(bytes, pending.transaction, NAT_KEEPALIVE_OPERATION_STATUS) {
            Ok(()) => {
                self.pending = None;
                self.diagnostics.acknowledged = self.diagnostics.acknowledged.saturating_add(1);
                true
            }
            Err(Error::ModemFailure(error)) => {
                self.fail(i32::from(error));
                true
            }
            _ => false,
        }
    }

    fn fail(&mut self, error: i32) {
        self.pending = None;
        self.diagnostics.dropped = self
            .diagnostics
            .dropped
            .saturating_add(u64::from(self.queued));
        self.queued = 0;
        self.blocked = true;
        self.diagnostics.error = error;
    }

    pub(crate) fn poll(&mut self, now: u64, next_transaction: &mut u16) -> Option<Vec<u8>> {
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
        if self.blocked || self.queued == 0 {
            return None;
        }
        let transaction = *next_transaction;
        let Some(next) = transaction.checked_add(1) else {
            self.fail(-2);
            return None;
        };
        *next_transaction = next;
        self.queued -= 1;
        let packet =
            keepalive_operation_failed(transaction).expect("nonzero allocated transaction");
        self.pending = Some(Pending {
            transaction,
            packet: packet.clone(),
            deadline: now.saturating_add(DEADLINE_MS),
            attempts: 1,
        });
        self.diagnostics.sent = self.diagnostics.sent.saturating_add(1);
        Some(packet)
    }
}
