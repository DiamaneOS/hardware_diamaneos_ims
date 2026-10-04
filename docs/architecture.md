# Architecture and security boundaries

The DCM path and optional Wi-Fi reporting path use separate process identities:

| Component | Privilege | Excluded capabilities |
| --- | --- | --- |
| IMS DCM daemon | Vendor UID 2990; QRTR to the configured modem; own Binder service | Internet, SMS, location, arbitrary Binder peers, file writes, capabilities |
| IMS broker | `CONNECTIVITY_USE_RESTRICTED_NETWORKS`, Network permission for policy accounting | Direct IP/modem sockets, location, SMS |
| Wi-Fi reporter (optional) | UID 2991; configured modem DSD endpoint; private Binder service | Internet, location, SMS, routing changes, capabilities |
| Wi-Fi observer (optional) | Passive Wi-Fi/network reads; `RADIO_SCAN_WITHOUT_LOCATION` | Internet permission, scans, location, phone-state and network-setting permissions |

The daemon owns all protocol state on one thread. Binder callbacks enqueue bounded
messages. Session IDs are bounded to 20–98, with capacity reserved for emergency
requests. One group per slot/type holds the Android request; IPv4/IPv6 sessions
reference that group. Reports carry generation serials. Address disappearance,
network loss and unavailable requests never produce a success address. No custom
emergency setup timeout competes with the modem's retry policy. Broker loss revokes
its networks at once but keeps groups and sessions for a downstream 15-second
grace period, so a broker that registers again re-files them within seconds.
Without one, the sessions then end with terminal results.

Vendor Rust Binder does not expose caller SELinux IDs. Therefore the daemon
refuses permissive SELinux, policy restricts registration/calls to the broker
process, and runtime checks pin the registered primary-user app UID. This is a
combined policy/code contract, not a UID check that can independently authenticate
an arbitrary installed package. Both policy and device isolation need validation.

The broker uses a fresh callback object for each daemon connection. Stale death
notifications and queued old callbacks cannot affect its successor. Network
replacement waits for matching capabilities and link properties before reporting. The broker reports only restricted cellular IMS/EIMS metadata. Its own UID firewall
or VPN-blocked state does not gate the modem bearer; it holds no INTERNET permission,
opens no IP sockets and grants no application access or VPN bypass.
Both address consumers share Android's preferred-address flag rule: failed-DAD
and deprecated addresses are rejected; tentative addresses require the optimistic
flag. Link-local, multicast, loopback and unspecified addresses remain excluded.
This changes eligibility only, not each consumer's existing address ordering.

The Network permission does not by itself grant direct socket access through
SELinux. These socket restrictions do not prove that every indirect network path
through permitted Android IPC is impossible; audit both permission and Binder
boundaries. The [Wi-Fi observer/reporter](wlan-reporting-design.md) uses separate identities
and does not widen this broker's authority. The observer obtains same-link BSSID,
addresses and bounded DNS metadata; the reporter validates and sends the verified
DSD request subset. Numeric status contains no network identifiers. Effective
permission flags must be inspected on-device: a manifest declaration alone does
not prove that a grant is user-revocable.

## Scope and remaining verification

The existing Android IMS service, modem firmware and carrier stack retain call
routing, voice/SMS and carrier emergency-location responsibilities. This component
supplies normal/emergency data connections; it is not an AML delivery service.

AML is not part of DiamaneOS. The AML app and its framework event patch are in
[platform_packages_apps_EmergencyLocation](https://github.com/DiamaneOS/platform_packages_apps_EmergencyLocation),
outside the build. Neither the daemon nor broker requires that repository or its
permissions.

Full Soong/link, enforcing policy, native syscall-filter execution and modem/carrier
behavior remain unqualified by host tests. Simulated emergency scenarios verify
only the implemented state/protocol boundaries, not real network fallback or
responder location receipt.

## Fixed values and downstream budgets

The PDP/session ID range20–98 is the authenticated stock DCM contract; capacity
is derived from those inclusive endpoints. Reserving eight IDs for emergency
admission and admitting at most four modem clients are downstream containment
choices. Emergency reservation protects space from normal IMS exhaustion within
the independent client limit; it cannot promise admission of unlimited clients.
Wire message/TLV values, QRTR family and arm64 seccomp offsets are fixed protocol
or Linux UAPI facts. They belong in named definitions with provenance, not
user-editable configuration. Device-selected modem node and slot count remain
product configuration. Limits and timings should change only with their
lifecycle, memory and device-qualification implications reviewed together.

The FP6 kernel integration reserves QRTR port `0x7fff` for the single local IMS
DCM instance (`0x302`). This is a downstream role allocation at the top of the
normal ephemeral range, not a carrier or modem protocol constant. Binding and
user send/receive operations require both the dedicated vendor UID and the
`diamaneos_imsdcm` SELinux subject. Other auto-bound QRTR clients use the remaining
range. Service records advertise their actual source node/port. The reserved
port remains unavailable to other roles while unbound, so stale discovery cannot
route DCM traffic into an unrelated replacement socket. Stale discovery and
transaction continuity across restart still need native recovery qualification.
Pair this daemon with `CONFIG_QRTR_IMSDCM_OWNERSHIP=y`; an unpaired stock kernel
does not enforce the ownership boundary.
