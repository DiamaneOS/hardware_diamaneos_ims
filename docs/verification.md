# Verification and limits

The ordinary Qualcomm IMS service owns signalling, voice, Wi-Fi calling and
carrier emergency location. This component provides restricted data
connections and truthful Wi-Fi observations:

- Its default profile0 reports connectivity and Android validation.
- Nondefault measurement requests get `QUALITY_NOT_MET/CQ_FAIL_INCONCLUSIVE`
  when measurements are unavailable. It never claims measured healthy quality
  or successful keepalive offload.

## Host checks

`tests/run-host-tests.sh` covers the DCM codec and connection state machine,
emergency capacity and selection, unavailable Wi-Fi profile handling,
transaction/peer fences, bounded retries, stale callback rejection, address
eligibility and broker network replacement. Also:

- emergency cases: dual-SIM and no-SIM requests, IMS and emergency side by side,
  broker and modem restarts, the session reserve, and a randomised model check
  of the emergency lifecycle
- the real broker classes against host-only Android fixtures, for its emergency
  request rules
- a policy check pinning the build, SELinux and init settings that keep
  emergency service on in user builds

Its report gives the case counts. The tests place no calls, send no SMS and
contact no endpoints.

## Builds and diagnostics

- Every build type uses the same notification and profile handling. No
  private compiler flag, debug property, histogram or trace enables
  functionality.
- On user builds the observer does not poll diagnostic status.
- Errors exclude network/subscriber identifiers, addresses, opaque measurement
  IDs, payloads and exception messages.
- The paired WLAN Binder interface exposes only current core progress and
  optional channel errors, to the registered, policy-confined observer. Update
  both endpoints together.
- The fixed root-only DCM counter dump is a read-only operational snapshot, not
  an IMS registration or carrier-delivery test. It holds no state lock while
  writing. Do not widen policy for diagnostic access.

## Image checks

- Check the actual Soong linking, JNI dependencies, VINTF,
  UID/signing/permission boundaries, enforcing SELinux and native syscall
  filters. `tests/device-check` is a read-only inventory, not functional
  acceptance.
- Carrier checks on the exact image: both SIMs, incoming/outgoing VoLTE and
  VoWiFi, SMS, data, ordinary data off, a non-default data SIM, locked/idle
  boot, network loss/reconnection, VPN lockdown, handovers and bounded service
  recovery.
- Measure registration recovery separately from command acknowledgement.
  Eventual recovery alone does not meet a declared recovery deadline.
- Simulations qualify only implemented emergency state, capacity, SIM
  selection, framework routing and callback boundaries. They cannot prove real
  emergency fallback, responder receipt or closed-modem SUPL/LPP behavior. No
  real emergency traffic is part of the ordinary test plan.
- AML is not part of DiamaneOS or this repository. eSIM management is a
  separate component.
