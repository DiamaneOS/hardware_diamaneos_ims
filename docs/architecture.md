# Architecture and security boundaries

Each process runs as its own identity:

| Component | Privilege | Excluded capabilities |
| --- | --- | --- |
| IMS DCM daemon | Vendor UID 2990, QRTR to the configured modem, own Binder service | Internet, SMS, location, arbitrary Binder peers, file writes, capabilities |
| IMS broker | `CONNECTIVITY_USE_RESTRICTED_NETWORKS` | Internet traffic, modem sockets, location, SMS |

The daemon owns all protocol state on one thread. Binder callbacks enqueue bounded
messages. Session IDs are bounded to 20–98, with capacity reserved for emergency
requests. One group per slot/type holds the Android request; IPv4/IPv6 sessions
reference that group. Reports carry generation serials. Address disappearance,
network loss and unavailable requests never produce a success address. No custom
emergency setup timeout competes with the modem's retry policy.

- One thread owns all protocol state. Binder callbacks enqueue bounded messages.
- Session IDs 20–98, with emergency capacity reserved (see
  [budgets](#fixed-values-and-downstream-budgets)). One group per slot/type
  holds the Android request, and IPv4/IPv6 sessions reference it. Reports carry
  generation serials.
- Address disappearance, network loss and unavailable requests never produce a
  success address.
- No custom emergency setup timeout competes with the modem's retry policy.
- Vendor Rust Binder does not expose caller SELinux IDs. So the daemon refuses
  permissive SELinux, policy limits registration and calls to the broker
  process, and runtime checks pin the registered primary-user app UID. This
  policy/code contract cannot by itself authenticate an arbitrary installed
  package, so policy and device isolation both need validation.

The broker uses a fresh callback object for each daemon connection. Stale death
notifications and queued old callbacks cannot affect its successor. Network
replacement waits for matching capabilities and link properties before reporting. A blocked network is not advertised as usable.
Tentative, failed-DAD, deprecated, link-local, multicast and unspecified addresses
are not advertised.

## Wi-Fi observer and reporter

[Separate identities](wlan-reporting-design.md) that do not widen the broker's
authority.

- The observer obtains same-link BSSID, addresses and bounded DNS metadata.
- The reporter validates and sends the verified DSD request subset.
- Numeric status contains no network identifiers.
- Check effective permission flags on the device: a manifest declaration alone
  does not prove a grant is user-revocable.

## Scope and limits

- The Android IMS service, modem firmware and carrier stack keep call routing,
  voice/SMS and carrier emergency location. This component supplies normal and
  emergency data connections, not AML delivery.
- AML is not part of DiamaneOS. Its app and framework event patch
  ([platform_packages_apps_EmergencyLocation](https://github.com/DiamaneOS/platform_packages_apps_EmergencyLocation))
  are outside the build. Neither the daemon nor the broker needs them.
- Host tests do not cover the full Soong build and linking, enforcing policy,
  native syscall-filter execution or modem and carrier behavior.
- Simulated emergency scenarios verify only the implemented state/protocol
  boundaries, not real network fallback or responder location receipt.
