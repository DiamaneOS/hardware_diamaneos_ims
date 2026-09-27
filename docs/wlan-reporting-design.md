# Wi-Fi reporting to the modem

An opt-in source replacement for the Wi-Fi reporting part of CNE: protocol and
lifecycle cores, an Android observer and a Rust reporter.

- The Qualcomm IWLAN frontend, radio and modem keep IMS and tunnel handling.
  The DCM daemon keeps IMS/EIMS PDN requests.
- Both endpoints and their generated interface libraries live in system_ext and
  update together as part of the OS build. Their fixed-field AIDL is a private,
  unstable interface within that partition, not a new vendor HAL.
- The reporter has its own OEM UID and does not inherit system UID privileges.

## Process boundaries

Two identities, neither shared with the system, phone, IWLAN apps or the PDN
broker:

| Process | Responsibility | Excluded authority |
| --- | --- | --- |
| Android observer | Observe the connected Wi-Fi network and report a bounded snapshot | Modem sockets, IP traffic, route changes, network configuration, SMS, subscription credentials |
| Rust reporter | Validate snapshots and send a fixed DSD protocol subset to the configured modem | IP sockets, filesystem writes, Android Wi-Fi management, arbitrary QMI requests, DCM service publication |

The observer must not acquire `NETWORK_STACK`, `MAINLINE_NETWORK_STACK`,
`NETWORK_SETTINGS`, `CHANGE_WIFI_STATE`, GPS location, subscriber identifiers or
usage statistics to reproduce CNE's broad behavior. The radio-specific
`RADIO_SCAN_WITHOUT_LOCATION` permission is a candidate for connected-network
metadata access with location disabled; it requires a separate review of its
effective permissions on the selected Android revision. It can expose nearby
network identifiers, so it is not a privacy-neutral permission. Do not claim
SELinux filters individual methods of the Wi-Fi Binder service.

Use passive network callbacks. Do not initiate scans or obtain historical scan
results. Inspect only the connected network, never forward a scan list. If the
platform requires the Network permission for callback delivery, document and
test it independently of direct socket restrictions. Platform signing authorizes
only declared permissions; it does not justify a shared UID or broad domain.

The reporter is a QRTR client, not another IMS DCM publisher. Give it a dedicated
UID, enforcing SELinux domain and syscall filter. QRTR's service number is not a
kernel-enforced per-service security boundary: the process remains trusted with
modem-facing access. Its fixed message allowlist and peer/transaction validation
are application controls, not a substitute for this limitation.

## Observer-to-reporter contract

- The Binder API accepts typed snapshots, not arbitrary bytes, commands, modem
  service addresses, file descriptors or callback-selected destinations.
- Bound every field and the number of addresses before queuing. Coalesce
  updates to the latest complete snapshot and keep no event backlog.
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
  protocol before changing this API.

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

Required behavior for these transitions:

- **Cold start and modem restart:** establish the real current state. Never
  replay an old connected snapshot before confirming it still applies.
- **Wi-Fi disconnect or address loss:** withdraw availability promptly with the
  verified loss operation. Wi-Fi enabled is not Wi-Fi connected.
- **Observer death or observation expiry:** invalidate cached observations and
  attempt the verified withdrawal. Keep an explicit uncertain state if the
  modem cannot acknowledge it.
- **Rapid reconnect or BSSID change:** invalidate the old generation so a late
  acknowledgement cannot mark the replacement available.
- **SIM removal or slot reassignment:** invalidate only the affected
  subscription context. Never silently route it to the other subscription.
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
- Avoid needless copies, but do not promise cryptographic erasure of managed or
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
3. Build the real Binder, UID, SELinux and seccomp integration with neverallows;
   inspect effective permissions rather than relying on manifest declarations.
4. Test with the native Qualcomm IWLAN path selected exclusively. Preserve a
   known working cellular baseline and verify cellular recovery after each test.

Selection is explicit through the two opt-in makefiles. The existing DCM broker
must not be described as implementing this path. The reporter currently binds the
configured primary/secondary DSD contexts, clears STA status before announcing a
snapshot, reports the actual Wi-Fi switch state, and requires acknowledgements.
It advertises no optional capabilities or fabricated WQE quality. The tested firmware acknowledges the zero-BSSID disconnected startup form;
a rejection on any firmware stops that session rather than skipping reconciliation.

The observer handles one unambiguous connected Internet-capable Wi-Fi network.
It withdraws on ambiguous multi-STA observations, loss, revocation or expiry.
Multi-STA support is not claimed. SIM identity remains modem-owned: the observer
has no phone-state permission; the reporter binds only the configured modem
subscription contexts and qualifies removal/reassignment behavior on-device.
Each failed session has bounded packet retries and at most three fresh client
attempts per unchanged observation, with a 30-second delay. Modem restart creates
fresh QRTR clients and binds again. A new observation permits another bounded
attempt sequence; it does not bypass the delay or the acknowledgement checks.
