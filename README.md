# DiamaneOS IMS connectivity

Android source location: `hardware/diamaneos/ims`.

This repository provides the modem's IMS and emergency data-connection path and
optional Wi-Fi link reporting for native Qualcomm IWLAN. It
does not replace the existing Android IMS service or the modem's IMS signalling,
voice and carrier emergency-location implementation. Native and carrier behavior
remain subject to the verification limits in [verification](docs/verification.md).

- `dcm/`: memory-safe QMI DCM codec and bounded connection state machine, with
  separate SIMs, shared address-family requests and reserved emergency capacity.
- `daemon/`: QRTR/Binder adapter, dedicated UID, enforcing policy, bounded queues
  and arm64 syscall filter. Device configuration identifies the modem explicitly.
- `broker/`: requests Android telephony IMS/EIMS networks, including carrier
  IWLAN as exposed by telephony. It declares the revocable Network permission
  for Android's network-policy accounting; SELinux forbids direct IP/modem
  sockets. It has no location or SMS permission.
- `wlan/`: bounded DSD protocol and acknowledgement state machine.
- `wlan-runtime/`: isolated QRTR client and authenticated Binder snapshot adapter.
- `wlan-observer/`: passive observations of the connected Wi-Fi link, with no
  Internet, location or phone-state permission.
- `integration/`: emergency-APN comparison tooling and source IWLAN provenance.

VoWiFi integration is described in [Wi-Fi calling](docs/wifi-calling.md).
The device-selected [Wi-Fi reporter](docs/wlan-reporting-design.md) separates
Android observations from modem access. It completes supported notification requests without inventing quality results.
User and userdebug builds use the same protocol path. Final native and device
qualification remain required before release. It is selected only by explicitly including `wlan-product.mk` and
`wlan-board.mk` alongside the native Qualcomm IWLAN path.
AML (Advanced Mobile Location) is not part of DiamaneOS. Its app, in
[platform_packages_apps_EmergencyLocation](https://github.com/DiamaneOS/platform_packages_apps_EmergencyLocation),
is not in the build and is not needed to build or test this repository.

## Development

With Rust/Cargo, Python 3 and JDK 17 or later:

```sh
cargo fetch --locked
./tests/run-host-tests.sh
```

Host tests use synthetic inputs; they do not publish a QRTR service, place a call,
send SMS or contact an emergency endpoint. `tests/device-check --serial "$ANDROID_SERIAL" --iwlan qti`
provides a separate read-only inventory of Android prerequisites for the native
Qualcomm path. Use `--iwlan aosp` only for a product selecting that alternative. It does not test carrier registration or delivery.

See [architecture](docs/architecture.md), [protocol](docs/dcm-protocol.md) and
[Android integration](docs/integration.md).
