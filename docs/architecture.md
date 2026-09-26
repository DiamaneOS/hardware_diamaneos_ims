# Architecture and security boundaries

There are three independently privileged processes:

| Component | Privilege | Excluded capabilities |
| --- | --- | --- |
| IMS DCM daemon | Vendor UID 2990; QRTR to the configured modem; own Binder service | Internet, SMS, location, arbitrary Binder peers, file writes, capabilities |
| IMS broker | `CONNECTIVITY_USE_RESTRICTED_NETWORKS` | Internet traffic, modem sockets, location, SMS |
| Emergency location | A trusted call event, limited location collection, device/country metadata, SMS/HTTPS delivery | QRTR, IMS Binder service, placing/cancelling/routing calls |

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

AML runs independently. The platform bridge dispatches asynchronously after the
existing emergency notification path; voice never waits for it. The receiver
checks system-server sender identity. Its read-only profile selects destinations;
an incoming event cannot choose a URL, SMS recipient or location. Location bypass
is request-scoped and time-bounded; it does not change the global location switch.
Coordinates, identifiers, APNs and packet contents are absent from logs. HTTPS
keeps platform TLS verification and refuses redirects. Data SMS avoids ordinary
text-message persistence; receiver-specific packing and port must be verified.

## Remaining qualification

The current AML collector uses GNSS, not Google's fused Wi-Fi/cell positioning.
Outgoing emergency calls and SMS have distinct protected activation events.
No-SIM HTTPS and missing metadata are supported only when explicitly accepted by
the read-only receiver profile; unknown fields are omitted, never fabricated.
The framework supplies the actual phone index, so no default SIM is guessed.
SMS requires complete metadata and the same active subscription; roaming SMS is
off unless its routing is explicitly verified. Recursive SMS destinations are
rejected. HTTPS uses the standard full IMSI unless a receiver explicitly accepts
the reduced form. Identity is read only for a matching profile and retained in
memory for the bounded operation. A 2xx response is transport receipt, not proof that
a call taker received or matched the location. SMS submission has no delivery
claim. Full framework/Soong builds, enforcing policy, seccomp execution, carrier
registration and physical call/audio/radio fallback remain unqualified.
