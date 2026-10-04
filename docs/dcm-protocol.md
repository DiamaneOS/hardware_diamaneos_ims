# DCM protocol boundary

The implementation uses Linux QRTR UAPI and QMI service framing: one type byte,
16-bit little-endian transaction, message ID and payload length, followed by
8-bit-tag / 16-bit-length TLVs. It has no QMUX header. Service is `0x302`, major
version 1, instance zero. The observed IDL revision is 1.22 with a maximum body of
1036 bytes. The codec validates datagram length, all TLV boundaries, duplicates,
required fields, counted arrays and known field widths before dispatch.

Core activation/deactivation fields are also described by
[libqmi's IMSDCM interface](https://github.com/linux-mobile-broadband/libqmi/blob/main/data/qmi-service-imsdcm.json).
The additional field layouts were independently read from the selected device's
service tables using the format documented in
[Qualcomm's QMI IDL definitions](https://github.com/qualcomm/qmi-framework/blob/main/include/qmi_idl_lib_internal.h).
No vendor executable, proprietary source or external codec implementation is
included here.

| Request | Behavior |
| --- | --- |
| `0x20` activation | Acknowledge ID/cookie/optional instance first; request IMS or EIMS; send result/address indication only from a validated live network |
| `0x21` deactivation | Scope to the requesting client and optional instance; acknowledge, revoke the context, release the last group reference, then send its terminal result without an address |
| `0x22` get IP | Observed handler sends no reply; no fabricated address |
| `0x23` link address | Validate the port/family/counted address aggregate and acknowledge receipt; never log its contents |
| `0x2e`, `0x34` state reports | Validate optional/state fields and acknowledge; do not enable unrelated RCS helpers |
| `0x32` WLAN timezone | Read-only local-calendar/UTC response with validated PDP and sequence echoes; no clock setter or location access |
| `0x33` destroy instance | Acknowledge, then release that client's matching sessions |
| Other known commands | Explicit failure, matching the observed unsupported form |
| Unknown/malformed requests | Decode error; non-request datagrams are ignored |

Activation slot comes from TLV `0x12` (one-based); `0x13` is the instance, not
the SIM slot. Unknown slot values are rejected rather than silently choosing a
SIM. IP family is 0 for IPv4 and 1 for IPv6. Address payloads are bounded to 40
ASCII bytes. Normal and emergency Android capabilities are separate. The core
never supplies a fabricated fallback IP address.

Intentional stricter behavior includes rejecting ambiguous duplicate TLVs,
invalid slots, unusable addresses, exhausted capacity and stale broker reports.
Broker/network loss is surfaced as failure and clears the held sessions. Those
choices need modem compatibility checks. The WLAN timezone reply uses eight
little-endian 16-bit values (second, minute, hour, day, month, year, Sunday-based
weekday, signed standard offset west of UTC in 15-minute units), followed by
64-bit UTC seconds. Local fields include daylight saving; the offset follows
the observed stock non-DST convention. Conversion failure is explicit failure.
This command alone does not implement or qualify Wi-Fi calling.

The configured modem node is an integration input. It is not learned from the
first packet. QRTR control packets are accepted from the local control endpoint;
ordinary client requests must come from the configured remote node. Publication
checks for an existing service first. If that lookup does not complete within two
seconds, the daemon publishes anyway: the paired kernel reserves the port and
service for this role. It keeps the conflict watch active and withdraws its own
registration on exit. A competing publisher stops the daemon.
