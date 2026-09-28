// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! One DSD client's acknowledged lifecycle. The runtime owns peer validation.
use crate::{
    bind_subscription, response_for, wifi_switch, withdrawal, Connected, BIND_SUBSCRIPTION,
    DATA_SETTINGS, WLAN_STATUS,
};

const DEADLINE_MS: u64 = 2_000;
const MAX_ATTEMPTS: u8 = 3;

#[derive(Clone, PartialEq, Eq)]
pub struct Observation {
    pub enabled: bool,
    pub network: Option<Connected>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Bind,
    Clear,
    Switch,
    Status,
    Idle,
    Failed,
}

struct Pending {
    transaction: u16,
    message: u16,
    packet: Vec<u8>,
    deadline: u64,
    attempts: u8,
}

pub struct Session {
    subscription: u32,
    next_transaction: u16,
    step: Step,
    pending: Option<Pending>,
    desired: Observation,
    sending: Option<Observation>,
    confirmed: Option<Observation>,
    previous_bssid: [u8; 6],
    failed_operation: u16,
    error: i32,
}

/// Fixed numeric diagnostics, with no observation or packet data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Diagnostics {
    /// Bind, clear, switch, status, settled-down, settled-up, failed.
    pub stage: i32,
    pub operation: u16,
    /// 0: no failure; positive: QMI error; -1: timeout; -2: transaction
    /// exhaustion; -3: local encoding failure.
    pub error: i32,
}

impl Session {
    pub fn new(subscription: u32) -> Option<Self> {
        (1..=2).contains(&subscription).then_some(Self {
            subscription,
            next_transaction: 1,
            step: Step::Bind,
            pending: None,
            desired: Observation {
                enabled: false,
                network: None,
            },
            sending: None,
            confirmed: None,
            previous_bssid: [0; 6],
            failed_operation: 0,
            error: 0,
        })
    }

    pub fn observe(&mut self, mut observation: Observation) {
        if !observation.enabled {
            observation.network = None;
        }
        // Only one desired snapshot is retained. No unbounded event history.
        self.desired = observation;
        if self.step == Step::Idle && self.confirmed.as_ref() != Some(&self.desired) {
            self.step = Step::Switch;
        }
    }

    pub fn failed(&self) -> bool {
        self.step == Step::Failed
    }

    /// Message identity only, never identifiers or encoded payloads.
    pub fn pending_message(&self) -> Option<u16> {
        self.pending.as_ref().map(|p| p.message)
    }

    pub fn settled(&self) -> bool {
        self.step == Step::Idle && self.confirmed.as_ref() == Some(&self.desired)
    }

    pub fn diagnostics(&self) -> Diagnostics {
        let stage = match self.step {
            Step::Bind => 0,
            Step::Clear => 1,
            Step::Switch => 2,
            Step::Status => 3,
            Step::Idle if self.confirmed.as_ref().is_some_and(|s| s.network.is_some()) => 5,
            Step::Idle => 4,
            Step::Failed => 6,
        };
        Diagnostics {
            stage,
            operation: self.pending_message().unwrap_or(self.failed_operation),
            error: self.error,
        }
    }

    /// Retries preserve transaction and payload. Exhaustion requires a fresh
    /// QRTR endpoint; this instance never wraps and reuses a transaction ID.
    pub fn poll(&mut self, now_ms: u64) -> Option<Vec<u8>> {
        if let Some(pending) = &mut self.pending {
            if now_ms < pending.deadline {
                return None;
            }
            // Never retransmit a connected payload after the observation changed.
            // Keep the old transaction quarantined by moving to a fresh ID.
            if matches!(self.step, Step::Switch | Step::Status)
                && self.sending.as_ref() != Some(&self.desired)
            {
                self.pending = None;
                self.step = Step::Switch;
                return self.poll(now_ms);
            }
            if pending.attempts >= MAX_ATTEMPTS {
                self.failed_operation = pending.message;
                self.error = -1;
                self.pending = None;
                self.step = Step::Failed;
                return None;
            }
            pending.attempts += 1;
            pending.deadline = now_ms.saturating_add(DEADLINE_MS);
            return Some(pending.packet.clone());
        }
        if matches!(self.step, Step::Idle | Step::Failed) {
            return None;
        }
        let tx = self.next_transaction;
        let Some(next) = tx.checked_add(1) else {
            self.error = -2;
            self.step = Step::Failed;
            return None;
        };
        self.next_transaction = next;
        let (message, packet) = match self.step {
            Step::Bind => (BIND_SUBSCRIPTION, bind_subscription(tx, self.subscription)),
            Step::Clear => (WLAN_STATUS, withdrawal(tx, [0; 6])),
            Step::Switch => {
                self.sending = Some(self.desired.clone());
                (DATA_SETTINGS, wifi_switch(tx, self.desired.enabled))
            }
            Step::Status => {
                // A disconnect arriving during switch acknowledgement takes
                // precedence over the old connected snapshot.
                if self.sending.as_ref() != Some(&self.desired) {
                    self.step = Step::Switch;
                    return self.poll(now_ms);
                }
                let packet = match self.sending.as_ref().and_then(|s| s.network.as_ref()) {
                    Some(network) => network.encode(tx),
                    None => withdrawal(tx, self.previous_bssid),
                };
                (WLAN_STATUS, packet)
            }
            _ => return None,
        };
        let Ok(packet) = packet else {
            self.failed_operation = message;
            self.error = -3;
            self.step = Step::Failed;
            return None;
        };
        self.pending = Some(Pending {
            transaction: tx,
            message,
            packet: packet.clone(),
            deadline: now_ms.saturating_add(DEADLINE_MS),
            attempts: 1,
        });
        Some(packet)
    }

    /// Malformed/unrelated packets cannot consume the pending request. A valid
    /// negative result does consume it and fails this session explicitly.
    pub fn receive(&mut self, bytes: &[u8]) {
        let Some(p) = &self.pending else {
            return;
        };
        match response_for(bytes, p.transaction, p.message) {
            Ok(()) => {}
            Err(crate::Error::ModemFailure(error)) => {
                self.failed_operation = p.message;
                self.error = i32::from(error);
                self.pending = None;
                self.step = Step::Failed;
                return;
            }
            Err(_) => return,
        }
        self.pending = None;
        self.step = match self.step {
            Step::Bind => Step::Clear,
            Step::Clear => Step::Switch,
            Step::Switch => Step::Status,
            Step::Status => {
                if let Some(s) = &self.sending {
                    self.previous_bssid = s.network.as_ref().map_or([0; 6], |n| n.bssid);
                }
                self.confirmed = self.sending.take();
                if self.confirmed.as_ref() == Some(&self.desired) {
                    Step::Idle
                } else {
                    Step::Switch
                }
            }
            _ => Step::Failed,
        };
    }
}
