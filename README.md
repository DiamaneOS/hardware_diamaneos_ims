# DiamaneOS IMS connectivity

Android source location: `hardware/diamaneos/ims`.

The modem's IMS and emergency data-connection path, plus optional Wi-Fi link
reporting for native Qualcomm IWLAN. Android's IMS service and the modem's IMS
signalling, voice and carrier emergency location are not replaced. Native and
carrier behavior: see [verification](docs/verification.md).

- `dcm/`: memory-safe QMI DCM codec and bounded connection state machine
  (separate SIMs, shared address-family requests, reserved emergency capacity).
- `daemon/`: QRTR/Binder adapter with a dedicated UID, enforcing policy, bounded
  queues, an arm64 syscall filter and an explicitly configured modem.
- `broker/`: requests telephony's IMS/EIMS networks, including carrier IWLAN.
  Reports restricted-bearer metadata only: no Internet, location or SMS
  permission, no app traffic, no non-local sockets (SELinux). Its own UID's
  VPN/firewall block does not describe the modem bearer.
- `wlan/`: bounded DSD protocol and acknowledgement state machine.
- `wlan-runtime/`: isolated QRTR client and authenticated Binder snapshot adapter.
- `wlan-observer/`: passive observation of the connected Wi-Fi link; no
  Internet, location or phone-state permission.
- `integration/`: emergency-APN comparison tooling and source IWLAN provenance.
- `imeiprov/`: one-shot [IMEI provisioning](docs/imei-provisioning.md)
  (`imeiprovd`): reads the read-only traceability partition, writes modem NV 550
  over the Fairphone TCL QMI service; own vendor UID and SELinux domain.

## Wi-Fi calling

- [Wi-Fi calling](docs/wifi-calling.md) describes the VoWiFi integration.
- The [Wi-Fi reporter](docs/wlan-reporting-design.md) keeps Android
  observations apart from modem access and completes supported notification
  requests without inventing quality results.
- It is selected only by including `wlan-product.mk` and `wlan-board.mk`
  alongside native Qualcomm IWLAN. User and userdebug builds use the same
  protocol path.
- Final native and device qualification is outstanding.

AML (Advanced Mobile Location) is not part of DiamaneOS. Its app
([platform_packages_apps_EmergencyLocation](https://github.com/DiamaneOS/platform_packages_apps_EmergencyLocation))
is not in the build and not needed to build or test this repository.

## Development

Needs Rust/Cargo, Python 3 and JDK 17 or later:

```sh
cargo fetch --locked
./tests/run-host-tests.sh
```

- Host tests use synthetic inputs: no QRTR service, calls, SMS or emergency
  endpoints.
- `tests/device-check --serial "$ANDROID_SERIAL" --iwlan qti` is a read-only
  inventory of Android prerequisites for the native Qualcomm path (`--iwlan
  aosp` only for a product selecting that alternative). It does not test
  carrier registration or delivery.

See also [architecture](docs/architecture.md), [protocol](docs/dcm-protocol.md)
and [Android integration](docs/integration.md).
