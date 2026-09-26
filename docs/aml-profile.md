# AML receiver profile

The app follows the transport structures in
[ETSI TS 103 625 V1.3.1](https://www.etsi.org/deliver/etsi_ts/103600_103699/103625/01.03.01_60/ts_103625v010301p.pdf).
Receiver configuration is part of interoperability. An empty profile disables
location collection/delivery; emergency voice proceeds independently.

A profile needs verified values for:

- Visited country and supported emergency numbers; domestic/roaming applicability.
- HTTPS address and/or SMS destination, data-SMS port and GSM-7 packing variant.
- Receiver acceptance of partial IMSI; IMEI policy for SMS-only versus paired
  SMS/HTTPS correlation; missing subscriber/device metadata handling.
- Collection deadline, acceptable fix age, expiration and a review reference.
- Receipt semantics and any retry/update policy. This initial implementation
  makes one bounded attempt per selected transport.

Example **for schema illustration only**, never a production receiver:

```xml
<aml-profiles>
    <profile country="de" numbers="112"
        https="https://receiver.invalid/aml" sms="" smsPort="-1"
        smsPacking="gsm7-lsb" evidence="synthetic-example-only"
        expiresUtcMs="1900000000000" timeoutMs="30000" maxFixAgeMs="10000" />
</aml-profiles>
```

`gsm7-msb-legacy` is available only for receivers that explicitly require the
older data-SMS packing. The app does not send ordinary text SMS, override an
SMSC or select an undocumented port. Messages use synthetic fixtures in tests.

For Germany, the factory ELS configuration inspected so far did not provide a
verified static receiver profile. Public Google documentation describes
[operator-defined handset configuration](https://developers.google.com/android/els/fundamentals).
A complete verified stock runtime profile may be used as evidence for matching
stock defaults; extracting a destination string alone does not establish its
format, routing or correlation rules. No public test endpoint is assumed.

Ask the operator for the fields above, including 112 versus 110 scope, roaming,
no-SIM handling and an approved verification process when there is no test
backend. This is a technical interoperability question, not a request to invent
new emergency infrastructure. No test message should be sent to a live receiver
as an endpoint-discovery technique.

## Explicit delivery policy

Optional `sources="CALL,SMS"` enables both outgoing communication types; the
default is `CALL`. The framework sends no received-SMS event, and the receiver
accepts only fresh protected broadcasts bearing system-server's shared identity.
The event includes the actual phone index and its UTC/elapsed start times.

`httpsImsi="full"` is the default standard HTTPS representation. Use `partial`
only when the receiver contract explicitly accepts it. SMS always uses the
partial IMSI. The app only reads a full IMSI for a matching HTTPS profile that
requires it. Unknown identifiers are never replaced with plausible zeros.

`allowNoSimHttps="true"` requires HTTPS and `allowMissingMetadata="true"`.
It permits a receiver-agreed sparse payload with unavailable metadata omitted;
network country must still be available from the specific emergency phone index.
No home-country, device-locale, default-SIM or default-slot fallback is used.
SMS is not attempted without an active subscription and complete metadata.

`allowRoamingSms="true"` requires separately verified routing through the normal
SMSC; the default permits SMS only when home and visited countries agree. This
implementation does not change the SMSC. A receiver needing a visited-SMSC override
requires additional integration. HTTPS routing still follows the visited country.
Ambiguous country/number/source profiles and SMS routes that would trigger another
configured AML SMS event are rejected, preventing recursive transmission.

Collection slots and delivery queues are bounded. Duplicate active events are
coalesced. SMS submission precedes HTTPS, so it is not held behind DNS/TLS failure.
Pending deliveries expire, and wakelocks end even on transport stalls. HTTPS uses
connect/read timeouts; the platform DNS resolver does not provide a hard Java
thread deadline. Native stall tests remain required, and transport receipt is
never reported as emergency-centre delivery.
