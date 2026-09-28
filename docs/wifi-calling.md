# Wi-Fi calling

WLAN time queries and Wi-Fi calling are separate responsibilities. The DCM daemon
answers the verified `0x32` time query with no clock-setting capability. It does
not negotiate carrier tunnels. Android telephony chooses the qualified carrier
network: `DataNetwork` exposes IMS/EIMS as `TRANSPORT_CELLULAR` even when its
underlying access is IWLAN. Do not replace that request with a plain Wi-Fi Internet
network or return a home-router address as the IMS PDN address.

The FP6 integration candidate uses the stock Qualcomm IWLAN frontend and its
certificate helper with the QCRIL `IIWlan` service. They share an application UID,
not the system UID; the device policy confines their Binder and QRTR access.
Required JNI libraries and carrier data are pinned in the vendor selection.
No CNE or second DCM publisher is selected. The separate source observer and
reporter supply acknowledged Wi-Fi state, current link addresses and bounded
same-link DNS metadata to DSD; they do not negotiate IKE or modify routing.

The alternative [DiamaneOS IWLAN fork](https://github.com/DiamaneOS/platform_packages_services_Iwlan)
uses `packages/services/Iwlan`, paired with AOSP `QualifiedNetworksService` and the
platform IKE/IPsec modules. Its `diamaneos/product.mk` and `diamaneos/board.mk`
provide opt-in source package, framework binding and domain selection. It is not
selected for the FP6 candidate. The app has
its own UID; it retains platform signing for the declared IPsec permission.

The native path has demonstrated basic incoming/outgoing ordinary calling on
two FP6 subscriptions in Wi-Fi-only mode. That evidence does not establish all
carriers, roaming, suspend, reconnect or handover behavior. The alternative AOSP
path remains unqualified for the FP6 modem data path. For each product, confirm
modem AP-assisted support, required vendor interfaces, selected carrier overrides,
provisioning, ePDG identity validation, accepted cipher suites, IMS registration,
voice/SMS, suspend and WWAN/IWLAN handovers. Keep normal and emergency behavior
separate in results. VPN/lockdown routing and cellular fallback must be observed,
not inferred from a successful tunnel or a Settings switch.

Carrier configuration remains authoritative. Do not force provisioned flags,
replace emergency domain selection with a local heuristic, allow a cleartext
fallback or accept an arbitrary ePDG certificate. Optional carrier entitlement
needs its own dependency/privacy review; no proprietary provisioning app is
silently added by this repository.
