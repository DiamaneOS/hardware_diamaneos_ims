// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! Read-only libc calendar adapter for the modem's WLAN time query.
use std::{
    io,
    mem::MaybeUninit,
    time::{SystemTime, UNIX_EPOCH},
};

pub struct Sample {
    /// sec, min, hour, day, month, year, Sunday-based weekday, signed quarters west.
    pub fields: [u16; 8],
    pub utc_seconds: u64,
}
pub fn now() -> io::Result<Sample> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_secs();
    at(seconds)
}
fn at(seconds: u64) -> io::Result<Sample> {
    let timestamp: libc::time_t = seconds.try_into().map_err(io::Error::other)?;
    let mut local = MaybeUninit::<libc::tm>::uninit();
    let mut utc = MaybeUninit::<libc::tm>::uninit();
    // SAFETY: input is a live time_t and each output has the exact libc layout.
    // The reentrant functions initialize the output on success, checked below.
    unsafe {
        if libc::localtime_r(&timestamp, local.as_mut_ptr()).is_null()
            || libc::gmtime_r(&timestamp, utc.as_mut_ptr()).is_null()
        {
            return Err(io::Error::other("calendar conversion failed"));
        }
        let local = local.assume_init();
        let mut utc = utc.assume_init();
        // Match the verified modem ABI: local fields include DST, but the zone
        // is the standard (non-DST) offset west of UTC, in 15-minute units.
        // gmtime_r supplies tm_isdst=0; mktime interprets that as local standard time.
        let interpreted = libc::mktime(&mut utc);
        if interpreted == -1 {
            return Err(io::Error::other("timezone conversion failed"));
        }
        let offset = (interpreted as i128) - (timestamp as i128);
        let year = local.tm_year as i64 + 1900;
        if !(1970..=9999).contains(&year)
            || offset % 900 != 0
            || !(-56..=56).contains(&(offset / 900))
        {
            return Err(io::Error::other("unrepresentable calendar"));
        }
        Ok(Sample {
            fields: [
                local.tm_sec as u16,
                local.tm_min as u16,
                local.tm_hour as u16,
                local.tm_mday as u16,
                (local.tm_mon + 1) as u16,
                year as u16,
                local.tm_wday as u16,
                ((offset / 900) as i16) as u16,
            ],
            utc_seconds: seconds,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn calendar_is_read_only_and_bounded() {
        let sample = at(1_800_000_000).unwrap();
        assert_eq!(sample.utc_seconds, 1_800_000_000);
        assert!(sample.fields[0] <= 60 && sample.fields[1] <= 59);
        assert!(sample.fields[2] <= 23 && (1..=31).contains(&sample.fields[3]));
        assert!((1..=12).contains(&sample.fields[4]) && sample.fields[6] <= 6);
        assert!(at(u64::MAX).is_err());
    }
}
