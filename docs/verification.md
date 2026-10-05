# Verification and release limits

Host checks exercise the DCM codec and connection state machine, emergency
capacity and selection, unavailable Wi-Fi profile handling, transaction/peer
fences, bounded retries, stale callback rejection, address eligibility and broker
network replacement. Emergency coverage includes dual-SIM and no-SIM requests,
IMS and emergency side by side, broker and modem restarts, the session reserve
and a randomised model check of the emergency lifecycle; the real broker classes
run against host-only Android fixtures to check its emergency request rules; and
a policy check pins the build, SELinux and init settings that keep emergency
service on in user builds. Run `tests/run-host-tests.sh`; use the resulting
report for case counts. These tests place no calls, send no SMS and contact no
endpoints.

The ordinary Qualcomm IMS service owns signalling, voice, Wi-Fi calling and
carrier emergency-location mechanisms. This component provides restricted data
connections and truthful Wi-Fi observations. Its default profile0 reports
connectivity and Android validation. Nondefault measurement requests receive
`QUALITY_NOT_MET/CQ_FAIL_INCONCLUSIVE` when measurements are unavailable; it never
claims measured healthy quality or successful keepalive offload.

Both user and userdebug builds use the same notification/profile handling.
No private compiler flag, debug property, histogram or trace enables functionality.
Debuggable observers retain a deduplicated numeric progress/error log only;
release observers do not poll diagnostic status. Errors exclude network/subscriber
identifiers, addresses, opaque measurement IDs, payloads and exception messages.

The paired WLAN Binder interface exposes only current core progress and optional
channel errors to the registered, policy-confined observer. Both endpoints must
be updated together. The fixed root-only DCM counter dump is an operational
read-only snapshot, not an IMS registration or carrier-delivery test. It holds no
state lock while writing; do not widen policy for diagnostic access.

For a release candidate, qualify actual Soong/linking, JNI dependencies, VINTF,
UID/signing/permission boundaries, enforcing SELinux and native syscall filters.
Use `tests/device-check` as a read-only inventory, not functional acceptance.
Run carrier checks on the exact candidate: both SIMs, incoming/outgoing VoLTE and
VoWiFi, SMS, data, ordinary data disabled, non-default data SIM, locked/idle boot,
network loss/reconnection, VPN lockdown, handovers and bounded service recovery.
Measure registration recovery separately from command acknowledgement; eventual
recovery alone does not meet a declared recovery deadline.

Simulations qualify only implemented emergency state, capacity, SIM selection,
framework routing and callback boundaries. They cannot prove real emergency
fallback, responder receipt or closed-modem SUPL/LPP behavior. No real emergency
traffic is part of the ordinary test plan. AML remains deferred outside this repo;
eSIM management is also a separate component.

Earlier development images demonstrated ordinary calling, messaging and VPN
coexistence on the tested subscriptions. A reporter-restart test also produced an
approximately 200-second registration outage before recovery. That limitation
must remain open until the final candidate meets its declared recovery criteria
(the current development qualification uses a provisional two-minute window);
removing diagnostics or passing host/build checks does not resolve it.
