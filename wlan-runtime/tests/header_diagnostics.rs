// SPDX-License-Identifier: Apache-2.0
use diamaneos_wlan_runtime::header_diagnostics::HeaderDiagnostics;

#[test]
fn only_well_framed_response_and_indication_headers_are_counted() {
    let mut stats = HeaderDiagnostics::default();
    assert_eq!(stats.snapshot(), (0, 0, -1));
    stats.observe(&[2, 1, 0, 0x27, 0, 0, 0]);
    stats.observe(&[4, 0, 0, 0x40, 0, 1, 0, 0xff]);
    assert_eq!(stats.snapshot(), (1, 1, 0x40));
    // Unsupported body bytes are not interpreted or retained.
    stats.observe(&[4, 0, 0, 0x41, 0, 0, 0]);
    assert_eq!(stats.snapshot(), (1, 2, 0x41));
    let counts = stats.histogram();
    assert_eq!(counts.len(), 64);
    assert_eq!(counts[0x40 - 0x20], 1);
    assert_eq!(counts[0x41 - 0x20], 1);
    assert_eq!(counts.iter().sum::<i32>(), 2);
    stats.observe(&[0, 1, 0, 0x43, 0, 0, 0]);
    stats.observe(&[6, 1, 0, 0x43, 0, 0, 0]);
    stats.observe(&[4, 0, 0, 0x42, 0, 1, 0]);
    stats.observe(&[4, 0, 0, 0x42, 0, 0, 0, 0]);
    assert_eq!(stats.snapshot(), (1, 2, 0x41));
}

#[test]
fn unrelated_indications_cannot_hide_a_measurement_header() {
    let mut stats = HeaderDiagnostics::default();
    stats.observe(&[4, 0, 0, 0x40, 0, 0, 0]);
    stats.observe(&[4, 0, 0, 0x24, 0, 0, 0]);
    stats.observe(&[4, 0, 0, 0xff, 0xff, 0, 0]);
    assert_eq!(stats.snapshot(), (0, 3, 0xffff));
    assert_eq!(stats.histogram()[0x40 - 0x20], 1);
    assert_eq!(stats.histogram().iter().sum::<i32>(), 2);
}

#[test]
fn truncated_input_cannot_read_or_update_header_fields() {
    let packet = [4, 0, 0, 0x41, 0, 0, 0];
    for length in 0..7 {
        let mut stats = HeaderDiagnostics::default();
        stats.observe(&packet[..length]);
        assert_eq!(stats.snapshot(), (0, 0, -1));
    }
}
