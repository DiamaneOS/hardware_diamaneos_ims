# DiamaneOS IMS data connection

Install this repository at `hardware/diamaneos/ims` in the Android source workspace.

This repository will bring up the IMS and emergency data connections that VoLTE
needs on Qualcomm devices, with DiamaneOS code in place of Qualcomm's closed
connectivity components. It is not yet part of any build or product.

What is here:

- `aidl/`: `vendor.diamaneos.hardware.imsdcm`, the VINTF-stable interface between
  the vendor service and the broker app.
- `broker/`: `org.diamaneos.imsbroker`, a small system_ext privileged app that asks
  Android for the IMS or emergency network for a SIM and reports its handle,
  addresses and MTU. It holds one privileged permission
  (`CONNECTIVITY_USE_RESTRICTED_NETWORKS`), has no internet access, runs before
  first unlock and cannot be disabled by users.
- `permissions/`: the privileged-permission allowlist and sysconfig entry.
- `sepolicy/`: vendor and system_ext policy for the service and the broker.
- `vintf/`: the manifest and compatibility-matrix fragments, the service's init
  file and its UID.

The vendor service itself is not written yet. Until it is, nothing here should
be added to a product.
