# Verification and limits

`tests/run-host-tests.sh` exercises the production Rust codec/state machine,
Java network-callback state and emergency-APN merger. It uses no phone, QRTR
publication, SMS or external network. Addresses are synthetic documentation values.

Covered scenarios include separate SIMs, shared IPv4/IPv6 requests, delayed and
stale callbacks, modem reset, broker loss, normal-session exhaustion with emergency
capacity retained, invalid peers, missing IP families, malformed frames, duplicate
TLVs, instance destruction, timezone encoding, competing publishers, blocked
networks and network replacement. The APN tests preserve the supplied input and
check idempotence. These tests are independent of the deferred AML repository.

Additional development checks include Java compilation against Android 17 SDK
modules, AIDL dump comparison, Linux host tests and arm64 Rust metadata compilation.
They are not full Soong/linker, SELinux or native syscall-filter verification.

The current verification plan separates ordinary peer-phone VoLTE/VoWiFi tests
from simulated emergency scenarios. Simulation can check code paths and modeled
failures; it cannot confirm that a real emergency call reaches a responder or that
a carrier delivers location. Preserve that evidence distinction without requiring
unplanned live emergency calls to run this suite.

After a platform build is authorized, `sh tests/run-platform-tests.sh emulator-SERIAL`
selects the existing Android mock suites for emergency state/number handling,
dual-SIM data selection, IMS call tracking, TeleService routing, GNSS callbacks,
carrier configuration and entitlement. Run it in the candidate source environment
against an isolated Android emulator using compatible platform/test keys. It
refuses physical-device serials and may build through `atest`. These Android
tests have not been run as part of the host-only checks. GNSS callback tests do
not simulate the closed modem's SUPL/LPP implementation or confirm location
delivery. Inspect test skips and installed target versions; a generic emulator
without the FP6 carrier assets cannot qualify their packaging.

`tests/device-check` reads package/service presence, enforcing state and ADB-auth
configuration for one explicit serial and `--iwlan qti` or `--iwlan aosp`.
Presence is a prerequisite, not functional
acceptance. Native process recovery, permissions, actual modem/IMS interoperability,
call audio, subscription switching, roaming and network loss need device evidence.

AML message encoding, country profiles, HTTPS/SMS delivery and the AML lab moved to
[platform_packages_apps_EmergencyLocation](https://github.com/DiamaneOS/platform_packages_apps_EmergencyLocation).
They remain deferred and are not part of this repository's active scope.

## DCM lifecycle diagnostics

The existing Binder dump transaction exposes a fixed snapshot of lifecycle
counters to UID 0 only. It accepts no arguments and does not issue modem commands.
It reports current session/group counts, counts by configured slot, validated
request count and last message ID, malformed-frame count, broker/network reports,
and client/modem losses. It contains no APN, IP address, network handle, payload,
peer port or subscriber identifier. Counters saturate rather than wrap and reset
with the daemon process. The snapshot lock is released before writing to the
caller-provided output descriptor.

Use this only in an authorized privileged diagnostic session; do not grant shell
or application domains additional Binder access merely to read it. A request
count that stops advancing does not distinguish an absent modem request from a
packet lost before the daemon. Successful reports still require independent IMS
registration and call evidence. Host tests exercise the diagnostic snapshot
through real state transitions; native dump access and runtime behavior require
separate platform/device checks.
