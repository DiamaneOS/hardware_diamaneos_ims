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
5. For AML, apply `integration/frameworks-base-emergency-location.patch` to the
   recorded source revision (see its provenance JSON), then compile the actual
   framework. It adds protected, explicit primary-user call/SMS broadcasts with shared
   sender identity and the originating phone index. The AML app uses the default application signing key, not
   the platform key. It receives no general emergency-call observer permission.
6. Add `aml/sepolicy` to system_ext private policy. Include the IMS signing-key
   mapping or supply the same package-specific mapping independently. Populate
   a verified `aml_profiles.xml`; only then set `DIAMANEOS_AML_QUALIFIED_PROFILE`
   and include `aml/product.mk`. Confirm runtime grants, request-scoped location
   bypass, receiver authentication and certificate/hostname enforcement.
7. Compare emergency carrier options and generate a candidate APN merge with
   `integration/merge_emergency_apns.py`. It never installs or overwrites inputs.
   Review duplicate precedence and any mixed-use row before adopting the output.

The framework bridge is maintained in [DiamaneOS frameworks/base](https://github.com/DiamaneOS/platform_frameworks_base). No additional remote
repository is needed for the daemon, broker or separate AML application.
Wi-Fi calling uses the separate [IWLAN integration](wifi-calling.md).

Full image gates include VINTF, UID collision checks, ELF dependencies, enforcing
SELinux, and a native seccomp smoke test. Normal carrier tests must cover both
SIMs, incoming/outgoing voice, mobile data disabled, non-default data SIM,
re-registration, IMS service restart, and call audio. Emergency radio fallback,
no-SIM routing, PSAP callback, RTT and AML delivery require authorized integration
or lab evidence. No host simulator establishes those results.
