# DiamaneOS emergency connectivity

Android source location: `hardware/diamaneos/ims`.

This is a development implementation, **not a qualified emergency-calling or
AML service**. It has not been deployed or tested with a carrier or emergency
call centre. Host simulations and Android API compilation cover only the
boundaries documented in [verification](docs/verification.md).

- `dcm/` is a memory-safe QMI DCM codec and state machine. It shares a broker
  request across IP families, separates SIMs and IMS/emergency networks,
  rejects stale reports and bounds sessions and messages.
- `daemon/` adapts that core to QRTR and Android Binder. It uses a dedicated
  vendor UID, enforcing SELinux, a bounded event queue and an arm64 seccomp
  allowlist. Device configuration must identify the modem node explicitly.
- `broker/` requests only Android telephony IMS/EIMS networks (including
  carrier IWLAN, which Android exposes with the cellular transport capability). It has no Internet,
  location or SMS permission. It closes requests on daemon loss and refuses
  stale callback epochs or subscription changes.
- `aml/` is a **separate app and UID**. It receives trusted outgoing emergency-call/SMS
  events, collects GNSS briefly, and supports bounded HTTPS/data-SMS messages.
  It cannot place, route or cancel a call, and has no modem access. Production
  routes are intentionally empty until verified against a receiver profile.
- `integration/` contains the small framework event-bridge patch and a tool
  for creating an emergency-APN candidate from authenticated stock XML. Neither
  is automatically applied. The bridge is maintained in the DiamaneOS frameworks-base fork.

The modem and the existing IMS telephony service still implement IMS signalling
and voice. This repository supplies their data-connection path; it does not
replace the entire modem IMS stack. See [Wi-Fi calling integration](docs/wifi-calling.md)
for the separate IWLAN stack and its qualification requirements.

## Development

With Rust/Cargo and JDK 17 or later:

```sh
cargo fetch --locked
./tests/run-host-tests.sh
```

Tests use synthetic identities and in-memory transports. They never use ADB,
place a call, publish a QRTR service, send SMS, or contact an AML endpoint.

See [architecture](docs/architecture.md), [protocol](docs/dcm-protocol.md),
[Android integration](docs/integration.md), and [AML receiver configuration](docs/aml-profile.md).
