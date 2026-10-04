# DSD STA status: offline codec

`wlan/` is a memory-safe, socket-free codec for an observed subset of Qualcomm
DSD STA reporting. The pure codec is host-tested and is used by the device-selected reporter.
It cannot send modem requests, observe Wi-Fi, publish a service or change routing.
It does not replace the full CNE stack. Basic FP6 calling has been demonstrated
with the adapter, while reconnect and broader lifecycle qualification remain open.

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
| TLVs `0x13`, `0x14` | Optional first/second IPv4 DNS server as little-endian numeric uint32 |
| TLVs `0x15`, `0x16` | Optional first/second IPv6 DNS server as 16 octets |
| TLV `0x1f` | Optional uint32 Wi-Fi mode; observed STA value 2 |
| TLV `0x21` | Optional uint32 connection state; 0 disconnected, 1 not validated, 2 validated |
| TLV `0x24` | Optional one-byte default-route boolean from Android default-network selection |
| Default-profile status | Request `0x43`: required uint32 profile0 in TLV `0x01`, optional uint32 state in TLV `0x10`; 0 met, 1 not met |
| Response | Result-only TLV `0x02`, uint16 result and error |

The stock request has other optional fields. This codec intentionally has no API
for SSID, scan results or unknown fields. Whether this subset suffices for the
selected firmware remains a qualification question. The stock STA builder sets
IPv6 prefix 64; the codec preserves an actual supplied prefix instead of inventing
one. This variation also needs device qualification.

The observed stock update path uses `0x20` with state 0 for disconnection. Do not
infer that adjacent request `0x21` is the correct lifecycle operation. Withdrawal
in this codec is bound to a previously validated observation and retains its real
BSSID while omitting addresses. Startup uses state0 with a zero BSSID to reconcile unknown previous state.
This is a disconnected-control placeholder, never an available network identity.
The tested FP6 firmware acknowledges this form; each new firmware integration
must qualify it. The sender requires a successful response before progressing.

The response parser checks framing, kind, message and transaction, fixed result
layout, and success/error consistency. Its caller must additionally validate the
QRTR endpoint and modem generation; the parser does not authenticate a peer.
Unknown extensions are rejected until reviewed. `session.rs` adds bounded retries, acknowledgement sequencing and subscription
binding (`0x27`, required uint32 TLV1, observed primary1/secondary2). Wi-Fi switch
reporting uses `0x34`, optional boolean TLV0x13 from the stock initializer. The
runtime does not advertise unimplemented measurement capabilities. It registers
for the reviewed notification subset and explicitly completes unavailable
measurement requests as described below. It acknowledges the stock default connectivity profile
alongside STA state, including negative startup reconciliation and withdrawal.

Loss updates first report profile0 as not met, then withdraw STA availability,
then report the actual administrative switch state. The authenticated
FP6.QREL.16.111.0 runtime callback sends profile status before STA status; this
sequence prevents leaving the old positive profile during disconnection. Positive
updates report the switch when required, then the current profile, then STA,
matching the stock runtime callback on connected inputs too. An acknowledged
switch bypass still uses profile before STA. Availability settles only after
the final station acknowledgement. Each step still requires its own
matching acknowledgement; a rejected profile stops the session, and an obsolete
transaction cannot settle a replacement observation. Both subscription bindings
remain independent. Native radio/Wi-Fi recovery qualification remains necessary.

Administrative switch notifications are sent for actual adapter state changes
or fresh/uncertain context reconciliation. DNS, address, validation, route and
connection changes still report STA/profile state, without repeating an already
acknowledged switch value. Starting a switch command invalidates its cached
acknowledgement: superseding an unacknowledged command must reconcile the modem,
even if the requested value equals the earlier acknowledged value.

Sensitive observations have no `Debug` or `Display` implementation. Encoded byte
buffers still contain identifiers; callers must not log or persist them. The
observer uses the shared Android preferred-address flag rule: reject failed-DAD
or deprecated addresses, and reject tentative addresses unless also optimistic.
The codec can check address values but cannot know their real link state. No
address state is fabricated, and the other address/snapshot checks remain in force.

See [the reporting design](wlan-reporting-design.md) for process isolation,
permission review, lifecycle requirements and remaining integration gates.

## Same-link DNS metadata

The stock `libwms.so` STA builder identifies IPv4 DNS slots at native offsets
0x28/0x30 and IPv6 DNS slots at 0x35/0x46. Its diagnostic format strings explicitly
name DNS addresses 1/2 and DNS V6 addresses 1/2. The authenticated DSD IDL maps
these to request 0x20 TLVs 0x13/0x14 (uint32 little-endian numeric IPv4) and
0x15/0x16 (16 IPv6 octets). Stock reverses the IPv4 in_addr before storage;
IPv6 is copied unchanged. These are resolver addresses, not local interface
addresses. No fabricated resolver or fallback is used.

The observer supplies at most the first two distinct DNS servers per address
family from the selected Wi-Fi LinkProperties, preserving order within each
family. The Binder representation is fixed-size and validates counts; the codec
rejects unspecified, loopback, multicast, mapped-IPv6 and invalid IPv4 addresses,
slot gaps and duplicates. Link-local resolvers are valid only in the reported
Wi-Fi link context. DNS changes replace the desired snapshot and stale pending
reports are not retried; withdrawal includes no resolver metadata.

This conveys the underlying link's resolver addresses to modem firmware, as the
stock report does. It does not issue DNS queries, change Android Private DNS or
VPN settings, or prove that modem-originated DNS follows those policies. That
modem behavior remains a device qualification item. No SSID, signal measurement,
optional capability, packet contents or resolver address is added to diagnostics.
Adding this stock metadata closes an identified omission; it does not by itself
prove that DNS was the VoWiFi failure's cause.

## Default-route metadata

The authenticated stock STA builder copies its default-route boolean to native
request offset `0xe5` (presence byte `0xe4`). The IDL maps it to optional TLV
`0x24`, one byte; the stock sender labels it `is_default_route`. This is separate
from Android validation and connection-state TLV `0x21`.

The observer uses default-network callback capabilities with its existing
ACCESS_NETWORK_STATE permission. Android delivers this metadata separately from
whether the observer itself is blocked from network access. It reports true only
for a Wi-Fi default or a VPN with Wi-Fi as its sole physical transport, with the
existing single-connected-Wi-Fi restriction. Mixed transports, missing capabilities
and disconnected snapshots produce false. A VPN default without any physical
transport, whose underlying network is absent or not yet known as during lockdown
reconnects, leaves the previous answer unchanged. It does not use getActiveNetwork(),
which can hide the default from callers without INTERNET permission, or hidden
Connectivity APIs. A replacement default keeps the previous answer until its own
capabilities classify it; late capabilities/loss from the old network are ignored.
Losing the default network without a replacement produces false. This does not change routes,
bypass a VPN, or assert successful carrier authentication. Withdrawal explicitly
sets the flag false. No new permissions or identifiers are added.

The stock relay uses the default-network callback and transport capabilities to
recognise Wi-Fi underneath a VPN. The downstream rule is conservative for mixed
underlays and retains the existing single-Wi-Fi-link restriction. Stock also
reports this flag in a separate default-profile message. Whether the complete
report sequence resolves FP6 reconnects requires device qualification.

## Default connectivity profile

The authenticated FP6.QREL.16.111.0 `libwms.so` default-profile callback checks
connected state, default-route state and Android validation. It reports profile0
as met only when all three hold, otherwise not met. The constructor creates and
starts that fixed default profile with identifier0. This is a connectivity
indicator, not a bandwidth, signal, latency or carrier-specific QoS measurement.

The source reporter uses the same criterion from its authenticated snapshot.
Missing, disconnected, unvalidated or non-default observations report not met.
It never accepts a caller-selected profile, a quality estimate or arbitrary
modem request bytes. Qualified measurements for nondefault profiles remain unimplemented; their
unavailable completion is separate from this connectivity profile.

Both STA and default-profile replies must match the current transaction and
verified modem peer before the session settles. Startup clears both prior states;
withdrawal clears profile0 as well as STA availability. Loss or replacement while
a positive update is pending prevents retrying that stale update. Negative replies
and bounded retry exhaustion fail the session; unsupported firmware is not treated
as success. The reply uses the existing result-only parser and fixed bounds.

## Keepalive operation failure

The authenticated FP6.QREL.16.111.0 stock handler decodes indication0x41, then sends operation
status request0x42. A negative local operation result becomes required TLV1,
uint32 little-endian1; nonnegative results become0. No distinct unsupported enum
meaning is assumed. This adapter cannot perform either requested operation and
returns generic failure for structurally valid start and stop instructions.

Accepted indication bodies have the stock47-byte maximum, required one-byte
operation TLV1 and optional fixed-size IPv4/IPv6, port and timer fields. Unknown
extensions, duplicates, truncation or wrong framing are rejected. Address, port
and timer bytes are not interpreted, retained or logged, and no network traffic
or cancellation success is claimed.

Only a current authenticated, subscription-bound modem context may enqueue a
completion. Each context retains at most eight queued markers and one pending
fixed reply. The reply has its own matching-ACK/retry state and shares the core
session's nonwrapping transaction space. Connectivity reconciliation has priority
and does not wait for keepalive completion. Each bounded runtime cycle advances
both deadline machines and sends at most one packet from each channel, with
connectivity first. Queue overflow leaves a sticky optional-channel error (-4) until context
replacement, without retaining instruction data or blocking core connectivity. Rejection/timeout blocks this optional
reply channel until context replacement; transaction exhaustion requires a new
session. This closes a missing failure contract, not keepalive functionality or
nondefault quality-profile support. Native carrier effect remains unqualified.

## Profile notification validation

The pure `profile_notice` decoder validates the authenticated FP6.QREL.16.111.0
initialization indication `0x45` and selection indication `0x3f`. Initialization
has a required uint32 type (known types4–43) and a maximum62-byte body. Optional
threshold fields10/11,14/15,16/17 have two-byte widths; optional12 is a count byte
and at most10 SIM-identifier bytes; optional13 is an eight-byte opaque identifier.
Selection has a required uint64 mask and a maximum36-byte body; optional10 is a
count byte with at most10 uninterpreted bytes, and optional11 is an eight-byte
opaque identifier. Framing, duplicate/unknown TLVs, truncation and widths are
checked. This does not validate threshold units or subscriber identity.

Stock maps bits32–63 to types4–35, bits3–9 to types36–42, and bit16 to type43.
The structural decoder retains only type/known-mask information, never thresholds,
SIM bytes or identifiers. The separate bounded lifecycle parser owns only the
opaque key needed to match initialization and selection, as described below.
Neither an indication nor a selected bit proves measured quality, modem
acceptance or carrier preference. Runtime peer and subscription checks precede
all notification handling.

## Offline profile ownership and unavailable measurements

The pure `profiles` module retains at most40
entries per verified subscription context, keyed by type and normalized opaque
measurement ID. Absent/explicit zero IDs share a key; nonzero IDs remain private.
The limit is downstream containment, not a stock key-space limit. Exhaustion
rejects new initialization without evicting existing entries. Duplicate
initialization is ignored. Neither thresholds nor SIM bytes are retained.

Selection starts only previously initialized entries for that measurement ID.
Cleared known bits destroy those entries, including unstarted ones. There is no
invented teardown status, and a later selection requires initialization again.
Unknown bits are rejected before mutation, a conservative difference from stock's
known-bit-only dispatch. A runtime must stop/invalidate the optional reporting
channel on unsupported lifecycle input, rather than preserve a stale positive
assessment. Malformed and overflow cases also need explicit runtime handling.

A selected entry can yield a private report token. It encodes only request0x43
with QUALITY_NOT_MET1 and CQ_FAIL_INCONCLUSIVE3; it cannot encode positive quality.
Measurement ID TLV12 is included only when nonzero. Unknown band is omitted.
This declares unavailable qualified measurements, not measured link failure or
offload success. Native modem acknowledgement and calling have been observed on development
images; recovery and carrier effects remain separate qualification gates.

Tokens carry entry revision and a runtime-owned context generation. Generations
must be globally unique across both subscriptions and never reused; stop on
exhaustion. Revalidate a token before sending or retrying. Match the exact pending
transaction and current endpoint/generation before acknowledging it. Destruction,
reinitialization and context replacement invalidate old tokens; a late ACK cannot
mark the replacement reported. No identity or packet buffer may enter diagnostics.
The runtime handles transport, retry/fairness and observation loss. Qualified
positive measurement policy remains unimplemented.

Unavailable completions use a separate pending-request channel. Each context has
at most one pending reply, with three attempts and a two-second deadline. Every
transport cycle advances core, keepalive and profile deadlines, emitting at most
three packets in that priority order. All channels share the nonwrapping
transaction allocator. Default and nondefault profile replies use different
matched transactions. Selection removal cancels an obsolete retry; its late ACK
cannot complete a reinitialized entry. A negative ACK or timeout blocks optional
reporting without claiming core failure. Transaction exhaustion requests bounded
session renewal only after allocated core work drains.

## Unavailable measurement completion

The reporter subscribes to the qualified stock notification subset after binding.
Bounded per-context ownership tracks initialization and selection for known
profile types, with checked revision/context fences and nonwrapping shared
transactions. Duplicate initialization is ignored; zero/cleared selections erase
owned entries, cancel obsolete replies and quarantine stale acknowledgements.

Selected nondefault profiles receive an explicit not-met/inconclusive completion
when no qualified measurement provider exists. Unknown lifecycle shapes and
capacity failures stop that optional channel; no positive quality is fabricated.
The independent keepalive channel likewise reports generic operation failure
without retaining endpoints or generating IP traffic. Core control deadlines and
retries continue fairly when optional channels are busy or unavailable.

This closes response contracts; it does not implement proprietary quality policy,
keepalive offload, or prove every carrier's recovery behavior. Profile0 keeps its
separate truthful connectivity meaning. Protocol activation is independent of
`ro.debuggable`; diagnostic traces and histograms are not part of this path.
