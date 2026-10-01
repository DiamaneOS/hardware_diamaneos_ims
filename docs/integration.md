# Android integration

Project-owned app IDs and the private WLAN Binder interface use `de.diamaneos`.
Update the observer and reporter together: their Binder service name and
generated interface packages must match. App package renames create new Android
identities rather than migrating the old apps' data or permission state. Verify
the new broker Network permission, system allowlists and SELinux app domains on
the resulting image. Retained artifacts from earlier builds keep their original
package names for reproducibility.

Work on an integration branch. Do not select this stack in a product merely
because its host tests pass.

1. Put this checkout at `hardware/diamaneos/ims`. Confirm the existing modem IMS
   service and call-audio path remain present. Remove competing DCM publishers
   from the candidate; never run this daemon alongside another `0x302` server.
2. Include `ims-board.mk` in the board configuration. Review the device-specific
   QRTR domain inventory instead of copying another device's grants wholesale.
3. Set `DIAMANEOS_IMS_MODEM_NODE` from verified device topology and
   `DIAMANEOS_IMS_SLOTS` from the product. Include `ims-product.mk`. It selects
   emergency PDN service by default; kill switches are diagnostic controls only.
4. Build `imsdcmd`, `DiamaneOSImsBroker` and the policy with ordinary hardening and
   neverallow checks enabled. AIDL V1 is frozen and hash-checked locally; the
   platform build must still run its native API compatibility checks.
5. Compare emergency carrier options and APNs. The FP6 candidate uses the
   authenticated stock APN table intact, including IMS/EIMS and MVNO filters.
   `integration/merge_emergency_apns.py` remains an optional comparison tool for
   other integrations; review duplicate precedence and mixed-use rows before
   adopting its output. It never installs or overwrites inputs.

The AML app and framework event patch in
[platform_packages_apps_EmergencyLocation](https://github.com/DiamaneOS/platform_packages_apps_EmergencyLocation)
are not part of DiamaneOS. Do not include them as prerequisites of IMS. Wi-Fi calling uses the separate
[IWLAN integration](wifi-calling.md).

Full image gates include VINTF, UID collision checks, ELF dependencies, enforcing
SELinux, and a native seccomp smoke test. Normal carrier tests must cover both
SIMs, incoming/outgoing voice, mobile data disabled, non-default data SIM,
re-registration, IMS service restart, and call audio. Emergency radio fallback,
no-SIM routing, PSAP callback, RTT and carrier emergency-location behavior cannot be established
by host simulation alone. Record simulated results separately from
carrier/device observations. AML qualification is outside this repository.
