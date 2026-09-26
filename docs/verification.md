# Verification and limits

`tests/run-host-tests.sh` exercises the production Rust codec/state machine,
Java network-callback state, AML encoder/session/authorization/transport logic,
and emergency-APN merger. It uses no phone, QRTR publication, SMS or external
network. In-memory HTTPS responses include rejection, redirects and TLS/I/O
failure. Synthetic IP addresses are from documentation ranges and the fake
receiver is under `.invalid`.

Covered scenarios include dual SIM, shared IPv4/IPv6 requests, delayed and stale
callbacks, modem reset, broker loss, normal-session exhaustion with emergency
capacity left, invalid peer, missing IP family, malformed frames, duplicate TLVs,
instance destruction, timezone encoding, competing QRTR publishers, blocked networks,
network replacement and thousands of arbitrary packets.
AML tests cover freshness/deadlines, cancellation, unavailable location,
coordinate/identifier bounds, locale-independent formatting, message lengths,
GSM packing, country/number/expiry matching, sender identity and transport error
handling. The APN test preserves existing input and checks idempotence.

Additional checks run during development: Java compilation against Android 17
SDK modules and generated AIDL, resource linking, API dump equality, Linux host
Rust tests, and Android arm64 Rust metadata compilation against the platform's
actual Binder libraries. These are not full Soong/linker or SELinux passes.

Still required:

- Full framework/app/daemon build, release AIDL freeze, enforcing policy and
  native syscall-filter execution; package permissions and callback authentication.
- Carrier/modem behavior, not just DCM packet behavior. The firmware controls
  IMS signalling, emergency domain selection and radio fallback.
- AML receiver profile and actual call/location correlation. GNSS-only collection
  does not reproduce Google's fused-location quality. No-SIM/roaming profiles, emergency-SMS activation and framework broadcast
  authentication require native and receiver verification despite host coverage.
- Device crash/soak, low-memory, roaming, locked/before-unlock, dual-SIM changes,
  radio loss, absent data subscription and end-to-end emergency lab cases.

Passing these host tests never changes those rows to PASS. The implementation
must not be presented as working emergency calling or German AML on that basis.
