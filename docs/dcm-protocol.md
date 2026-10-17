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
| `0x21` deactivation | Scope to the requesting client and optional instance, then acknowledge, revoke the context, release the last group reference, then send its terminal result without an address |
| `0x22` get IP | Observed handler sends no reply. No fabricated address |
| `0x23` link address | Validate the port/family/counted address aggregate and acknowledge receipt. Never log its contents |
| `0x2e`, `0x34` state reports | Validate optional/state fields and acknowledge. Do not enable unrelated RCS helpers |
| `0x32` WLAN timezone | Read-only local-calendar/UTC response with validated PDP and sequence echoes. No clock setter or location access |
| `0x33` destroy instance | Acknowledge, then release that client's matching sessions |
| Other known commands | Explicit failure, matching the observed unsupported form |
| Unknown/malformed requests | Decode error. Non-request datagrams are ignored |

- Activation slot comes from TLV `0x12` (one-based). `0x13` is the instance,
  not the SIM slot. Unknown slot values are rejected, never mapped to a SIM.
- IP family: 0 for IPv4, 1 for IPv6. Address payloads are at most 40 ASCII
  bytes.
- Normal and emergency Android capabilities are separate. The core never
  supplies a fabricated fallback IP address.
- Network loss, as on stock: each affected context gets the terminal result
  and its PDP ID is freed, but the Android request stays filed so Android's
  retry can bring the bearer back. The modem's next activation for that slot
  and type uses it and is answered at once if the network is back. The
  client's own deactivation, instance destruction or loss, a broker failure
  report, broker loss and the emergency switch release it.
- Stricter than stock, on purpose: ambiguous duplicate TLVs, invalid slots,
  unusable addresses, exhausted capacity and stale broker reports are rejected.
  Broker loss is reported as failure and clears the held sessions and
  requests. Check these choices against each new modem firmware.

## WLAN timezone reply

- Eight little-endian 16-bit values (second, minute, hour, day, month, year,
  Sunday-based weekday, signed standard offset west of UTC in 15-minute units),
  then 64-bit UTC seconds.
- Local fields include daylight saving. The offset follows the observed stock
  non-DST convention. Conversion failure is explicit failure.
- This command alone does not implement or qualify Wi-Fi calling.

## Modem node and publication

- The modem node is an integration input, not learned from the first packet.
- QRTR control packets are accepted from the local control endpoint. Ordinary
  client requests must come from the configured remote node.
- Publication checks for an existing service first. If that lookup takes more
  than two seconds, the daemon publishes anyway: the paired kernel reserves the
  port and service for this role. The conflict watch stays active, and the
  daemon withdraws its registration on exit.
- Another publisher on the local node blocks publication until a later retry
  and is counted. Existing clients are still served. The daemon's own record is
  not a conflict. Records on other nodes are counted and ignored.
