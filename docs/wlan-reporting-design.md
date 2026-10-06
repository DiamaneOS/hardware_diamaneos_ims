# Wi-Fi reporting to the modem

An opt-in source candidate for the Wi-Fi reporting part of CNE: protocol and
lifecycle cores, an Android observer and a Rust reporter.

- Native and device qualification are outstanding; this is not a
  production-ready Wi-Fi calling claim.
- The Qualcomm IWLAN frontend, radio and modem keep IMS and tunnel handling.
  The DCM daemon keeps IMS/EIMS PDN requests.
- Both endpoints and their generated interface libraries live in system_ext and
  update together as part of the OS build. Their fixed-field AIDL is a private,
  unstable interface within that partition, not a new vendor HAL.
- The reporter has its own OEM UID; it does not inherit system UID privileges.

## Process boundaries

Two identities, neither shared with the system, phone, IWLAN apps or the PDN
broker:

| Process | Responsibility | Excluded authority |
| --- | --- | --- |
| Android observer | Observe the connected Wi-Fi network and report a bounded snapshot | Modem sockets, IP traffic, route changes, network configuration, SMS, subscription credentials |
| Rust reporter | Validate snapshots and send a fixed DSD protocol subset to the configured modem | IP sockets, filesystem writes, Android Wi-Fi management, arbitrary QMI requests, DCM service publication |

Observer:

- Must not acquire `NETWORK_STACK`, `MAINLINE_NETWORK_STACK`,
  `NETWORK_SETTINGS`, `CHANGE_WIFI_STATE`, GPS location, subscriber identifiers
  or usage statistics to reproduce CNE's broad behavior.
- Uses the radio-specific `RADIO_SCAN_WITHOUT_LOCATION` permission for
  connected-network metadata with location off. Its effective permissions on
  the selected Android revision need a separate review. It can expose nearby
  network identifiers, so it is not privacy-neutral.
- Do not claim SELinux filters individual methods of the Wi-Fi Binder service.
- Passive network callbacks only: no scans, no historical scan results. Inspect
  only the connected network; never forward a scan list.
- The passive reads/listener need ACCESS_NETWORK_STATE or the Wi-Fi listener
  permission, not INTERNET. The observer declares no INTERNET permission; its
  direct socket restrictions are unchanged.
- Platform signing authorizes only declared permissions; it does not justify a
  shared UID or broad domain.

Reporter:

- A QRTR client, not another IMS DCM publisher, with a dedicated UID, enforcing
  SELinux domain and syscall filter.
- QRTR's service number is not a kernel-enforced per-service security boundary:
  the process stays trusted with modem-facing access. Its fixed message
  allowlist and peer/transaction validation are application controls, not a
  substitute.

## Observer-to-reporter contract

- The Binder API accepts typed snapshots, not arbitrary bytes, commands, modem
  service addresses, file descriptors or callback-selected destinations.
- Bound every field and the number of addresses before queuing. Coalesce
  updates to the latest complete snapshot; keep no event backlog.
- Snapshots carry a connection generation and monotonic sequence.
- Validate the configured observer identity through the combined signing,
  seapp, UID and SELinux contract. A package name or client-supplied UID is not
  authentication.
- Reject stale generations after observer reconnect, network replacement and
  modem restart. A replacement network must not inherit the previous network's
  address, validation state or identity while callbacks arrive out of order.
- Use only real, current network metadata. Reject redacted MAC placeholders,
  unspecified/multicast addresses and unusable or obsolete link addresses.
  Never turn missing metadata into a fabricated address or report an
  unvalidated network as validated.
- Verify required fields and their precise semantics against the selected
  protocol before finalizing this API.

## Modem transaction lifecycle

- Bind only to the configured modem node and a verified DSD service/version.
  Discovery is scoped to the local QRTR control endpoint.
- Reject other nodes, unmatched ports, unexpected message types, duplicate
  fields, malformed lengths, and responses outside the current transaction and
  modem generation. Unsolicited packets are no authority to choose a modem.
- Serialize requests per verified subscription context. Bounded deadlines and
  retries: never retry forever, grow an unbounded queue or advance state just
  because a datagram was sent. A positive transport result is not a modem
  success response.
- Each indication must be understood and bounded. Never answer unsupported
  requests with fabricated success.

These transitions need explicit protocol and simulation evidence:

- **Cold start and modem restart:** establish the real current state; never
  replay an old connected snapshot before confirming it still applies.
- **Wi-Fi disconnect or address loss:** withdraw availability promptly with the
  verified loss operation. Wi-Fi enabled is not Wi-Fi connected.
- **Observer death or observation expiry:** invalidate cached observations and
  attempt the verified withdrawal. Keep an explicit uncertain state if the
  modem cannot acknowledge it.
- **Rapid reconnect or BSSID change:** invalidate the old generation so a late
  acknowledgement cannot mark the replacement available.
- **SIM removal or slot reassignment:** invalidate only the affected
  subscription context; never silently route it to the other subscription.
- **Reporter restart:** re-establish and reconcile state. An init restart alone
  cannot guarantee the modem has forgotten an earlier availability
  announcement.

A heartbeat/expiry mechanism can bound stale observer state while the reporter
is running and scheduled. It does not wake a suspended device (suspend and
resume need their own device tests) and cannot guarantee withdrawal after
reporter failure or modem unresponsiveness. Do not hide this by resetting the
radio, changing persistent modem settings or claiming unconditional fail-closed
behavior.

## Privacy and feature scope

- Keep Wi-Fi identifiers, local addresses, DNS information and network handles
  in memory only for the active snapshot. Never emit them through logs, `Debug`
  formatting, dumpsys, metrics, crash attachments or public fixtures.
- Avoid needless copies; do not promise cryptographic erasure of managed or
  ordinary heap memory.
- Logs describe transitions and bounded error categories, not payloads.
- Where a verified required field reveals a Wi-Fi identifier to the modem,
  record that disclosure explicitly. Leaving out GPS access does not make BSSID
  reporting free of location information.
- Send optional fields only when they are shown to be necessary. Never invent a
  global identifier or substitute one from another network.
- Do not advertise NAT keepalive, signal measurements, scanning or other modem
  capabilities without implementing and qualifying their lifecycle.
- A successful initial registration does not prove suspend, reconnect, roaming,
  handover, dual-SIM or emergency behavior. Keep those acceptance results
  separate.

## Before selecting a reporter in a product

1. Bind the request/response fields, loss operation, subscription semantics and
   capability meanings to exact source or authenticated device evidence. A
   native structure size or an adjacent message ID is not a wire specification.
2. Test the codec and lifecycle with malformed frames, spoofed peers, late
   responses, lost acknowledgements, observer death and modem restart.
3. Build the real Binder, UID, SELinux and seccomp integration with
   neverallows; inspect effective permissions instead of trusting manifest
   declarations.
4. Test with the native Qualcomm IWLAN path selected exclusively. Keep a known
   working cellular baseline and verify cellular recovery after each test.

Selection is explicit, through the two opt-in makefiles. Do not describe the
DCM broker as implementing this path.

## Current behavior

Reporter:

- Binds the configured primary/secondary DSD contexts and clears STA status
  before announcing a snapshot.
- Reports the actual Wi-Fi switch state and bounded same-link DNS metadata, and
  requires acknowledgements.
- Reports only the stock default connectivity profile, from observed
  connection, validation and default-route state. Advertises no optional
  measurement capabilities and supplies no signal/throughput estimates.
- The tested firmware acknowledges the zero-BSSID disconnected startup form. A
  rejection on any firmware stops that session rather than skipping
  reconciliation.
- Failed sessions get bounded packet retries and limited renewals (see
  [scheduling](#scheduling-and-containment-limits)). Modem restart creates
  fresh QRTR clients that bind again. A new observation permits another bounded
  attempt sequence; it does not bypass the delay or the acknowledgement checks.

Observer:

- Handles one unambiguous connected Internet-capable Wi-Fi network. Withdraws on
  ambiguous multi-STA observations, loss, revocation or expiry. Multi-STA
  support is not claimed.
- SIM identity stays modem-owned: the observer has no phone-state permission.
  The reporter binds only the configured modem subscription contexts;
  removal/reassignment behavior is qualified on the device.

## Non-sensitive status

On debuggable builds, the observer queries `getStatus` over its authenticated,
generation-bound Binder connection and logs only changed status via Android Log.

- The reporter returns fixed numeric progress for the two modem contexts: no
  BSSID, IP address, network handle, packet bytes or SIM identity.
- Ten integers: stage, operation and core error for each context, plus
  keepalive and profile-channel errors for each context.
- Release observers skip this polling. There are no lifecycle traces,
  histograms or exported traffic counters.
- Diagnostic-query failures do not invalidate an already delivered observation,
  and an unavailable diagnostic connection cannot renew an observation lease.
- The reporter's socket seccomp filter is not widened for logging. Init routes
  its standard error to `/dev/null`, so stderr is no evidence of device
  progress.
- Status is sampled asynchronously; it does not establish IMS registration or
  call acceptance.

Stages: -1 no endpoint/observation yet, 0 bind, 1 startup clear, 2 switch,
3 STA status, 4 acknowledged unavailable, 5 acknowledged available, 6 failed,
7 default connectivity profile, 8 notification registration, 9 startup clears
acknowledged and awaiting an authenticated observation. Settled states need both
connectivity acknowledgements.

- Operation: the pending or failed message ID.
- Error: 0 no recorded failure; positive values are QMI errors; -1 timeout;
  -2 transaction exhaustion; -3 local encoding failure.
- Keepalive error -4: a valid instruction discarded at queue capacity. It stays
  visible until context replacement and does not block core connectivity.

The temporary observation-state and address-rejection instrumentation from IPv6
bring-up has been removed. Address filtering and snapshot consistency checks are
unchanged.

## Scheduling and containment limits

- The observer's ten-second heartbeat refreshes a thirty-second lease. Real
  network changes and current-reporter death trigger immediate publication.
- Expiry withdraws the last positive network without guessing a switch change.
- The reporter handles at most eight received datagrams per context per cycle,
  then polls each independent deadline machine. Its ordinary loop period is
  250 ms.
- A failed session gets at most three fresh client attempts (renewals) per
  unchanged observation, thirty seconds apart.
- These are downstream bounds on repeated work, not carrier registration timers
  or a promise of registration within that time.
- SIGTERM/SIGINT shutdown attempts withdrawal for at most 2.5 seconds, polling
  every 50 ms: one two-second protocol deadline plus scheduling margin, while
  bounding shutdown. Android init's default stop/restart uses SIGKILL and does
  not exercise this handler. Forced termination or an unavailable modem cannot
  promise withdrawal.
- No timer fabricates connectivity or bypasses protocol acknowledgements.
- Keep heartbeat and lease values coherent when changing this policy. Exact
  recovery still needs native qualification.

Startup reconciliation runs independently of observer delivery: discover and
bind, register notifications, clear station availability and default profile0,
then wait. Unknown Wi-Fi administrative state is preserved. No switch value or
positive connectivity is sent until an authenticated observation arrives. The
init restart floor and protocol renewal limits are separate; neither guarantees
carrier registration.
