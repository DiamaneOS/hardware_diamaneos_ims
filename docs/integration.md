# Android integration

- Project-owned app IDs and the private WLAN Binder interface use `de.diamaneos`.
- Update the observer and reporter together: their Binder service name and
  generated interface packages must match.
- A renamed package is a new Android identity; the old app's data and
  permission state do not migrate. Verify the broker's permission, system
  allowlists and SELinux app domains on the resulting image.
- Retained artifacts from earlier builds keep their original package names for
  reproducibility.

## Steps

Work on an integration branch. Passing host tests is no reason to select this
stack in a product.

1. Put this checkout at `hardware/diamaneos/ims`. Confirm the modem IMS service
   and call-audio path are still present. Remove competing DCM publishers;
   never run this daemon alongside another `0x302` server.
2. Include `ims-board.mk` in the board configuration. Review the device's own
   QRTR domain inventory instead of copying another device's grants.
3. Set `DIAMANEOS_IMS_MODEM_NODE` from verified device topology and
   `DIAMANEOS_IMS_SLOTS` from the product, and include `ims-product.mk`. It
   selects emergency PDN service by default. Nonpersistent kill switches exist
   only in debuggable builds and cannot disable a later boot or user release.
4. Build `imsdcmd`, `DiamaneOSImsBroker` and the policy with ordinary hardening
   and neverallow checks. AIDL V1 is frozen and hash-checked locally; the
   platform build must still run its native API compatibility checks.
5. Compare emergency carrier options and APNs. The FP6 candidate uses the
   authenticated stock APN table intact, including IMS/EIMS and MVNO filters.
   For other integrations, `integration/merge_emergency_apns.py` is an optional
   comparison tool; it never installs or overwrites inputs. Review duplicate
   precedence and mixed-use rows before adopting its output.

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
