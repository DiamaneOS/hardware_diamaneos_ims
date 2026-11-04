// SPDX-License-Identifier: Apache-2.0
// Shared observation lifetime logic. Android Binder adapter is in main.rs.
#![forbid(unsafe_code)]
use diamaneos_wlan_reporting::{session::Observation, Connected};
use std::net::{Ipv4Addr, Ipv6Addr};

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

/// What one modem subscription may see. Wi-Fi details go only to a SIM with
/// Wi-Fi calling on (bit `subscription - 1` of `wifi_calling_slots`).
pub fn for_subscription(
    observation: &Observation,
    wifi_calling_slots: u32,
    subscription: u32,
) -> Observation {
    let on = subscription
        .checked_sub(1)
        .and_then(|slot| 1u32.checked_shl(slot))
        .is_some_and(|bit| wifi_calling_slots & bit != 0);
    Observation {
        enabled: observation.enabled,
        network: observation.network.clone().filter(|_| on),
    }
}

/// The observer's Binder snapshot (de.diamaneos.wlan.Snapshot), field for
/// field, so the checks below run on the host as well.
#[derive(Clone, Default)]
pub struct SnapshotFields {
    pub enabled: bool,
    pub connected: bool,
    pub validated: bool,
    pub default_route: bool,
    pub bssid: [u8; 6],
    pub has_ipv4: bool,
    pub ipv4: [u8; 4],
    pub has_ipv6: bool,
    pub ipv6: [u8; 16],
    pub ipv6_prefix: i32,
    pub dns4_count: i32,
    pub dns4: [u8; 8],
    pub dns6_count: i32,
    pub dns6: [u8; 32],
    pub wifi_calling_slots: i32,
}

/// Check one snapshot. Returns the observation and the Wi-Fi calling slot
/// bits, or None to reject the call.
pub fn observation(snapshot: &SnapshotFields) -> Option<(Observation, u32)> {
    if !(0..=3).contains(&snapshot.wifi_calling_slots) {
        return None;
    }
    let network = if snapshot.connected {
        if !snapshot.enabled || !(0..=128).contains(&snapshot.ipv6_prefix) {
            return None;
        }
        let v4 = snapshot.has_ipv4.then(|| Ipv4Addr::from(snapshot.ipv4));
        let v6 = snapshot
            .has_ipv6
            .then(|| (Ipv6Addr::from(snapshot.ipv6), snapshot.ipv6_prefix as u8));
        if !(0..=2).contains(&snapshot.dns4_count) || !(0..=2).contains(&snapshot.dns6_count) {
            return None;
        }
        let mut dns4 = [None; 2];
        let mut dns6 = [None; 2];
        for (index, bytes) in snapshot.dns4.chunks_exact(4).enumerate() {
            if index < snapshot.dns4_count as usize {
                dns4[index] = Some(Ipv4Addr::new(bytes[0], bytes[1], bytes[2], bytes[3]));
            } else if bytes.iter().any(|b| *b != 0) {
                return None;
            }
        }
        for (index, bytes) in snapshot.dns6.chunks_exact(16).enumerate() {
            if index < snapshot.dns6_count as usize {
                let mut address = [0; 16];
                address.copy_from_slice(bytes);
                dns6[index] = Some(Ipv6Addr::from(address));
            } else if bytes.iter().any(|b| *b != 0) {
                return None;
            }
        }
        Some(
            Connected::new(snapshot.bssid, v4, v6, snapshot.validated)
                .map(|network| network.with_default_route(snapshot.default_route))
                .and_then(|network| network.with_dns(dns4, dns6))
                .ok()?,
        )
    } else {
        None
    };
    Some((
        Observation {
            enabled: snapshot.enabled,
            network,
        },
        snapshot.wifi_calling_slots as u32,
    ))
}
