// SPDX-License-Identifier: Apache-2.0
use diamaneos_ims_dcm::protocol::{Encoder, Kind};
use diamaneos_wlan_reporting::profiles::{Change, Error, Registry};

fn init(profile: u32, id: Option<u64>) -> Vec<u8> {
    let mut p = Encoder::new(Kind::Indication, 0, 0x45);
    p.tlv(1, &profile.to_le_bytes()).unwrap();
    if let Some(id) = id {
        p.tlv(0x13, &id.to_le_bytes()).unwrap();
    }
    p.finish()
}
fn select(mask: u64, id: Option<u64>) -> Vec<u8> {
    let mut p = Encoder::new(Kind::Indication, 0, 0x3f);
    p.tlv(1, &mask.to_le_bytes()).unwrap();
    if let Some(id) = id {
        p.tlv(0x11, &id.to_le_bytes()).unwrap();
    }
    p.finish()
}

#[test]
fn normalized_zero_identity_and_exact_negative_request_are_preserved() {
    let mut r = Registry::new(1).unwrap();
    assert_eq!(r.receive(&init(35, None)), Ok(Change::Initialized));
    assert_eq!(r.receive(&init(35, Some(0))), Ok(Change::Duplicate));
    assert!(r.report_due().is_none());
    r.receive(&select(1_u64 << 63, Some(0))).unwrap();
    let token = r.report_due().unwrap();
    assert_eq!(
        token.inconclusive(0x1234).unwrap(),
        [
            0, 0x34, 0x12, 0x43, 0, 21, 0, 1, 4, 0, 35, 0, 0, 0, 0x10, 4, 0, 1, 0, 0, 0, 0x11, 4,
            0, 3, 0, 0, 0,
        ]
    );
    assert!(token.inconclusive(0).is_err());
    assert!(r.acknowledged(token));
    assert!(!r.is_current(token));
    assert!(!r.acknowledged(token));
    assert!(r.report_due().is_none());
}

#[test]
fn nonzero_identity_is_echoed_only_from_the_owned_selected_entry() {
    let mut r = Registry::new(1).unwrap();
    let id = 0x0123_4567_89ab_cdef;
    r.receive(&init(36, Some(id))).unwrap();
    r.receive(&select(1 << 3, Some(id))).unwrap();
    let p = r.report_due().unwrap().inconclusive(1).unwrap();
    assert_eq!(p.len(), 39);
    assert_eq!(&p[5..7], &[32, 0]);
    assert_eq!(
        &p[28..],
        &[0x12, 8, 0, 0xef, 0xcd, 0xab, 0x89, 0x67, 0x45, 0x23, 0x01]
    );
}

#[test]
fn selection_cannot_initialize_and_duplicate_init_cannot_restart_reported_work() {
    let mut r = Registry::new(1).unwrap();
    r.receive(&select(1 << 3, None)).unwrap();
    assert!(r.report_due().is_none());
    r.receive(&init(36, None)).unwrap();
    assert!(r.report_due().is_none());
    r.receive(&select(1 << 3, None)).unwrap();
    let token = r.report_due().unwrap();
    assert!(r.acknowledged(token));
    assert_eq!(r.receive(&init(36, None)), Ok(Change::Duplicate));
    r.receive(&select(1 << 3, None)).unwrap();
    assert!(r.report_due().is_none());
}

#[test]
fn clear_is_scoped_and_old_ack_cannot_mark_a_reinitialized_entry_or_context() {
    let mut r = Registry::new(1).unwrap();
    for id in [1, 2] {
        r.receive(&init(36, Some(id))).unwrap();
        r.receive(&select(1 << 3, Some(id))).unwrap();
    }
    let old = r.report_due().unwrap();
    r.receive(&select(0, Some(1))).unwrap();
    assert!(!r.is_current(old));
    assert!(!r.acknowledged(old));
    let second = r.report_due().unwrap();
    assert!(r.acknowledged(second));
    r.receive(&init(36, Some(1))).unwrap();
    r.receive(&select(1 << 3, Some(1))).unwrap();
    assert!(!r.acknowledged(old));
    assert!(r.report_due().is_some());
    let mut replacement = Registry::new(2).unwrap();
    replacement.receive(&init(36, Some(1))).unwrap();
    replacement.receive(&select(1 << 3, Some(1))).unwrap();
    assert!(!replacement.is_current(old));
    assert!(!replacement.acknowledged(old));
    assert!(Registry::new(0).is_none());
}

#[test]
fn capacity_and_rejected_masks_do_not_silently_evict_or_mutate_owned_entries() {
    let mut r = Registry::new(1).unwrap();
    for id in 1..=40 {
        r.receive(&init(36, Some(id))).unwrap();
    }
    r.receive(&select(1 << 3, Some(1))).unwrap();
    let token = r.report_due().unwrap();
    assert_eq!(r.receive(&init(36, Some(41))), Err(Error::Capacity));
    assert_eq!(
        r.receive(&select(1, Some(1))),
        Err(Error::UnmappedSelection)
    );
    assert!(r.is_current(token));
    let mut invalid = select(0, Some(1));
    invalid[5] += 1;
    assert_eq!(r.receive(&invalid), Err(Error::Malformed));
    assert!(r.is_current(token));
    r.receive(&select(0, Some(40))).unwrap();
    assert_eq!(r.receive(&init(36, Some(41))), Ok(Change::Initialized));
    assert!(r.is_current(token));
}
