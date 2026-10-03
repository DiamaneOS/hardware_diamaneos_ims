// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! Header-only observations. Never retain payloads, transactions or endpoints.

#[derive(Clone, Copy)]
pub struct HeaderDiagnostics {
    responses: u32,
    indications: u32,
    last_indication: u16,
    indications_by_id: [u32; 64],
}

impl Default for HeaderDiagnostics {
    fn default() -> Self {
        Self {
            responses: 0,
            indications: 0,
            last_indication: 0,
            indications_by_id: [0; 64],
        }
    }
}

impl HeaderDiagnostics {
    /// Call only after the runtime has matched the current modem endpoint.
    /// This validates envelope framing, not any unsupported message body.
    pub fn observe(&mut self, packet: &[u8]) {
        if packet.len() < 7
            || usize::from(u16::from_le_bytes([packet[5], packet[6]])) != packet.len() - 7
        {
            return;
        }
        match packet[0] {
            2 => self.responses = self.responses.saturating_add(1),
            4 => {
                self.indications = self.indications.saturating_add(1);
                self.last_indication = u16::from_le_bytes([packet[3], packet[4]]);
                if let Some(index) = self.last_indication.checked_sub(0x20).filter(|v| *v < 64) {
                    let count = &mut self.indications_by_id[usize::from(index)];
                    *count = count.saturating_add(1);
                }
            }
            _ => {}
        }
    }

    pub fn snapshot(&self) -> (i64, i64, i32) {
        (
            i64::from(self.responses),
            i64::from(self.indications),
            if self.indications == 0 {
                -1
            } else {
                i32::from(self.last_indication)
            },
        )
    }

    /// Bounded histogram for IDs0x20..0x5f, so a later indication cannot hide a rare one.
    pub fn histogram(&self) -> Vec<i32> {
        self.indications_by_id
            .iter()
            .map(|v| (*v).min(i32::MAX as u32) as i32)
            .collect()
    }
}
