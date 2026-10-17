// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! Host tests for the `--write` run with a fake modem. Synthetic IMEIs only
//! (fake all-zero TACs with correct Luhn digits).

use diamaneos_imeiprov::imei::Imei;
use diamaneos_imeiprov::provision::{write_run, Modem, Order, Status};

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
    /// Reads that work before every later read fails.
    reads_before_failure: Option<usize>,
    /// Writes that land before the next one stops the run (a crash).
    writes_before_crash: Option<usize>,
    log: Vec<u8>,
}

impl FakeModem {
    fn normal() -> [Option<Imei>; 2] {
        [Some(slot1()), Some(slot2())]
    }

    fn reversed() -> [Option<Imei>; 2] {
        [Some(slot2()), Some(slot1())]
    }

    /// One boot: the run and the status it reports.
    fn boot(&mut self) -> Status {
        self.log.clear();
        let run = write_run(self, &slot1(), &slot2()).unwrap();
        Status::of(&run, &slot1())
    }
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
        if let Some(n) = &mut self.writes_before_crash {
            if *n == 0 {
                return false;
            }
            *n -= 1;
        }
        self.log.push(subscription);
        if self.failing_sub == Some(subscription) {
            return false;
        }
        self.nv[usize::from(subscription)] = Some(*imei);
        true
    }
}

#[test]
fn first_write_reverses_and_the_next_boot_corrects() {
    let mut modem = FakeModem::default();
    assert_eq!(modem.boot(), Status::Reversed);
    assert_eq!(modem.nv, FakeModem::reversed());
    assert_eq!(modem.log, [0, 1]);

    assert_eq!(modem.boot(), Status::Corrected);
    assert_eq!(modem.nv, FakeModem::normal());
    assert_eq!(modem.log, [1, 0], "subscription 0 keeps the marker until last");

    assert_eq!(modem.boot(), Status::Unchanged);
    assert_eq!(modem.nv, FakeModem::normal());
}

#[test]
fn a_failed_reversed_write_falls_back_to_the_normal_pair() {
    let mut modem = FakeModem {
        failing_sub: Some(1),
        ..Default::default()
    };
    let run = write_run(&mut modem, &slot1(), &slot2()).unwrap();
    let Order::Fallback(reversed) = run.order else {
        panic!("{:?}", run.order);
    };
    assert!(reversed.sub0_ok && !reversed.sub1_ok);
    assert_eq!(Status::of(&run, &slot1()), Status::Failed);
    assert_eq!(modem.nv, [Some(slot1()), None]);

    // No read-back works: the normal pair is still written and the run fails.
    let mut modem = FakeModem {
        reads_before_failure: Some(1),
        ..Default::default()
    };
    let run = write_run(&mut modem, &slot1(), &slot2()).unwrap();
    assert!(matches!(run.order, Order::Fallback(w) if !w.verified));
    assert!(!run.written.verified);
    assert_eq!(modem.nv, FakeModem::normal());
}

#[test]
fn fallback_after_a_bad_read_back_reports_written() {
    struct Mismatch(FakeModem, usize);
    impl Modem for Mismatch {
        type ReadError = ();
        fn read(&mut self) -> Result<Option<Imei>, ()> {
            self.1 += 1;
            // The read-back after the reversed write shows the wrong value.
            if self.1 == 2 {
                return Ok(Some(slot1()));
            }
            self.0.read()
        }
        fn write(&mut self, subscription: u8, imei: &Imei) -> bool {
            self.0.write(subscription, imei)
        }
    }
    let mut modem = Mismatch(FakeModem::default(), 0);
    let run = write_run(&mut modem, &slot1(), &slot2()).unwrap();
    assert!(matches!(run.order, Order::Fallback(_)));
    assert_eq!(Status::of(&run, &slot1()), Status::Written);
    assert_eq!(modem.0.nv, FakeModem::normal());
}

#[test]
fn a_crash_during_the_reversed_write_is_corrected_next_boot() {
    // Stopped after subscription 0: the marker is there, subscription 1 not.
    let mut modem = FakeModem {
        writes_before_crash: Some(1),
        ..Default::default()
    };
    let _ = write_run(&mut modem, &slot1(), &slot2());
    assert_eq!(modem.nv, [Some(slot2()), None]);
    modem.writes_before_crash = None;
    assert_eq!(modem.boot(), Status::Corrected);
    assert_eq!(modem.nv, FakeModem::normal());
}

#[test]
fn a_crash_during_the_correction_repeats_it() {
    let mut modem = FakeModem {
        nv: FakeModem::reversed(),
        writes_before_crash: Some(1),
        ..Default::default()
    };
    let _ = write_run(&mut modem, &slot1(), &slot2());
    assert_eq!(modem.nv, [Some(slot2()), Some(slot2())]);
    modem.writes_before_crash = None;
    assert_eq!(modem.boot(), Status::Corrected);
    assert_eq!(modem.nv, FakeModem::normal());
}

#[test]
fn a_failed_correction_is_retried_and_reported_failed() {
    let mut modem = FakeModem {
        nv: FakeModem::reversed(),
        failing_sub: Some(0),
        ..Default::default()
    };
    assert_eq!(modem.boot(), Status::Failed);
    assert_eq!(modem.nv[0], Some(slot2()));
    modem.failing_sub = None;
    assert_eq!(modem.boot(), Status::Corrected);
    assert_eq!(modem.nv, FakeModem::normal());
}

#[test]
fn failed_slot2_write_is_retried_on_the_next_boot() {
    let mut modem = FakeModem {
        nv: [Some(slot1()), None],
        failing_sub: Some(1),
        ..Default::default()
    };
    assert_eq!(modem.boot(), Status::Failed);
    assert_eq!(modem.nv[1], None);

    // Next boot: slot 1 already matches, slot 2 is still written.
    modem.failing_sub = None;
    assert_eq!(modem.boot(), Status::Unchanged);
    assert_eq!(modem.nv, FakeModem::normal());
}

#[test]
fn an_unexpected_value_gets_the_normal_pair_and_reports_written() {
    let other = Imei::from_ascii(b"000000000000034").unwrap();
    let mut modem = FakeModem {
        nv: [Some(other), Some(other)],
        ..Default::default()
    };
    assert_eq!(modem.boot(), Status::Written);
    assert_eq!(modem.nv, FakeModem::normal());
    assert_eq!(modem.log, [0, 1]);
}

#[test]
fn read_failures_fail_the_run() {
    let mut unreadable = FakeModem {
        reads_before_failure: Some(0),
        ..Default::default()
    };
    assert!(write_run(&mut unreadable, &slot1(), &slot2()).is_err());
    assert!(unreadable.log.is_empty());

    let mut no_read_back = FakeModem {
        nv: FakeModem::normal(),
        reads_before_failure: Some(1),
        ..Default::default()
    };
    let run = write_run(&mut no_read_back, &slot1(), &slot2()).unwrap();
    assert!(run.written.sub0_ok && run.written.sub1_ok && !run.written.verified);
    assert_eq!(Status::of(&run, &slot1()), Status::Failed);
}

#[test]
fn every_status_is_in_the_property_enum() {
    let contexts = include_str!("../../sepolicy/vendor/property_contexts");
    let line = contexts
        .lines()
        .find(|l| l.starts_with("vendor.diamaneos.imei.status "))
        .unwrap();
    let values: Vec<&str> = line.split_whitespace().skip(4).collect();
    for status in [
        Status::Running,
        Status::Written,
        Status::Unchanged,
        Status::Failed,
        Status::Reversed,
        Status::Corrected,
    ] {
        assert!(values.contains(&status.as_str()));
    }
}
