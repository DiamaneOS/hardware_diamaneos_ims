# Android integration

## Steps

Work on an integration branch. Passing host tests is no reason to select this
stack in a product.

1. Put this checkout at `hardware/diamaneos/ims`. Confirm the modem IMS service
   and call-audio path are still present. Remove competing DCM publishers and
   never run this daemon alongside another `0x302` server.
2. Include `ims-board.mk` in the board configuration. Review the device's own
   QRTR domain inventory instead of copying another device's grants.
3. Set `DIAMANEOS_IMS_MODEM_NODE` from verified device topology and
   `DIAMANEOS_IMS_SLOTS` from the product, and include `ims-product.mk`. It
   emergency PDN service by default; kill switches are diagnostic controls only.
4. Build `imsdcmd`, `DiamaneOSImsBroker` and the policy with ordinary hardening
   and neverallow checks. AIDL V1 is frozen and hash-checked locally. The
   platform build must also run its native API compatibility checks.
5. Compare emergency carrier options and generate a candidate APN merge with
   `integration/merge_emergency_apns.py`. It never installs or overwrites inputs.
   Review duplicate precedence and any mixed-use row before adopting the output.

The AML app and framework event patch in
[platform_packages_apps_EmergencyLocation](https://github.com/DiamaneOS/platform_packages_apps_EmergencyLocation)
are not part of DiamaneOS and not IMS prerequisites. Wi-Fi calling uses the
separate [IWLAN integration](wifi-calling.md).

## Tests

- Full image gates: VINTF, UID collisions, ELF dependencies, enforcing SELinux,
  a native seccomp smoke test.
- Normal carrier tests must cover both SIMs, incoming/outgoing voice, mobile data
  off, a non-default data SIM, re-registration, IMS service restart and call
  audio.
- Host simulation alone cannot establish emergency radio fallback, no-SIM
  routing, PSAP callback, RTT or carrier emergency location. Record simulated
  results separately from carrier/device observations.
- AML qualification is outside this repository.
