# DSD STA status: offline codec

`wlan/` is a memory-safe, socket-free codec for an observed subset of Qualcomm
DSD STA reporting. The pure codec is host-tested and is used by the opt-in reporter candidate.
It cannot send modem requests, observe Wi-Fi, publish a service or change routing.
It is not a functional replacement for CNE and has not been device-qualified.

The wire subset was independently bound to the authenticated FP6.QREL.16.100.0
`libqmiservices.so` IDL descriptors and the `libwms.so` STA builder plus
`libcne.so` sender. No vendor implementation is linked or copied. The IDL table
format is described by [Qualcomm's QMI framework](https://github.com/qualcomm/qmi-framework/blob/main/include/qmi_idl_lib_internal.h).

| Item | Observed contract |
| --- | --- |
| Service | DSD `0x2a`, major version 1 |
| STA status request | `0x20`; seven-byte QRTR QMI service header, no QMUX header |
| TLV `0x01` | Required six-byte BSSID |
| TLV `0x10` | Optional IPv4 address as a little-endian numeric uint32 |
| TLV `0x11` | Optional 16 IPv6 octets followed by a one-byte prefix length |
| TLV `0x1f` | Optional uint32 Wi-Fi mode; observed STA value 2 |
| TLV `0x21` | Optional uint32 connection state; 0 disconnected, 1 not validated, 2 validated |
| Response | Result-only TLV `0x02`, uint16 result and error |

The stock request has other optional fields. This codec intentionally has no API
for SSID, scan results or unknown fields. Whether this subset suffices for the
selected firmware remains a qualification question. The stock STA builder sets
IPv6 prefix 64; the codec preserves an actual supplied prefix instead of inventing
one. This variation also needs device qualification.

The observed stock update path uses `0x20` with state 0 for disconnection. Do not
infer that adjacent request `0x21` is the correct lifecycle operation. Withdrawal
in this codec is bound to a previously validated observation and retains its real
BSSID while omitting addresses. The private candidate also tests state0 with a zero BSSID to reconcile unknown
previous state. This is a disconnected-control placeholder, never an available
network identity. Its firmware acceptance remains a release gate; the sender
requires a successful response before progressing.

The response parser checks framing, kind, message and transaction, fixed result
layout, and success/error consistency. Its caller must additionally validate the
QRTR endpoint and modem generation; the parser does not authenticate a peer.
Unknown extensions are rejected until reviewed. `session.rs` adds bounded retries, acknowledgement sequencing and subscription
binding (`0x27`, required uint32 TLV1, observed primary1/secondary2). Wi-Fi switch
reporting uses `0x34`, optional boolean TLV0x13 from the stock initializer. The
runtime does not advertise unimplemented capabilities or register for optional
measurement requests.

Sensitive observations have no `Debug` or `Display` implementation. Encoded byte
buffers still contain identifiers; callers must not log or persist them. The
observer must exclude tentative/deprecated addresses using platform link flags;
the codec can check address values but cannot know their real link state.

See [the reporting design](wlan-reporting-design.md) for process isolation,
permission review, lifecycle requirements and remaining integration gates.
