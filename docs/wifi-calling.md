# Wi-Fi calling

WLAN time queries and Wi-Fi calling are separate responsibilities. The DCM daemon
answers the verified `0x32` time query with no clock-setting capability. It does
not negotiate carrier tunnels. Android telephony chooses the qualified carrier
network: `DataNetwork` exposes IMS/EIMS as `TRANSPORT_CELLULAR` even when its
underlying access is IWLAN. Do not replace that request with a plain Wi-Fi Internet
network or return a home-router address as the IMS PDN address.

Use the [DiamaneOS IWLAN fork](https://github.com/DiamaneOS/platform_packages_services_Iwlan)
at `packages/services/Iwlan`, paired with AOSP `QualifiedNetworksService` and the
platform IKE/IPsec modules. Its `diamaneos/product.mk` and `diamaneos/board.mk`
provide opt-in source package, framework binding and domain selection. The app has
its own UID; it retains platform signing for the declared IPsec permission.

This prepares an integration candidate, not proven FP6 interoperability. Confirm
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
