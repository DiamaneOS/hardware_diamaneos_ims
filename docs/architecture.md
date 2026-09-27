# Architecture and security boundaries

There are two independently privileged processes in this repository:

| Component | Privilege | Excluded capabilities |
| --- | --- | --- |
| IMS DCM daemon | Vendor UID 2990; QRTR to the configured modem; own Binder service | Internet, SMS, location, arbitrary Binder peers, file writes, capabilities |
| IMS broker | `CONNECTIVITY_USE_RESTRICTED_NETWORKS` | Internet traffic, modem sockets, location, SMS |

The daemon owns all protocol state on one thread. Binder callbacks enqueue bounded
messages. Session IDs are bounded to 20–98, with capacity reserved for emergency
requests. One group per slot/type holds the Android request; IPv4/IPv6 sessions
reference that group. Reports carry generation serials. Address disappearance,
network loss and unavailable requests never produce a success address. No custom
emergency setup timeout competes with the modem's retry policy.

Vendor Rust Binder does not expose caller SELinux IDs. Therefore the daemon
refuses permissive SELinux, policy restricts registration/calls to the broker
process, and runtime checks pin the registered primary-user app UID. This is a
combined policy/code contract, not a UID check that can independently authenticate
an arbitrary installed package. Both policy and device isolation need validation.

The broker uses a fresh callback object for each daemon connection. Stale death
notifications and queued old callbacks cannot affect its successor. Network
replacement waits for matching capabilities and link properties before reporting. A blocked network is not advertised as usable.
Tentative, failed-DAD, deprecated, link-local, multicast and unspecified addresses
are not advertised.

## Scope and remaining verification

The existing Android IMS service, modem firmware and carrier stack retain call
routing, voice/SMS and carrier emergency-location responsibilities. This component
supplies normal/emergency data connections; it is not an AML delivery service.

The deferred AML app and its framework event patch are maintained separately in
[platform_packages_apps_EmergencyLocation](https://github.com/DiamaneOS/platform_packages_apps_EmergencyLocation).
Neither the daemon nor broker requires that repository or its permissions.

Full Soong/link, enforcing policy, native syscall-filter execution and modem/carrier
behavior remain unqualified by host tests. Simulated emergency scenarios verify
only the implemented state/protocol boundaries, not real network fallback or
responder location receipt.
