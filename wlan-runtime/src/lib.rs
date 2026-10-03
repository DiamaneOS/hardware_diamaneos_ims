// SPDX-License-Identifier: Apache-2.0
// Shared observation lifetime logic; Android Binder adapter is in main.rs.
#![forbid(unsafe_code)]
use diamaneos_wlan_reporting::session::Observation;

/// Three ordinary 10-second observer heartbeats. Never keep a positive snapshot
/// indefinitely after observer death, delivery failure, or a stalled process.
const OBSERVATION_LEASE_MS: u64 = 30_000;

#[derive(Default)]
pub struct Observations {
    generation: u64,
    sequence: u64,
    value: Option<Observation>,
    updated_ms: u64,
    active: bool,
}
impl Observations {
    pub fn is_current(&self, generation: u64) -> bool {
        self.active && generation != 0 && generation == self.generation
    }
    pub fn register(&mut self) -> Option<u64> {
        self.generation = self.generation.checked_add(1)?;
        self.sequence = 0;
        self.invalidate();
        self.active = true;
        Some(self.generation)
    }
    pub fn update(
        &mut self,
        generation: u64,
        sequence: u64,
        now_ms: u64,
        value: Observation,
    ) -> bool {
        if !self.active
            || generation == 0
            || generation != self.generation
            || sequence <= self.sequence
            || now_ms < self.updated_ms
        {
            return false;
        }
        self.sequence = sequence;
        self.updated_ms = now_ms;
        self.value = Some(value);
        true
    }
    pub fn lost(&mut self, generation: u64) {
        if generation == self.generation {
            self.active = false;
            self.invalidate();
        }
    }
    fn invalidate(&mut self) {
        if let Some(value) = &mut self.value {
            value.network = None;
        }
    }
    pub fn current(&mut self, now_ms: u64) -> Option<Observation> {
        if self.sequence != 0 && now_ms.saturating_sub(self.updated_ms) >= OBSERVATION_LEASE_MS {
            self.active = false;
            self.invalidate();
        }
        self.value.clone()
    }
}
