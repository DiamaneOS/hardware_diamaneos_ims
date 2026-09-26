# DCM protocol boundary

## Framing

- Linux QRTR UAPI with QMI service framing and no QMUX header. A frame is one
  type byte, then 16-bit little-endian transaction, message ID and payload
  length fields, then TLVs with an 8-bit tag and 16-bit length.
- Service `0x302`, major version 1, instance zero. Observed IDL revision 1.22,
  maximum body 1036 bytes.
- Before dispatch the codec validates datagram length, all TLV boundaries,
  duplicates, required fields, counted arrays and known field widths.
- Core activation/deactivation fields are also described by
  [libqmi's IMSDCM interface](https://github.com/linux-mobile-broadband/libqmi/blob/main/data/qmi-service-imsdcm.json).
  The other field layouts were read independently from the selected device's
  service tables, using the format in
  [Qualcomm's QMI IDL definitions](https://github.com/qualcomm/qmi-framework/blob/main/include/qmi_idl_lib_internal.h).
  No vendor executable, proprietary source or external codec is included.

## Requests

| Request | Behavior |
| --- | --- |
| `0x20` activation | Acknowledge ID/cookie/optional instance first, request IMS or EIMS, and send result/address indication only from a validated live network |
| `0x21` deactivation | Scope to the requesting client and optional instance; release the last group reference |
| `0x22` get IP | Observed handler sends no reply. No fabricated address |
| `0x23` link address | Validate the port/family/counted address aggregate and acknowledge receipt. Never log its contents |
| `0x2e`, `0x34` state reports | Validate optional/state fields and acknowledge. Do not enable unrelated RCS helpers |
| `0x32` WLAN timezone | Read-only local-calendar/UTC response with validated PDP and sequence echoes. No clock setter or location access |
| `0x33` destroy instance | Acknowledge, then release that client's matching sessions |
| Other known commands | Explicit failure, matching the observed unsupported form |
| Unknown/malformed requests | Decode error. Non-request datagrams are ignored |

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
checks for an existing service first, keeps the conflict watch active, and withdraws
its own registration on exit. A competing publisher stops the daemon.
