// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! Modem input to the `--write` run: what NV 550 reads return and whether each
//! write is acknowledged, driven by the fuzzer. Synthetic IMEIs only.
#![no_main]

use arbitrary::{Arbitrary, Unstructured};
use diamaneos_imeiprov::imei::Imei;
use diamaneos_imeiprov::provision::{write_run, Modem, Order, Status};
use libfuzzer_sys::fuzz_target;

#[derive(Arbitrary, Debug)]
enum Read {
    Fail,
    Empty,
    /// 15 digits, reduced to 0..=9. Made up by the fuzzer, never logged.
    Value([u8; 15]),
}

struct FuzzModem<'a, 'b> {
    u: &'a mut Unstructured<'b>,
    reads: usize,
    writes: usize,
}

impl Modem for FuzzModem<'_, '_> {
    type ReadError = ();
    fn read(&mut self) -> Result<Option<Imei>, ()> {
        self.reads += 1;
        match Read::arbitrary(self.u).unwrap_or(Read::Fail) {
            Read::Fail => Err(()),
            Read::Empty => Ok(None),
            Read::Value(mut d) => {
                d.iter_mut().for_each(|x| *x %= 10);
                Ok(Some(Imei::from_digits(d).unwrap()))
            }
        }
    }
    fn write(&mut self, subscription: u8, _imei: &Imei) -> bool {
        assert!(subscription <= 1);
        self.writes += 1;
        bool::arbitrary(self.u).unwrap_or(false)
    }
}

fuzz_target!(|data: &[u8]| {
    let mut u = Unstructured::new(data);
    // Synthetic pair (all-zero TAC, Luhn-valid), as in imeiprov/tests.
    let pick = u8::arbitrary(&mut u).unwrap_or(0);
    let a = Imei::from_ascii(b"000000000000018").unwrap();
    let b = Imei::from_ascii(b"000000000000026").unwrap();
    let (slot1, slot2) = match pick % 3 {
        0 => (a, b),
        1 => (b, a),
        _ => (a, a),
    };
    let mut modem = FuzzModem { u: &mut u, reads: 0, writes: 0 };
    let Ok(run) = write_run(&mut modem, &slot1, &slot2) else {
        assert_eq!(modem.writes, 0, "no write after a failed first read");
        return;
    };
    // At most two passes of two writes plus a read-back each.
    assert!(modem.writes == 2 || modem.writes == 4);
    assert!(modem.reads <= 3);
    let status = Status::of(&run, &slot1);
    if !run.written.ok() {
        assert_eq!(status, Status::Failed);
    }
    if let Order::Fallback(reversed) = run.order {
        assert!(!reversed.ok());
    }
    let _ = (status.as_str(), run.order.as_str());
});
