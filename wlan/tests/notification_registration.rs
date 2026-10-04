// SPDX-License-Identifier: Apache-2.0
use diamaneos_wlan_reporting::{
    notification_registration, session::Session, Error, DEFAULT_PROFILE_STATUS,
};

fn ack(request: &[u8]) -> [u8; 14] {
    [
        2, request[1], request[2], request[3], request[4], 7, 0, 2, 4, 0, 0, 0, 0, 0,
    ]
}

#[test]
fn fixed_stock_registration_has_no_extra_fields() {
    // Independent golden bytes from authenticated IDL0x38 and stock stores.
    assert_eq!(
        notification_registration(0x1234).unwrap(),
        [0, 0x34, 0x12, 0x38, 0, 8, 0, 0x12, 1, 0, 1, 0x14, 1, 0, 1]
    );
    assert_eq!(notification_registration(0), Err(Error::ZeroTransaction));
}

#[test]
fn notification_registration_is_acknowledged_before_normal_reconciliation() {
    for slot in 1..=2 {
        let mut session = Session::new(slot, 1).unwrap();
        let bind = session.poll(0).unwrap();
        assert_eq!(bind[3], 0x27);
        session.receive(&ack(&bind));
        let registration = session.poll(1).unwrap();
        assert_eq!(registration[3], 0x38);
        assert_eq!(session.diagnostics().stage, 8);
        // An old binding ACK cannot confirm notification registration.
        session.receive(&ack(&bind));
        assert!(session.poll(2).is_none());
        session.receive(&ack(&registration));
        let clear = session.poll(3).unwrap();
        assert_eq!(u16::from(clear[3]), DEFAULT_PROFILE_STATUS);
        session.receive(&ack(&registration));
        assert!(session.poll(4).is_none());
    }
}

#[test]
fn rejected_or_unanswered_registration_never_reports_availability() {
    for rejected in [false, true] {
        let mut session = Session::new(1, 1).unwrap();
        let bind = session.poll(0).unwrap();
        session.receive(&ack(&bind));
        let registration = session.poll(0).unwrap();
        if rejected {
            let mut reply = ack(&registration);
            reply[10] = 1;
            reply[12] = 5;
            session.receive(&reply);
        } else {
            assert_eq!(session.poll(2000).unwrap(), registration);
            assert_eq!(session.poll(4000).unwrap(), registration);
            assert!(session.poll(6000).is_none());
        }
        assert!(session.failed());
        assert_eq!(session.diagnostics().operation, 0x38);
        assert!(session.poll(u64::MAX).is_none());
    }
}
