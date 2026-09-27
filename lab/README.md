# Emergency connectivity lab

These tools are runnable locally now. They do not place calls, send real SMS,
contact emergency recipients, or establish worldwide service coverage. Production
AML remains disabled until verified regional profiles are supplied and native
integration is qualified. Country configuration and laboratory simulation are
separate from carrier/emergency-centre acceptance.

## Host scenarios

Requires Python 3 and JDK 17 or newer (`JAVA_HOME` may select the JDK):

```sh
./lab/run run lab/scenarios/domestic.properties
./lab/run matrix --countries de,fr,gb,us --output results.json
./lab/run matrix --countries all --output all-countries.json
python3 -m unittest discover -s lab/tests -v
./tests/run-host-tests.sh
```

`all` uses the JDK's ISO country list. Each country runs the same synthetic
scenarios: this verifies country-independent routing behavior, not a database of
real national emergency services. Reports distinguish PASS (an explicit expected
outcome matched), FAIL, and OBSERVED (no expectation supplied). Output files are
created exclusively; choose a new path rather than overwriting prior evidence.

Copy a `.properties` scenario to add a case. Supported fields:

| Field | Meaning / default |
| --- | --- |
| `name` | Report label |
| `country`, `profile_country` | Visited and configured country; `de`, same country |
| `number`, `profile_numbers` | Synthetic activation number and accepted comma-separated numbers; `112`, same number |
| `source`, `profile_sources` | CALL or SMS; CALL, CALL/SMS |
| `has_sim`, `home_country` | Simulated subscription state; true, visited country |
| `sender_uid`, `event_age_ms` | Synthetic sender and event age; 1000, 0 |
| `profile_expired`, `delivery_expired` | Expiry failures; false |
| `fix` | good, none, stale, mock, future or inaccurate |
| `https` | 200, 204, 400, 503, redirect, tls_error, offline or late_connect |
| `allow_roaming_sms`, `allow_no_sim_https`, `allow_missing_metadata` | Explicit profile flags; false |
| `https_imsi`, `sms_packing` | full/partial, gsm7-lsb/gsm7-msb-legacy |
| `expected_decision`, `expected_sms`, `expected_https`, `expected_fix` | Assertions against reported outcomes |

`other` is accepted for profile/home country as a deliberate mismatch. All phone
identifiers, coordinates and destinations are fixed synthetic fixtures. Input
cannot select a real SMS or HTTPS recipient. A future timestamp can be tested
with a negative event age. Actual receiver acceptance is always unverified.

The harness executes the production catalogue, identity policy, freshness logic,
location session, message encoders, SMS packing and HTTPS sender. It substitutes
in-memory transport and metadata. It does not execute Android's telephony/radio,
GNSS driver, system-server event dispatch or production app scheduling.

## Standalone Android APK

Build on a workstation with Android SDK platform 36/build-tools 36.0.0 and a JDK:

```sh
export ANDROID_SDK_ROOT=/path/to/android-sdk
./lab/build-apk
```

Output: `lab/out/apk/diamaneos-aml-lab.apk` and `artifact.json` with its hash.
Requires Android 15/API 35 or newer. It is a separate package using a disposable
lab signing key, never the platform or production release key. The APK requests
**zero permissions**, has no services/receivers or instrumentation target, and
cannot activate production AML. Production build files do not include lab code.

Install it yourself when ready, using an explicit device selection:

```sh
adb -s "$ANDROID_SERIAL" install -r lab/out/apk/diamaneos-aml-lab.apk
```

Open **DiamaneOS AML Lab**, choose a two-letter country code or `all`, and run the
bundled cases. **Open scenario file** uses Android's document picker for additional
`.properties` files; no rebuild is needed. The report appears on screen. The app
never reads the phone's location, SIM identity or real carrier state.

The build script creates a local test keystore under ignored `lab/out/`. Preserve
that key for updates to an installed lab APK; it is not suitable for production.
The script builds and checks the APK but never installs or launches it.

## Real TLS, entirely on loopback

```sh
./lab/tls-smoke
```

Requires OpenSSL and permission to bind a local loopback socket. This uses the
actual HTTPS sender with a temporary TLS receiver and a private test truststore.
It verifies certificate rejection, successful trusted delivery, refusal to follow
redirects and handling of HTTP errors. Only `127.0.0.1` is permitted. Certificates
and keys are temporary; no global trust store or TLS verifier is changed. This
still does not test a real emergency recipient or Android's networking stack.

## Regional profiles and native integration

```sh
./lab/run validate-profiles /path/to/aml_profiles.xml
./lab/device-check --serial "$ANDROID_SERIAL"
```

Profile validation uses the same bounded catalogue and field parser as production:
up to 1,024 profiles, indexed by country, with ambiguity/recursion rejection. XML
DTD/entities/external access are disabled. Success establishes schema validity,
not receiver authenticity or interoperability. Expired profiles return failure.
Zero profiles explicitly means no configured recipients. Populate the existing
`aml/res/xml/aml_profiles.xml` from verified receiver documentation; no Java code
change is needed for the formats and routing policies already supported.

`device-check` only reads enforcing state, ADB-authentication configuration,
package presence and DCM service presence. It neither installs nor grants anything,
changes a setting, selects a SIM, nor attempts a call. Missing prerequisites return
nonzero; passing prerequisites are not a functional telephony/AML pass.

After a separate enforcing integration build, retain evidence for:

- The framework's native event tests, receiver spoof rejection, permissions,
  package/domain separation, before-unlock behavior and process recovery.
- DCM/modem interoperability, both SIMs, VoLTE/VoWiFi registration, normal peer
  calls/SMS, audio, handovers and radio/network loss.
- Country/carrier receiver configuration, emergency fallback/callback/RTT and
  call/location correlation through an arranged lab/operator test path.
- GNSS performance and a separately reviewed network/fused provider if required;
  DNS stalls, delivery cancellation, wakelock cleanup and SMS submission results.

No generic test phone number or live AML destination is embedded. New receiver
formats, authentication mechanisms or unsupported modem interfaces may still
require implementation work; this lab cannot guarantee arbitrary worldwide
interoperability simply by changing a country code.
