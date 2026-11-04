// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! Kernel input: bytes read from the traceability block device (IMEI ASCII
//! windows, Bluetooth address, Wi-Fi MAC) and modem NV 550 values.
//! Nothing decoded here is printed: only the redacted Debug is checked.
#![no_main]

use diamaneos_imeiprov::bdaddr::{self, BdAddr};
use diamaneos_imeiprov::imei::{check_pair, Imei, NV550_LEN};
use diamaneos_imeiprov::wlanmac::{self, WlanMac};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // The traceability reads are fixed windows, but the decoders take slices.
    if let Ok(imei) = Imei::from_ascii(data) {
        assert_eq!(data.len(), 15);
        assert!(data.iter().all(u8::is_ascii_digit));
        let back = Imei::from_nv550_bcd(&imei.to_nv550_bcd()).expect("encoded value decodes");
        assert_eq!(back, imei);
        assert_eq!(format!("{imei:?}"), "Imei(<redacted>)");
        let _ = check_pair(&imei, &back).usable();
    }
    if let Ok(imei) = Imei::from_nv550_bcd(data) {
        assert!(data.len() >= NV550_LEN);
        assert_eq!(&imei.to_nv550_bcd()[..], &data[..NV550_LEN]);
        assert!(imei.digits().iter().all(|d| *d <= 9));
    }
    if let Ok(address) = BdAddr::from_trace(data) {
        assert_eq!(data.len(), bdaddr::LEN);
        let value = address.property_value();
        assert_eq!(value.len(), bdaddr::PROPERTY_LEN);
        assert_eq!(format!("{address:?}"), "BdAddr(<redacted>)");
    }
    if let Ok(mac) = WlanMac::from_trace(data) {
        assert_eq!(data.len(), wlanmac::LEN);
        let file = mac.mac_file();
        assert_eq!(file.len(), wlanmac::FILE_LEN);
        assert!(file.starts_with("Intf0MacAddress=") && file.ends_with("\nEND\n"));
        assert_eq!(format!("{mac:?}"), "WlanMac(<redacted>)");
    }
    if let Ok(digits) = <[u8; 15]>::try_from(data) {
        if let Ok(imei) = Imei::from_digits(digits) {
            assert_eq!(Imei::from_nv550_bcd(&imei.to_nv550_bcd()), Ok(imei));
        }
    }
});
