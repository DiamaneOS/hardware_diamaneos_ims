// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! One DSD client's acknowledged lifecycle. The runtime owns peer validation.
use crate::{
    bind_subscription, default_profile_status, diagnostic_notification_registration, response_for,
    wifi_switch, withdrawal, Connected, BIND_SUBSCRIPTION, DATA_SETTINGS, DEFAULT_PROFILE_STATUS,
    INDICATION_REGISTRATION, WLAN_STATUS,
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
    DiagnosticRegistration,
    Clear,
    ClearProfile,
    Switch,
    Status,
    Profile,
    WithdrawProfile,
    WithdrawStatus,
    WithdrawSwitch,
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
    bound: bool,
    keepalive_failures: crate::keepalive::Failures,
    diagnostic_registration: bool,
    next_transaction: u16,
    step: Step,
    pending: Option<Pending>,
    desired: Observation,
    sending: Option<Observation>,
    confirmed: Option<Observation>,
    // Administrative state is independent of connection/metadata settlement.
    // None also covers a command whose effect has not been acknowledged.
    acknowledged_switch: Option<bool>,
    previous_bssid: [u8; 6],
    failed_operation: u16,
    error: i32,
}

/// Fixed numeric diagnostics, with no observation or packet data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Diagnostics {
    /// Bind, clear, switch, status, settled-down, settled-up, failed, profile;
    /// private diagnostic registration uses8.
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
            bound: false,
            keepalive_failures: Default::default(),
            diagnostic_registration: false,
            next_transaction: 1,
            step: Step::Bind,
            pending: None,
            desired: Observation {
                enabled: false,
                network: None,
            },
            sending: None,
            confirmed: None,
            acknowledged_switch: None,
            previous_bssid: [0; 6],
            failed_operation: 0,
            error: 0,
        })
    }

    /// Opt-in PRIVATE DIAGNOSTIC, not supported production profile handling.
    /// Runtime selection requires both a private compile flag and ro.debuggable.
    pub fn new_with_diagnostic_registration(subscription: u32) -> Option<Self> {
        Self::new(subscription).map(|mut session| {
            session.diagnostic_registration = true;
            session
        })
    }

    pub fn observe(&mut self, mut observation: Observation) {
        if !observation.enabled {
            observation.network = None;
        }
        // Only one desired snapshot is retained. No unbounded event history.
        self.desired = observation;
        if self.step == Step::Idle && self.confirmed.as_ref() != Some(&self.desired) {
            self.step = self.update_start();
        }
    }

    fn update_start(&self) -> Step {
        if self.desired.network.is_none() {
            Step::WithdrawProfile
        } else {
            Step::Switch
        }
    }

    fn settle_update(&mut self) -> Step {
        if let Some(s) = &self.sending {
            self.previous_bssid = s.network.as_ref().map_or([0; 6], |n| n.bssid);
        }
        self.confirmed = self.sending.take();
        if self.confirmed.as_ref() == Some(&self.desired) {
            Step::Idle
        } else {
            self.update_start()
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
            Step::DiagnosticRegistration => 8,
            Step::Clear => 1,
            Step::ClearProfile | Step::Profile | Step::WithdrawProfile => 7,
            Step::Switch | Step::WithdrawSwitch => 2,
            Step::Status | Step::WithdrawStatus => 3,
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

    pub fn keepalive_diagnostics(&self) -> crate::keepalive::Diagnostics {
        self.keepalive_failures.diagnostics()
    }

    /// Retries preserve transaction and payload. Exhaustion requires a fresh
    /// QRTR endpoint; this instance never wraps and reuses a transaction ID.
    pub fn poll(&mut self, now_ms: u64) -> Option<Vec<u8>> {
        self.poll_control(now_ms)
    }

    /// Bounded transport cycle. Send both present packets in order: connectivity
    /// first, then optional completion. Both deadline machines advance even when
    /// control emits continuously; neither result is discarded or speculative.
    pub fn poll_cycle(&mut self, now_ms: u64) -> [Option<Vec<u8>>; 2] {
        let control = self.poll_control(now_ms);
        let reply = if self.bound {
            self.keepalive_failures
                .poll(now_ms, &mut self.next_transaction)
        } else {
            None
        };
        // A final allocated connectivity packet must still be sent and tracked.
        // Renew only after its ACK/retries drain; optional exhaustion cannot
        // discard an in-flight withdrawal or consume its transaction.
        if self.keepalive_failures.diagnostics().error == -2
            && self.pending.is_none()
            && !self.failed()
        {
            self.failed_operation = crate::NAT_KEEPALIVE_OPERATION_STATUS;
            self.error = -2;
            self.pending = None;
            self.step = Step::Failed;
            return [None, None];
        }
        [control, reply]
    }

    fn poll_control(&mut self, now_ms: u64) -> Option<Vec<u8>> {
        if let Some(pending) = &mut self.pending {
            if now_ms < pending.deadline {
                return None;
            }
            // Never retransmit a connected payload after the observation changed.
            // Keep the old transaction quarantined by moving to a fresh ID.
            if matches!(
                self.step,
                Step::Switch
                    | Step::Status
                    | Step::Profile
                    | Step::WithdrawProfile
                    | Step::WithdrawStatus
                    | Step::WithdrawSwitch
            ) && self.sending.as_ref() != Some(&self.desired)
            {
                self.pending = None;
                self.step = self.update_start();
                return self.poll_control(now_ms);
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
        if self.step == Step::WithdrawProfile && self.desired.network.is_some() {
            self.step = Step::Switch;
        }
        if matches!(self.step, Step::WithdrawStatus | Step::WithdrawSwitch)
            && self.sending.as_ref() != Some(&self.desired)
        {
            self.step = self.update_start();
            return self.poll_control(now_ms);
        }
        if self.acknowledged_switch == Some(self.desired.enabled) {
            match self.step {
                Step::Switch => {
                    self.sending = Some(self.desired.clone());
                    self.step = Step::Profile;
                    return self.poll_control(now_ms);
                }
                Step::WithdrawSwitch => {
                    self.step = self.settle_update();
                    return self.poll_control(now_ms);
                }
                _ => {}
            }
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
            Step::DiagnosticRegistration => (
                INDICATION_REGISTRATION,
                diagnostic_notification_registration(tx),
            ),
            Step::Clear => (WLAN_STATUS, withdrawal(tx, [0; 6])),
            Step::ClearProfile => (DEFAULT_PROFILE_STATUS, default_profile_status(tx, false)),
            Step::WithdrawProfile => {
                self.sending = Some(self.desired.clone());
                // Revoke the default profile before withdrawing STA or the
                // switch, matching stock's runtime profile-before-STA order.
                (DEFAULT_PROFILE_STATUS, default_profile_status(tx, false))
            }
            Step::WithdrawStatus => (WLAN_STATUS, withdrawal(tx, self.previous_bssid)),
            Step::WithdrawSwitch => {
                self.acknowledged_switch = None;
                (DATA_SETTINGS, wifi_switch(tx, self.desired.enabled))
            }
            Step::Switch => {
                self.sending = Some(self.desired.clone());
                // A superseded or lost acknowledgement cannot leave the old
                // cached value trusted after this command may have applied.
                self.acknowledged_switch = None;
                (DATA_SETTINGS, wifi_switch(tx, self.desired.enabled))
            }
            Step::Status => {
                // Loss or replacement while acknowledging the profile takes
                // precedence over the old connected snapshot.
                if self.sending.as_ref() != Some(&self.desired) {
                    self.step = self.update_start();
                    return self.poll_control(now_ms);
                }
                let packet = match self.sending.as_ref().and_then(|s| s.network.as_ref()) {
                    Some(network) => network.encode(tx),
                    None => withdrawal(tx, self.previous_bssid),
                };
                (WLAN_STATUS, packet)
            }
            Step::Profile => {
                // A lost/replaced observation must never gain a stale positive
                // default-profile update before station reporting.
                if self.sending.as_ref() != Some(&self.desired) {
                    self.step = self.update_start();
                    return self.poll_control(now_ms);
                }
                let connected_default = self
                    .sending
                    .as_ref()
                    .and_then(|observation| observation.network.as_ref())
                    .is_some_and(|network| network.validated && network.default_route);
                (
                    DEFAULT_PROFILE_STATUS,
                    default_profile_status(tx, connected_default),
                )
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
        if self.bound && self.keepalive_failures.receive(bytes) {
            return;
        }
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
        if self.step == Step::Bind {
            self.bound = true;
        }
        if matches!(self.step, Step::Switch | Step::WithdrawSwitch) {
            self.acknowledged_switch = self.sending.as_ref().map(|s| s.enabled);
        }
        self.step = match self.step {
            Step::Bind if self.diagnostic_registration => Step::DiagnosticRegistration,
            Step::Bind | Step::DiagnosticRegistration => Step::Clear,
            Step::Clear => Step::ClearProfile,
            Step::ClearProfile => self.update_start(),
            // Stock's runtime callback reports the current default profile
            // before STA on both positive and negative paths. Availability is
            // still confirmed only after the final station acknowledgement.
            Step::Switch => Step::Profile,
            Step::Profile => Step::Status,
            Step::Status | Step::WithdrawSwitch => self.settle_update(),
            Step::WithdrawProfile => Step::WithdrawStatus,
            Step::WithdrawStatus => Step::WithdrawSwitch,
            _ => Step::Failed,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optional_exhaustion_preserves_the_last_control_request_and_retries() {
        let mut session = Session::new(1).unwrap();
        session.bound = true;
        session.step = Step::WithdrawProfile;
        session.next_transaction = u16::MAX - 1;
        session.receive(&[4, 0, 0, 0x41, 0, 4, 0, 1, 1, 0, 1]);
        let [control, optional] = session.poll_cycle(0);
        let control = control.unwrap();
        assert_eq!(control[3], DEFAULT_PROFILE_STATUS as u8);
        assert_eq!(&control[1..3], &(u16::MAX - 1).to_le_bytes());
        assert!(optional.is_none());
        assert!(!session.failed());
        assert_eq!(session.keepalive_diagnostics().error, -2);
        assert_eq!(session.pending.as_ref().unwrap().transaction, u16::MAX - 1);
        let [retry, optional] = session.poll_cycle(2000);
        assert_eq!(retry.unwrap(), control);
        assert!(optional.is_none());
        assert!(!session.failed());
        session.receive(&[
            2, control[1], control[2], control[3], 0, 7, 0, 2, 4, 0, 0, 0, 0, 0,
        ]);
        assert!(session.step == Step::WithdrawStatus);
        assert!(session.pending.is_none());
        assert!(session.poll_cycle(2001).into_iter().all(|p| p.is_none()));
        assert!(session.failed());
        assert_eq!(session.next_transaction, u16::MAX);
    }
}
