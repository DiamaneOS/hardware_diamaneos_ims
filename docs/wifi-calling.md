# Wi-Fi calling

WLAN time queries and Wi-Fi calling are separate. The DCM daemon answers the
verified `0x32` time query with no clock-setting capability and negotiates no
carrier tunnels.

Android telephony chooses the qualified carrier network: `DataNetwork` exposes
IMS/EIMS as `TRANSPORT_CELLULAR` even over IWLAN. Do not replace that request
with a plain Wi-Fi Internet network or return a home-router address as the IMS
PDN address.

## FP6: native Qualcomm path

- The stock Qualcomm IWLAN frontend and its certificate helper, with the QCRIL
  `IIWlan` service. They share an application UID, not the system UID; device
  policy confines their Binder and QRTR access.
- Required JNI libraries and carrier data are pinned in the vendor selection.
  No CNE or second DCM publisher is selected.
- The source observer and reporter give DSD acknowledged Wi-Fi state, current
  link addresses and bounded same-link DNS metadata. They negotiate no IKE and
  change no routing.
- Shown: basic incoming/outgoing ordinary calls on two FP6 subscriptions in
  Wi-Fi-only mode. Not established: all carriers, roaming, suspend, reconnect,
  handover.

## Alternative: AOSP path

The [DiamaneOS IWLAN fork](https://github.com/DiamaneOS/platform_packages_services_Iwlan)
at `packages/services/Iwlan`, with AOSP `QualifiedNetworksService` and the
platform IKE/IPsec modules. Its `diamaneos/product.mk` and `diamaneos/board.mk`
opt in the source package, framework binding and domain selection. The app has
its own UID and keeps platform signing for the declared IPsec permission. Not
selected for the FP6, and unqualified for the FP6 modem data path.

## Per-product checks and rules

- Confirm modem AP-assisted support, required vendor interfaces, selected
  carrier overrides, provisioning, ePDG identity validation, accepted cipher
  suites, IMS registration, voice/SMS, suspend and WWAN/IWLAN handovers.
- Keep normal and emergency results separate.
- Observe VPN/lockdown routing and cellular fallback; don't infer them from a
  working tunnel or a Settings switch.
- Carrier configuration stays authoritative. Do not force provisioned flags,
  replace emergency domain selection with a local heuristic, allow a cleartext
  fallback or accept an arbitrary ePDG certificate.
- Optional carrier entitlement needs its own dependency/privacy review. This
  repository silently adds no proprietary provisioning app.
