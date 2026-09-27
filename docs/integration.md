# Android integration

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
   neverallow checks enabled. Update and freeze AIDL V1 using the platform AIDL
   tasks before release. The current interface is still unfrozen.
5. Compare emergency carrier options and generate a candidate APN merge with
   `integration/merge_emergency_apns.py`. It never installs or overwrites inputs.
   Review duplicate precedence and any mixed-use row before adopting the output.

The deferred AML app and framework event patch live in
[platform_packages_apps_EmergencyLocation](https://github.com/DiamaneOS/platform_packages_apps_EmergencyLocation).
Do not include them as prerequisites of IMS. Wi-Fi calling uses the separate
[IWLAN integration](wifi-calling.md).

Full image gates include VINTF, UID collision checks, ELF dependencies, enforcing
SELinux, and a native seccomp smoke test. Normal carrier tests must cover both
SIMs, incoming/outgoing voice, mobile data disabled, non-default data SIM,
re-registration, IMS service restart, and call audio. Emergency radio fallback,
no-SIM routing, PSAP callback, RTT and carrier emergency-location behavior cannot be established
by host simulation alone. Record simulated results separately from any future
carrier/device observations. AML qualification is outside this repository.
