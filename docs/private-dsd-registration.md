# Private notification-registration experiment

This isolated candidate tests whether stock's two common DSD registration fields
produce notifications on the tested FP6/carriers. It is not a production VoWiFi
fix and must not be published or retained in release sources.

The authenticated stock16.111 initializer sends request0x38 with optional byte
TLVs0x12/0x14=1 in both subscription contexts, after binding. The experiment uses
only those fields. Their event and service-readiness meaning remains unproved;
firmware may expect handling that this observation-only candidate does not supply.
IMS availability can therefore change. Retain the trial23 return images and
compare only with no call active before flashing.

The runtime requires both its explicit private Rust cfg and immutable
ro.debuggable. User builds keep the original session path. Matching registration
ACK is mandatory; negative replies and bounded retry exhaustion fail explicitly.
Current endpoint, generation, transaction and input-work bounds remain in force.
Header diagnostics record only existing bounded IDs/counts. No body, profile ID,
SIM identifier, Wi-Fi identifier or modem endpoint is logged by this addition.

Fixed profile0 reports retain their existing truthful connectivity meaning. This
adds no QoE result, RSSI threshold, measurement, scan, keepalive, capability0x53
or STA WQE-running marker. Header arrival cannot prove valid bodies, a supported
quality service or causality. Zero arrivals cannot exclude additional negotiation.

After the private comparison, either return to trial23 or prepare a fully reviewed
implementation from the established consumer contract. Remove this private cfg,
codec/session entry point, experiment tests and temporary diagnostics before
promoting a production change.
