/* SPDX-License-Identifier: Apache-2.0
 * Copyright 2026 The DiamaneOS Project
 */
package org.diamaneos.emergencylocation;

import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.net.URI;
import java.security.cert.Certificate;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Properties;
import java.util.Set;

import javax.net.ssl.HttpsURLConnection;
import javax.net.ssl.SSLException;

/** Lab-only driver of production policy, sessions, encoders and HTTPS transport. No live I/O. */
public final class AmlScenario {
    private static final long UTC = 1800000000000L;
    private static final Set<String> KEYS =
            Set.of(
                    "name",
                    "country",
                    "profile_country",
                    "number",
                    "profile_numbers",
                    "source",
                    "profile_sources",
                    "has_sim",
                    "home_country",
                    "sender_uid",
                    "event_age_ms",
                    "profile_expired",
                    "fix",
                    "https",
                    "allow_roaming_sms",
                    "allow_no_sim_https",
                    "allow_missing_metadata",
                    "https_imsi",
                    "sms_packing",
                    "expected_decision",
                    "expected_sms",
                    "expected_https",
                    "expected_fix",
                    "delivery_expired");

    public static Map<String, String> run(Properties input) throws Exception {
        for (String key : input.stringPropertyNames()) {
            if (!KEYS.contains(key) || input.getProperty(key).length() > 256)
                throw new IllegalArgumentException("unknown or oversized scenario field");
        }
        String country = get(input, "country", "de");
        if (!country.matches("[a-z]{2}")) throw new IllegalArgumentException("invalid country");
        String number = get(input, "number", "112"), source = get(input, "source", "CALL");
        boolean sim = flag(input, "has_sim", true);
        Map<String, String> report = new LinkedHashMap<>();
        report.put("name", get(input, "name", "custom"));
        report.put("country", country);
        report.put("evidence", "synthetic-production-logic-only");
        report.put("real_world_validated", "false");
        report.put("sms", "NOT_ATTEMPTED");
        report.put("https", "NOT_ATTEMPTED");
        report.put("fix", "NONE");
        Map<String, String> attrs = new LinkedHashMap<>();
        attrs.put("country", region(get(input, "profile_country", country), country));
        attrs.put("numbers", get(input, "profile_numbers", number));
        attrs.put("sources", get(input, "profile_sources", "CALL,SMS"));
        // Fixed synthetic destinations. A scenario cannot select a real recipient.
        attrs.put("https", "https://receiver.invalid/aml");
        attrs.put("sms", "5550100");
        attrs.put("smsPort", "1234");
        attrs.put("evidence", "synthetic-lab-profile");
        attrs.put(
                "expiresUtcMs",
                Long.toString(UTC + (flag(input, "profile_expired", false) ? -1 : 100000)));
        attrs.put("allowRoamingSms", Boolean.toString(flag(input, "allow_roaming_sms", false)));
        attrs.put("allowNoSimHttps", Boolean.toString(flag(input, "allow_no_sim_https", false)));
        attrs.put(
                "allowMissingMetadata",
                Boolean.toString(flag(input, "allow_missing_metadata", false)));
        attrs.put("httpsImsi", get(input, "https_imsi", "full"));
        attrs.put("smsPacking", get(input, "sms_packing", "gsm7-lsb"));
        AmlProfiles profiles = new AmlProfiles(List.of(AmlProfiles.parse(attrs)));
        String action =
                source.equals("CALL")
                        ? TrustedEvent.ACTION
                        : source.equals("SMS") ? TrustedEvent.SMS_ACTION : "unknown";
        String decision;
        if (!TrustedEvent.accepts(
                Integer.parseInt(get(input, "sender_uid", "1000")), action, number)) {
            decision = "UNTRUSTED_EVENT";
        } else if (!TrustedEvent.fresh(
                100000 - Long.parseLong(get(input, "event_age_ms", "0")), 100000)) {
            decision = "STALE_EVENT";
        } else {
            AmlProfile profile = profiles.select(country, number, source, UTC);
            if (profile == null) decision = "NO_PROFILE";
            else {
                // ITU test PLMN and synthetic device identity, never read from a phone.
                AmlMessage.Identity id =
                        new AmlMessage.Identity(
                                sim ? "001" : "",
                                sim ? "01" : "",
                                sim ? "001010000000001" : "",
                                "123456789012345",
                                sim ? "001" : "",
                                sim ? "01" : "",
                                number,
                                true);
                if (!profile.acceptsIdentity(id, sim)) decision = "IDENTITY_REJECTED";
                else {
                    decision = "ACCEPTED";
                    AmlSession session = new AmlSession(0, profile.timeoutMs, profile.maxAgeMs);
                    String fixMode = get(input, "fix", "good");
                    if (!Set.of("good", "stale", "none", "mock", "inaccurate", "future")
                            .contains(fixMode))
                        throw new IllegalArgumentException("invalid fix mode");
                    if (!fixMode.equals("none")) {
                        long timestamp =
                                fixMode.equals("stale")
                                        ? 1000
                                        : fixMode.equals("future") ? 31000 : 29000;
                        AmlMessage.Fix fix =
                                new AmlMessage.Fix(
                                        48.1,
                                        7.2,
                                        fixMode.equals("inaccurate") ? 5000 : 8,
                                        timestamp,
                                        UTC,
                                        "gps");
                        session.offer(fix, 29000, fixMode.equals("mock"));
                    }
                    AmlMessage.Fix fix = session.finish(30000);
                    report.put("fix", fix == null ? "NONE" : "AVAILABLE");
                    boolean expired = flag(input, "delivery_expired", false);
                    if (!expired
                            && AmlProfiles.smsAllowed(
                                    profile,
                                    id,
                                    sim,
                                    region(get(input, "home_country", country), country))) {
                        byte[] payload =
                                SmsPayload.pack(
                                        AmlMessage.sms(id, fix, UTC, true),
                                        profile.legacySmsPacking);
                        report.put("sms", "RECORDED");
                        report.put("sms_bytes", Integer.toString(payload.length));
                    }
                    byte[] body =
                            AmlMessage.https(
                                    id, fix, UTC, UTC, source, profile.httpsImsi.equals("full"));
                    String outcome = get(input, "https", "200");
                    if (!Set.of(
                                    "200",
                                    "204",
                                    "400",
                                    "503",
                                    "redirect",
                                    "tls_error",
                                    "offline",
                                    "late_connect")
                            .contains(outcome))
                        throw new IllegalArgumentException("invalid HTTPS outcome");
                    final boolean[] allowed = {!expired};
                    RecordingConnection connection = new RecordingConnection(outcome, allowed);
                    HttpsSender transport =
                            new HttpsSender(
                                    uri -> {
                                        if (!uri.equals(profile.https))
                                            throw new AssertionError("wrong recipient");
                                        if (outcome.equals("tls_error"))
                                            throw new SSLException("synthetic");
                                        if (outcome.equals("offline"))
                                            throw new IOException("synthetic");
                                        return connection;
                                    });
                    report.put(
                            "https", transport.send(profile.https, body, () -> allowed[0]).name());
                    report.put("https_bytes", Integer.toString(connection.body.size()));
                    if (connection.body.size() > 0
                            && (connection.getInstanceFollowRedirects()
                                    || connection.getConnectTimeout() != 5000
                                    || connection.getReadTimeout() != 5000))
                        throw new AssertionError("transport controls changed");
                }
            }
        }
        report.put("decision", decision);
        boolean checked = false, passed = true;
        for (String field : List.of("decision", "sms", "https", "fix")) {
            String expected = input.getProperty("expected_" + field);
            if (expected != null) {
                checked = true;
                passed &= expected.equals(report.get(field));
            }
        }
        report.put("result", !checked ? "OBSERVED" : passed ? "PASS" : "FAIL");
        return report;
    }

    private static String region(String value, String country) {
        return value.equals("other") ? (country.equals("de") ? "fr" : "de") : value;
    }

    static String get(Properties p, String key, String fallback) {
        return p.getProperty(key, fallback);
    }

    static boolean flag(Properties p, String key, boolean fallback) {
        String value = get(p, key, Boolean.toString(fallback));
        if (!value.equals("true") && !value.equals("false"))
            throw new IllegalArgumentException("invalid boolean");
        return value.equals("true");
    }

    private static final class RecordingConnection extends HttpsURLConnection {
        final ByteArrayOutputStream body = new ByteArrayOutputStream();
        final String outcome;
        final boolean[] permitted;

        RecordingConnection(String outcome, boolean[] permitted) throws Exception {
            super(URI.create("https://receiver.invalid/aml").toURL());
            this.outcome = outcome;
            this.permitted = permitted;
        }

        @Override
        public ByteArrayOutputStream getOutputStream() {
            if (outcome.equals("late_connect")) permitted[0] = false;
            return body;
        }

        @Override
        public int getResponseCode() {
            return outcome.equals("redirect") ? 307 : Integer.parseInt(outcome);
        }

        @Override
        public void disconnect() {}

        @Override
        public void connect() {
            throw new AssertionError("real connect forbidden");
        }

        @Override
        public boolean usingProxy() {
            return false;
        }

        @Override
        public String getCipherSuite() {
            return "synthetic";
        }

        @Override
        public Certificate[] getLocalCertificates() {
            return new Certificate[0];
        }

        @Override
        public Certificate[] getServerCertificates() {
            return new Certificate[0];
        }
    }
}
