// SPDX-License-Identifier: Apache-2.0
// Broker report checks. Documentation addresses (RFC 5737, RFC 3849) only.
use diamaneos_ims_dcm::protocol::PdnType;
use diamaneos_ims_runtime::broker_report::{network, request};

#[test]
fn request_accepts_known_types_slots_and_positive_serials_only() {
    let r = request(1, Some(PdnType::Ims), 7).unwrap();
    assert_eq!((r.key.slot, r.key.kind, r.serial), (1, PdnType::Ims, 7));
    assert!(request(-1, Some(PdnType::Emergency), 1).is_some());
    assert!(request(2, Some(PdnType::Ims), i32::MAX).is_some());
    assert!(request(3, Some(PdnType::Ims), 1).is_none());
    assert!(request(-2, Some(PdnType::Ims), 1).is_none());
    assert!(request(0, Some(PdnType::Ims), 0).is_none());
    assert!(request(0, Some(PdnType::Ims), -1).is_none());
    assert!(request(0, None, 1).is_none());
}

#[test]
fn network_takes_exact_address_widths() {
    let n = network(42, &[192, 0, 2, 1], &[], 1500).unwrap();
    assert_eq!(n.v4, Some("192.0.2.1".parse().unwrap()));
    assert_eq!((n.handle, n.v6, n.mtu), (42, None, 1500));
    let v6: [u8; 16] = "2001:db8::1".parse::<std::net::Ipv6Addr>().unwrap().octets();
    assert!(network(42, &[], &v6, 0).unwrap().v4.is_none());
    assert!(network(42, &[], &[], 1500).is_some());
    for bad in [&[192, 0, 2][..], &[192, 0, 2, 1, 0], &[0; 16]] {
        assert!(network(42, bad, &[], 1500).is_none());
    }
    assert!(network(42, &[], &v6[..15], 1500).is_none());
    assert!(network(42, &[], &[0; 17], 1500).is_none());
}

#[test]
fn network_rejects_zero_handles_bad_mtus_and_unusable_addresses() {
    assert!(network(0, &[192, 0, 2, 1], &[], 1500).is_none());
    assert!(network(-1, &[192, 0, 2, 1], &[], 1500).is_some());
    assert!(network(42, &[192, 0, 2, 1], &[], -1).is_none());
    assert!(network(42, &[192, 0, 2, 1], &[], 65535).is_some());
    assert!(network(42, &[192, 0, 2, 1], &[], 65536).is_none());
    for address in [[0, 0, 0, 0], [127, 0, 0, 1], [224, 0, 0, 1], [169, 254, 0, 1], [255; 4]] {
        assert!(network(42, &address, &[], 1500).is_none());
    }
    let link_local: [u8; 16] = "fe80::1".parse::<std::net::Ipv6Addr>().unwrap().octets();
    assert!(network(42, &[], &link_local, 1500).is_none());
}
