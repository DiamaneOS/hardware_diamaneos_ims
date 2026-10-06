# DSD STA status: offline codec

`wlan/` is a memory-safe, socket-free codec for an observed subset of Qualcomm
DSD STA reporting.

- Host-tested; used by the device-selected reporter.
- Cannot send modem requests, observe Wi-Fi, publish a service or change
  routing. Not a full CNE replacement.
- Basic FP6 calling has been shown with the adapter. Open points:
  [open qualification](#open-qualification).

The wire subset was bound independently to the authenticated FP6.QREL.16.100.0
`libqmiservices.so` IDL descriptors, the `libwms.so` STA builder and the
`libcne.so` sender. No vendor implementation is linked or copied.
[Qualcomm's QMI framework](https://github.com/qualcomm/qmi-framework/blob/main/include/qmi_idl_lib_internal.h)
describes the IDL table format.

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

- The stock request has other optional fields. The codec deliberately has no
  API for SSID, scan results or unknown fields.
- The stock STA builder sets IPv6 prefix 64; the codec keeps the actual
  supplied prefix instead of inventing one.

## Withdrawal and startup

- Stock disconnects with `0x20` and state 0. Do not infer that the adjacent
  request `0x21` is the right lifecycle operation.
- Withdrawal is bound to a previously validated observation: it keeps the real
  BSSID and omits addresses.
- Startup sends state 0 with a zero BSSID to reconcile unknown previous state:
  a disconnected-control placeholder, never an available network identity. The
  tested FP6 firmware acknowledges it.
- The sender needs a successful response before moving on.

## Responses and sessions

- The response parser checks framing, kind, message and transaction, the fixed
  result layout, and success/error consistency. It does not authenticate a
  peer: the caller must also validate the QRTR endpoint and modem generation.
- Unknown extensions are rejected until reviewed.
- `session.rs` adds bounded retries, acknowledgement sequencing and subscription
  binding (`0x27`, required uint32 TLV 1, observed primary 1 / secondary 2).
- Wi-Fi switch reporting uses `0x34`, optional boolean TLV `0x13`, from the
  stock initializer.
- The runtime advertises no unimplemented measurement capabilities, registers
  for the reviewed notification subset and explicitly completes unavailable
  measurement requests.
- It acknowledges the stock default connectivity profile alongside STA state,
  including negative startup reconciliation and withdrawal.

## Update order

- **Loss:** profile0 not met, then STA withdrawal, then the actual
  administrative switch state. The authenticated FP6.QREL.16.111.0 runtime
  callback also sends profile before STA; this avoids leaving the old positive
  profile during disconnection.
- **Positive updates:** the switch when required, then the current profile,
  then STA, as the stock callback does on connected inputs. An acknowledged
  switch bypass still sends profile before STA.
- Availability settles only after the final station acknowledgement. Each step
  needs its own matching acknowledgement; a rejected profile stops the session,
  and an obsolete transaction cannot settle a replacement observation.
- Both subscription bindings stay independent.
- Switch notifications go out for actual adapter state changes or
  fresh/uncertain context reconciliation. DNS, address, validation, route and
  connection changes report STA/profile state without repeating an
  acknowledged switch value.
- Starting a switch command invalidates its cached acknowledgement: superseding
  an unacknowledged command must reconcile the modem, even if the value equals
  the earlier acknowledged one.

## Sensitive data and addresses

- Sensitive observations have no `Debug` or `Display` implementation. Encoded
  byte buffers still contain identifiers; callers must not log or persist them.
- The observer uses the shared Android preferred-address flag rule: reject
  failed-DAD or deprecated addresses, and tentative ones unless also
  optimistic.
- The codec can check address values but not their real link state. No address
  state is fabricated; the other address/snapshot checks stay in force.

Process isolation, permission review and lifecycle requirements: see
[the reporting design](wlan-reporting-design.md).

## Same-link DNS metadata

- The stock `libwms.so` STA builder has IPv4 DNS slots at native offsets
  0x28/0x30 and IPv6 DNS slots at 0x35/0x46; its diagnostic format strings
  name DNS addresses 1/2 and DNS V6 addresses 1/2.
- The authenticated DSD IDL maps them to request 0x20 TLVs 0x13/0x14 (uint32
  little-endian numeric IPv4) and 0x15/0x16 (16 IPv6 octets). Stock reverses
  the IPv4 in_addr before storage; IPv6 is copied unchanged.
- These are resolver addresses, not local interface addresses. No fabricated
  resolver or fallback.
- The observer supplies at most the first two distinct DNS servers per family
  from the selected Wi-Fi LinkProperties, in order.
- The Binder representation is fixed-size and validates counts. The codec
  rejects unspecified, loopback, multicast, mapped-IPv6 and invalid IPv4
  addresses, slot gaps and duplicates. Link-local resolvers are valid only in
  the reported Wi-Fi link context.
- DNS changes replace the desired snapshot; stale pending reports are not
  retried. Withdrawal carries no resolver metadata.
- Like the stock report, this gives modem firmware the link's resolver
  addresses. It issues no DNS queries and changes no Private DNS or VPN
  settings. Diagnostics gain no SSID, signal measurement, optional capability,
  packet contents or resolver address.
- Adding this stock metadata closes an identified omission; it does not by
  itself prove DNS caused the VoWiFi failure.

## Default-route metadata

- The authenticated stock STA builder copies its default-route boolean to
  native request offset `0xe5` (presence byte `0xe4`). The IDL maps it to
  optional one-byte TLV `0x24`; the stock sender labels it `is_default_route`.
  It is separate from Android validation and connection-state TLV `0x21`.
- The observer uses default-network callback capabilities with its existing
  ACCESS_NETWORK_STATE permission. Android delivers them regardless of whether
  the observer itself is blocked from the network.
- **True** only for a Wi-Fi default, or a VPN with Wi-Fi as its sole physical
  transport, within the single-connected-Wi-Fi restriction.
- **False** for mixed transports, missing capabilities, disconnected snapshots,
  withdrawal, and loss of the default network without a replacement.
- **Unchanged** for a VPN default without a physical transport whose underlying
  network is absent or not yet known (as during lockdown reconnects).
- A replacement default keeps the previous answer until its own capabilities
  classify it; late capabilities/loss from the old network are ignored.
- Not used: getActiveNetwork(), which can hide the default from callers
  without INTERNET permission, and hidden Connectivity APIs.
- No route changes, VPN bypass, claim of carrier authentication, new
  permissions or identifiers.
- The stock relay recognises Wi-Fi under a VPN from the default-network
  callback and transport capabilities. The downstream rule is conservative for
  mixed underlays. Stock also reports this flag in a separate default-profile
  message.

## Default connectivity profile

- The authenticated FP6.QREL.16.111.0 `libwms.so` callback checks connected
  state, default-route state and Android validation; profile0 is met only when
  all three hold. The constructor creates and starts that fixed default profile
  with identifier 0.
- A connectivity indicator, not a bandwidth, signal, latency or carrier QoS
  measurement.
- The reporter uses the same criterion from its authenticated snapshot:
  missing, disconnected, unvalidated or non-default observations report not
  met. It never accepts a caller-selected profile, a quality estimate or
  arbitrary modem request bytes.
- Qualified measurements for nondefault profiles are not implemented; their
  unavailable completion is separate from this profile.
- STA and default-profile replies must both match the current transaction and
  verified modem peer before the session settles.
- Startup clears both prior states; withdrawal clears profile0 as well as STA.
  Loss or replacement while a positive update is pending stops it being
  retried.
- Negative replies and bounded retry exhaustion fail the session; unsupported
  firmware is not success. The reply uses the existing result-only parser and
  fixed bounds.

## Keepalive operation failure

- The authenticated FP6.QREL.16.111.0 stock handler decodes indication 0x41,
  then sends operation status request 0x42: a negative local result becomes
  required TLV 1, uint32 little-endian 1; nonnegative results become 0. No
  distinct "unsupported" meaning is assumed.
- This adapter cannot perform either operation and returns generic failure for
  structurally valid start and stop instructions.
- Accepted indication bodies: the stock 47-byte maximum, required one-byte
  operation TLV 1, optional fixed-size IPv4/IPv6, port and timer fields.
  Unknown extensions, duplicates, truncation and wrong framing are rejected.
  Address, port and timer bytes are not interpreted, kept or logged.
- Only a current authenticated, subscription-bound modem context may queue a
  completion: at most eight queued markers and one pending fixed reply per
  context. The reply has its own matching-ACK/retry state and shares the core
  session's nonwrapping transaction space.
- Connectivity reconciliation comes first and does not wait for keepalive. Each
  bounded runtime cycle advances both deadline machines and sends at most one
  packet per channel, connectivity first.
- Queue overflow leaves a sticky optional-channel error (-4) until context
  replacement, without keeping instruction data or blocking core connectivity.
  Rejection/timeout blocks this reply channel until context replacement;
  transaction exhaustion needs a new session.
- This closes a missing failure contract. No keepalive functionality,
  nondefault quality profiles, network traffic or cancellation success is
  claimed.

## Profile notification validation

The pure `profile_notice` decoder validates two authenticated FP6.QREL.16.111.0
indications:

- **Initialization `0x45`:** required uint32 type (known types 4–43), maximum
  62-byte body. Optional threshold fields 10/11, 14/15 and 16/17 are two bytes
  wide; optional 12 is a count byte plus at most 10 SIM-identifier bytes;
  optional 13 is an eight-byte opaque identifier.
- **Selection `0x3f`:** required uint64 mask, maximum 36-byte body. Optional 10
  is a count byte plus at most 10 uninterpreted bytes; optional 11 is an
  eight-byte opaque identifier.
- Framing, duplicate/unknown TLVs, truncation and widths are checked; threshold
  units and subscriber identity are not.
- Stock maps bits 32–63 to types 4–35, bits 3–9 to types 36–42, and bit 16 to
  type 43.
- The decoder keeps only type/known-mask information, never thresholds, SIM
  bytes or identifiers. The separate bounded lifecycle parser keeps only the
  opaque key that matches initialization and selection.
- Neither an indication nor a selected bit proves measured quality, modem
  acceptance or carrier preference. Runtime peer and subscription checks come
  first.

## Profile ownership and unavailable measurements

Ownership (the pure `profiles` module):

- At most 40 entries per verified subscription context, keyed by type and
  normalized opaque measurement ID. Absent and explicit zero IDs share a key;
  nonzero IDs stay private.
- The limit is downstream containment, not a stock key-space limit. When full,
  new initialization is rejected without evicting entries. Duplicate
  initialization is ignored. Thresholds and SIM bytes are not kept.
- Selection starts only entries already initialized for that measurement ID.
  Cleared known bits (or a zero selection) destroy those entries, including
  unstarted ones. There is no invented teardown status; a later selection needs
  initialization again.
- Unknown bits are rejected before any change, a conservative difference from
  stock's known-bit-only dispatch.
- On unsupported lifecycle input the runtime must stop/invalidate the optional
  channel rather than keep a stale positive assessment. Malformed and overflow
  cases also need explicit runtime handling: the reporter stops the channel on
  unknown lifecycle shapes and capacity failures.

Report tokens:

- A selected entry can yield a private report token. It encodes only request
  0x43 with QUALITY_NOT_MET 1 and CQ_FAIL_INCONCLUSIVE 3; it cannot encode
  positive quality. Measurement ID TLV 12 is included only when nonzero;
  unknown band is omitted.
- This declares unavailable qualified measurements, not measured link failure
  or offload success. Native modem acknowledgement and calling have been
  observed on development images.
- Tokens carry the entry revision and a runtime-owned context generation.
  Generations must be globally unique across both subscriptions and never
  reused; stop on exhaustion.
- Revalidate a token before sending or retrying. Match the exact pending
  transaction and current endpoint/generation before acknowledging it.
- Destruction, reinitialization and context replacement invalidate old tokens;
  a late ACK cannot mark the replacement reported or complete a reinitialized
  entry.
- No identity or packet buffer may enter diagnostics.

Reporter runtime:

- Subscribes to the qualified stock notification subset after binding, and
  tracks ownership of known profile types per context with checked
  revision/context fences.
- Selected nondefault profiles get the not-met/inconclusive completion, as no
  qualified measurement provider exists.
- Handles transport, retry/fairness and observation loss.
- Completions use a separate pending-request channel: at most one pending reply
  per context, three attempts, a two-second deadline. Selection removal cancels
  an obsolete retry.
- Every transport cycle advances core, keepalive and profile deadlines, sending
  at most three packets in that priority order, so core control continues
  fairly when optional channels are busy or unavailable. All channels share the
  nonwrapping transaction allocator; default and nondefault profile replies use
  different matched transactions.
- A negative ACK or timeout blocks optional reporting without claiming core
  failure. Transaction exhaustion requests bounded session renewal only after
  allocated core work drains.
- Protocol activation does not depend on `ro.debuggable`; diagnostic traces and
  histograms are not part of this path.
- This closes response contracts. It does not implement proprietary quality
  policy, qualified positive measurements or keepalive offload, or prove every
  carrier's recovery behavior.

## Open qualification

Each of these needs device qualification:

- whether this subset is enough for the selected firmware;
- keeping the supplied IPv6 prefix instead of stock's fixed 64;
- the zero-BSSID startup form, on each new firmware integration;
- native radio/Wi-Fi recovery, reconnect and the broader lifecycle;
- whether modem-originated DNS follows Android's Private DNS and VPN policies;
- whether the full report sequence resolves FP6 reconnects;
- the native carrier effect of keepalive failure replies, and the recovery and
  carrier effects of unavailable-measurement completions.
