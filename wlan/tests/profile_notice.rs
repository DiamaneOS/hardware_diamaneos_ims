// SPDX-License-Identifier: Apache-2.0
use diamaneos_ims_dcm::protocol::{Encoder, Kind};
use diamaneos_wlan_reporting::profile_notice::Notice;
use diamaneos_wlan_reporting::session::{Observation, Session};

fn packet(id: u16, fields: &[(u8, &[u8])]) -> Vec<u8> {
    let mut out = Encoder::new(Kind::Indication, 0, id);
    for (tag, value) in fields {
        out.tlv(*tag, value).unwrap();
    }
    out.finish()
}

#[test]
fn initialization_is_bounded_and_retains_only_the_type() {
    let minimal = packet(0x45, &[(1, &4_u32.to_le_bytes())]);
    assert_eq!(minimal.len(), 14);
    assert_eq!(Notice::parse(&minimal), Some(Notice::Initialized(4)));
    let full = packet(
        0x45,
        &[
            (1, &43_u32.to_le_bytes()),
            (0x10, &(-85_i16).to_le_bytes()),
            (0x11, &(-75_i16).to_le_bytes()),
            (0x14, &(-85_i16).to_le_bytes()),
            (0x15, &(-75_i16).to_le_bytes()),
            (0x16, &(-85_i16).to_le_bytes()),
            (0x17, &(-75_i16).to_le_bytes()),
            // Synthetic bytes, not a device or subscriber fixture.
            (0x12, &[10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            (0x13, &u64::MAX.to_le_bytes()),
        ],
    );
    assert_eq!(full.len(), 69);
    assert_eq!(Notice::parse(&full), Some(Notice::Initialized(43)));
    for profile in [0, 3, 44, u32::MAX] {
        assert_eq!(
            Notice::parse(&packet(0x45, &[(1, &profile.to_le_bytes())])),
            None
        );
    }
}

#[test]
fn all_selection_bits_map_without_turning_observations_into_active_state() {
    for profile in 4_u8..=43 {
        let bit = match profile {
            4..=35 => profile + 28,
            36..=42 => profile - 33,
            _ => 16,
        };
        let Notice::Selected(selection) =
            Notice::parse(&packet(0x3f, &[(1, &(1_u64 << bit).to_le_bytes())])).unwrap()
        else {
            panic!("selection expected");
        };
        for other in 0_u8..=255 {
            assert_eq!(selection.contains(other), other == profile);
        }
        assert!(!selection.has_unmapped_bits());
    }
    let full = packet(
        0x3f,
        &[
            (1, &u64::MAX.to_le_bytes()),
            (0x10, &[10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            (0x11, &u64::MAX.to_le_bytes()),
        ],
    );
    assert_eq!(full.len(), 43);
    let Notice::Selected(selection) = Notice::parse(&full).unwrap() else {
        panic!("selection expected");
    };
    assert!(selection.has_unmapped_bits());
    assert!((4..=43).all(|p| selection.contains(p)));
    let Notice::Selected(empty) =
        Notice::parse(&packet(0x3f, &[(1, &0_u64.to_le_bytes())])).unwrap()
    else {
        panic!("selection expected");
    };
    assert!(!(4..=43).any(|p| empty.contains(p)));
}

#[test]
fn framing_unknown_fields_duplicates_and_variable_lengths_fail_closed() {
    for (id, required_tag, required, array_tag) in [
        (0x45, 1, 4_u32.to_le_bytes().to_vec(), 0x12),
        (0x3f, 1, 0_u64.to_le_bytes().to_vec(), 0x10),
    ] {
        let valid = packet(id, &[(required_tag, &required)]);
        for end in 0..valid.len() {
            assert_eq!(Notice::parse(&valid[..end]), None);
        }
        for array in [vec![], vec![1], vec![0, 1], vec![11; 12]] {
            assert_eq!(
                Notice::parse(&packet(
                    id,
                    &[(required_tag, &required), (array_tag, &array)]
                )),
                None
            );
        }
        assert!(
            Notice::parse(&packet(id, &[(required_tag, &required), (array_tag, &[0])])).is_some()
        );
        assert_eq!(
            Notice::parse(&packet(id, &[(required_tag, &required), (0xff, &[0])])),
            None
        );
        let mut duplicate = valid.clone();
        duplicate.extend_from_slice(&valid[7..]);
        let body_length = (duplicate.len() - 7) as u16;
        duplicate[5..7].copy_from_slice(&body_length.to_le_bytes());
        assert_eq!(Notice::parse(&duplicate), None);
        for kind in [0, 2, 3, 255] {
            let mut wrong = valid.clone();
            wrong[0] = kind;
            assert_eq!(Notice::parse(&wrong), None);
        }
    }
    assert_eq!(Notice::parse(&packet(0x41, &[(1, &[0])])), None);
}

#[test]
fn optional_fields_have_exact_wire_widths_and_do_not_change_the_result() {
    for (id, required, fields) in [
        (
            0x45,
            4_u32.to_le_bytes().to_vec(),
            vec![
                (0x10, 2),
                (0x11, 2),
                (0x14, 2),
                (0x15, 2),
                (0x16, 2),
                (0x17, 2),
                (0x13, 8),
            ],
        ),
        (0x3f, 0_u64.to_le_bytes().to_vec(), vec![(0x11, 8)]),
    ] {
        let expected = Notice::parse(&packet(id, &[(1, &required)]));
        for (tag, size) in fields {
            for n in 0..=size + 1 {
                let value = vec![0x55; n];
                let result = Notice::parse(&packet(id, &[(1, &required), (tag, &value)]));
                assert_eq!(result, if n == size { expected } else { None });
            }
        }
    }
}

fn acknowledge(session: &mut Session, request: &[u8]) {
    session.receive(&[
        2, request[1], request[2], request[3], request[4], 7, 0, 2, 4, 0, 0, 0, 0, 0,
    ]);
}

#[test]
fn private_observations_require_bind_and_do_not_consume_a_pending_control_ack() {
    let init = packet(0x45, &[(1, &4_u32.to_le_bytes())]);
    let mut session = Session::new_with_diagnostic_registration(1).unwrap();
    session.observe(Observation {
        enabled: false,
        network: None,
    });
    let bind = session.poll(0).unwrap();
    session.receive(&init);
    assert_eq!(session.profile_diagnostics().initialization_counts()[0], 0);
    assert!(session.poll(1).is_none());
    acknowledge(&mut session, &bind);
    let registration = session.poll(2).unwrap();
    session.receive(&init);
    assert_eq!(session.profile_diagnostics().initialization_counts()[0], 1);
    assert!(session.poll(3).is_none());
    acknowledge(&mut session, &registration);
    assert_eq!(session.poll(4).unwrap()[3], 0x20);

    let mut ordinary = Session::new(1).unwrap();
    ordinary.observe(Observation {
        enabled: false,
        network: None,
    });
    let bind = ordinary.poll(0).unwrap();
    acknowledge(&mut ordinary, &bind);
    ordinary.receive(&init);
    assert_eq!(ordinary.profile_diagnostics().initialization_counts()[0], 0);
}

#[test]
fn diagnostic_counts_distinguish_zero_selection_repeats_rejection_and_context_reset() {
    let mut session = Session::new_with_diagnostic_registration(1).unwrap();
    session.observe(Observation {
        enabled: false,
        network: None,
    });
    let bind = session.poll(0).unwrap();
    acknowledge(&mut session, &bind);
    for _ in 0..1000 {
        session.receive(&packet(0x45, &[(1, &4_u32.to_le_bytes())]));
    }
    session.receive(&packet(0x3f, &[(1, &0_u64.to_le_bytes())]));
    session.receive(&packet(0x3f, &[(1, &((1_u64 << 32) | 1).to_le_bytes())]));
    session.receive(&packet(0x45, &[(1, &44_u32.to_le_bytes())]));
    let counts = session.profile_diagnostics();
    assert_eq!(counts.initialization_counts()[0], 1000);
    assert_eq!(counts.selection_counts()[0], 1);
    assert_eq!(counts.selection_messages, 2);
    assert_eq!(counts.unmapped_selections, 1);
    assert_eq!(counts.rejected_messages, 1);
    assert_eq!(counts.initialization_counts().len(), 40);
    let replacement = Session::new_with_diagnostic_registration(1).unwrap();
    assert!(replacement
        .profile_diagnostics()
        .initialization_counts()
        .iter()
        .all(|v| *v == 0));
    assert_eq!(replacement.profile_diagnostics().selection_messages, 0);
}
