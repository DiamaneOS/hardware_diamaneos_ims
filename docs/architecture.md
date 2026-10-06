# Architecture and security boundaries

Each process runs as its own identity:

| Component | Privilege | Excluded capabilities |
| --- | --- | --- |
| IMS DCM daemon | Vendor UID 2990; QRTR to the configured modem; own Binder service | Internet, SMS, location, arbitrary Binder peers, file writes, capabilities |
| IMS broker | `CONNECTIVITY_USE_RESTRICTED_NETWORKS` | Internet permission, direct IP/modem sockets, location, SMS |
| Wi-Fi reporter (optional) | UID 2991; configured modem DSD endpoint; private Binder service | Internet, location, SMS, routing changes, capabilities |
| Wi-Fi observer (optional) | Passive Wi-Fi/network reads; `RADIO_SCAN_WITHOUT_LOCATION` | Internet permission, scans, location, phone-state and network-setting permissions |

## DCM daemon

- One thread owns all protocol state; Binder callbacks enqueue bounded messages.
- Session IDs 20–98, with emergency capacity reserved (see
  [budgets](#fixed-values-and-downstream-budgets)). One group per slot/type
  holds the Android request; IPv4/IPv6 sessions reference it. Reports carry
  generation serials.
- Address disappearance, network loss and unavailable requests never produce a
  success address.
- No custom emergency setup timeout competes with the modem's retry policy.
- Vendor Rust Binder does not expose caller SELinux IDs. So the daemon refuses
  permissive SELinux, policy limits registration and calls to the broker
  process, and runtime checks pin the registered primary-user app UID. This
  policy/code contract cannot by itself authenticate an arbitrary installed
  package; policy and device isolation both need validation.

## Broker

- A fresh callback object per daemon connection, so stale death notifications
  and queued old callbacks cannot affect its successor.
- Network replacement waits for matching capabilities and link properties
  before reporting.
- Reports only restricted cellular IMS/EIMS metadata. Its own UID firewall or
  VPN block does not gate the modem bearer.
- No INTERNET permission, no IP sockets, no application access or VPN bypass.
- Both address consumers share Android's preferred-address flag rule: failed-DAD
  and deprecated addresses are rejected, tentative ones need the optimistic
  flag, and link-local, multicast, loopback and unspecified addresses stay
  excluded. This changes eligibility only, not address order.
- The socket restrictions do not rule out indirect network paths through
  permitted Android IPC; audit both permission and Binder boundaries.

## Wi-Fi observer and reporter

[Separate identities](wlan-reporting-design.md); they do not widen the broker's
authority.

- The observer obtains same-link BSSID, addresses and bounded DNS metadata.
- The reporter validates and sends the verified DSD request subset.
- Numeric status contains no network identifiers.
- Check effective permission flags on the device: a manifest declaration alone
  does not prove a grant is user-revocable.

## Scope and open verification

- The Android IMS service, modem firmware and carrier stack keep call routing,
  voice/SMS and carrier emergency location. This component supplies normal and
  emergency data connections, not AML delivery.
- AML is not part of DiamaneOS. Its app and framework event patch
  ([platform_packages_apps_EmergencyLocation](https://github.com/DiamaneOS/platform_packages_apps_EmergencyLocation))
  are outside the build; neither the daemon nor the broker needs them.
- Not qualified by host tests: the full Soong/link, enforcing policy, native
  syscall-filter execution, modem/carrier behavior.
- Simulated emergency scenarios verify only the implemented state/protocol
  boundaries, not real network fallback or responder location receipt.

## Fixed values and downstream budgets

- The PDP/session ID range 20–98 is the authenticated stock DCM contract;
  capacity derives from those inclusive endpoints.
- Downstream containment choices: eight IDs reserved for emergency admission,
  at most four modem clients. The reserve protects emergency space from normal
  IMS exhaustion within the client limit; it cannot promise admission of
  unlimited clients.
- Wire message/TLV values, the QRTR family and arm64 seccomp offsets are fixed
  protocol or Linux UAPI facts: named definitions with provenance, not
  user-editable configuration. Modem node and slot count are product
  configuration.
- Change limits and timings only with their lifecycle, memory and
  device-qualification effects reviewed together.

## QRTR port reservation

- The FP6 kernel integration reserves QRTR port `0x7fff` for the single local
  IMS DCM instance (`0x302`): a downstream role allocation at the top of the
  normal ephemeral range, not a carrier or modem constant.
- Binding and user send/receive need both the dedicated vendor UID and the
  `diamaneos_imsdcm` SELinux subject. Other auto-bound QRTR clients use the rest
  of the range; service records advertise their actual source node/port.
- While unbound, the port stays unavailable to other roles, so stale discovery
  cannot route DCM traffic into an unrelated replacement socket. Stale discovery
  and transaction continuity across restart still need native recovery
  qualification.
- Pair this daemon with `CONFIG_QRTR_IMSDCM_OWNERSHIP=y`; an unpaired stock
  kernel does not enforce the ownership boundary.
