// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! Host tests for the `--write` run with a fake modem. Synthetic IMEIs only
//! (fake all-zero TACs with correct Luhn digits).

use diamaneos_imeiprov::imei::Imei;
use diamaneos_imeiprov::provision::{write_run, Modem, Status};

fn slot1() -> Imei {
    Imei::from_ascii(b"000000000000018").unwrap()
}

fn slot2() -> Imei {
    Imei::from_ascii(b"000000000000026").unwrap()
}

/// NV 550 per subscription. A read shows subscription 0, as on the phone.
#[derive(Default)]
struct FakeModem {
    nv: [Option<Imei>; 2],
    failing_sub: Option<u8>,
    reads_before_failure: Option<usize>,
    writes: usize,
}

impl Modem for FakeModem {
    type ReadError = ();

    fn read(&mut self) -> Result<Option<Imei>, ()> {
        match &mut self.reads_before_failure {
            Some(0) => Err(()),
            Some(n) => {
                *n -= 1;
                Ok(self.nv[0])
            }
            None => Ok(self.nv[0]),
        }
    }

    fn write(&mut self, subscription: u8, imei: &Imei) -> bool {
        self.writes += 1;
        if self.failing_sub == Some(subscription) {
            return false;
        }
        self.nv[usize::from(subscription)] = Some(*imei);
        true
    }
}

#[test]
fn failed_slot2_write_is_retried_on_the_next_boot() {
    let mut modem = FakeModem {
        failing_sub: Some(1),
        ..Default::default()
    };
    let (before, written) = write_run(&mut modem, &slot1(), &slot2()).unwrap();
    assert_eq!(before, None);
    assert!(written.sub0_ok && !written.sub1_ok && written.verify_slot1);
    assert!(!written.ok());
    assert_eq!(modem.nv[1], None);

    // Next boot: slot 1 already matches, slot 2 is still written.
    modem.failing_sub = None;
    let (before, written) = write_run(&mut modem, &slot1(), &slot2()).unwrap();
    assert_eq!(before, Some(slot1()));
    assert!(written.ok());
    assert_eq!(modem.nv, [Some(slot1()), Some(slot2())]);
}

#[test]
fn read_failures_fail_the_run() {
    let mut unreadable = FakeModem {
        reads_before_failure: Some(0),
        ..Default::default()
    };
    assert!(write_run(&mut unreadable, &slot1(), &slot2()).is_err());
    assert_eq!(unreadable.writes, 0);

    let mut no_read_back = FakeModem {
        reads_before_failure: Some(1),
        ..Default::default()
    };
    let (_, written) = write_run(&mut no_read_back, &slot1(), &slot2()).unwrap();
    assert!(written.sub0_ok && written.sub1_ok && !written.verify_slot1);
    assert!(!written.ok());
}

#[test]
fn status_is_written_only_when_the_modem_started_without_the_imeis() {
    let mut modem = FakeModem::default();
    let status = |modem: &mut FakeModem| {
        let (before, written) = write_run(modem, &slot1(), &slot2()).unwrap();
        Status::of(before, &slot1(), &written)
    };
    assert_eq!(status(&mut modem), Status::Written);
    assert_eq!(status(&mut modem), Status::Unchanged);
    modem.failing_sub = Some(1);
    assert_eq!(status(&mut modem), Status::Failed);
}

#[test]
fn every_status_is_in_the_property_enum() {
    let contexts = include_str!("../../sepolicy/vendor/property_contexts");
    let line = contexts
        .lines()
        .find(|l| l.starts_with("vendor.diamaneos.imei.status "))
        .unwrap();
    let values: Vec<&str> = line.split_whitespace().skip(4).collect();
    for status in [Status::Running, Status::Written, Status::Unchanged, Status::Failed] {
        assert!(values.contains(&status.as_str()));
    }
}
