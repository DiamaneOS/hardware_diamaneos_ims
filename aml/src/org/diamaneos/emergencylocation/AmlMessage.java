/* SPDX-License-Identifier: Apache-2.0
 * Copyright 2026 The DiamaneOS Project
 */
package org.diamaneos.emergencylocation;

import java.net.URLEncoder;
import java.nio.charset.StandardCharsets;
import java.time.Instant;
import java.time.ZoneOffset;
import java.time.format.DateTimeFormatter;
import java.util.LinkedHashMap;
import java.util.Locale;
import java.util.Map;

/** ETSI TS 103 625 V1.3.1 encoders. Sensitive values deliberately have no toString(). */
final class AmlMessage {
    static final class Fix {
        final double latitude, longitude, accuracy;
        final long elapsedMs, utcMs;
        final String source;

        Fix(double lat, double lon, double accuracy, long elapsed, long utc, String source) {
            if (!Double.isFinite(lat)
                    || !Double.isFinite(lon)
                    || !Double.isFinite(accuracy)
                    || lat < -90
                    || lat > 90
                    || lon < -180
                    || lon > 180
                    || accuracy <= 0
                    || accuracy > 99999
                    || elapsed < 0
                    || utc <= 0
                    || !(source.equals("gps") || source.equals("wifi") || source.equals("cell")))
                throw new IllegalArgumentException("invalid location sample");
            latitude = lat;
            longitude = lon;
            this.accuracy = accuracy;
            elapsedMs = elapsed;
            utcMs = utc;
            this.source = source;
        }
    }

    static final class Identity {
        final String partialImsi, fullImsi, imei, networkMcc, networkMnc, number;

        Identity(
                String homeMcc,
                String homeMnc,
                String imei,
                String networkMcc,
                String networkMnc,
                String number) {
            this(homeMcc, homeMnc, "", imei, networkMcc, networkMnc, number, false);
        }

        Identity(
                String homeMcc,
                String homeMnc,
                String fullImsi,
                String imei,
                String networkMcc,
                String networkMnc,
                String number,
                boolean allowMissing) {
            if (homeMcc == null) homeMcc = "";
            if (homeMnc == null) homeMnc = "";
            if (fullImsi == null) fullImsi = "";
            if (imei == null) imei = "";
            if (networkMcc == null) networkMcc = "";
            if (networkMnc == null) networkMnc = "";
            if (!allowMissing || !homeMcc.isEmpty()) digits(homeMcc, 3, 3);
            if (!allowMissing || !homeMnc.isEmpty()) digits(homeMnc, 2, 3);
            if (homeMcc.isEmpty() != homeMnc.isEmpty()) throw new IllegalArgumentException();
            if (!allowMissing || !imei.isEmpty()) digits(imei, 15, 16);
            if (!allowMissing || !networkMcc.isEmpty()) digits(networkMcc, 3, 3);
            if (!allowMissing || !networkMnc.isEmpty()) digits(networkMnc, 2, 3);
            if (networkMcc.isEmpty() != networkMnc.isEmpty()) throw new IllegalArgumentException();
            if (!fullImsi.isEmpty()) {
                digits(fullImsi, 5, 15);
                if (homeMcc.isEmpty() || !fullImsi.startsWith(homeMcc + homeMnc))
                    throw new IllegalArgumentException("subscriber identity mismatch");
            }
            this.fullImsi = fullImsi;
            if (!number.matches("[0-9]{2,6}"))
                throw new IllegalArgumentException("invalid emergency number");
            String plmn = homeMcc + homeMnc;
            partialImsi = plmn.isEmpty() ? "" : plmn + "0".repeat(15 - plmn.length());
            this.imei = imei;
            this.networkMcc = networkMcc;
            this.networkMnc = networkMnc;
            this.number = number;
        }

        boolean complete() {
            return !partialImsi.isEmpty()
                    && !imei.isEmpty()
                    && !networkMcc.isEmpty()
                    && !networkMnc.isEmpty();
        }
    }

    private static final DateTimeFormatter TIME =
            DateTimeFormatter.ofPattern("uuuuMMddHHmmss", Locale.ROOT).withZone(ZoneOffset.UTC);

    private static void digits(String s, int min, int max) {
        if (s == null || s.length() < min || s.length() > max || !s.matches("[0-9]+"))
            throw new IllegalArgumentException("invalid identifier");
    }

    private static void putKnown(Map<String, String> fields, String name, String value) {
        if (!value.isEmpty()) fields.put(name, value);
    }

    private static String decimal(double n) {
        return String.format(Locale.ROOT, "%+.5f", n);
    }

    static String sms(Identity id, Fix fix, long failureUtcMs, boolean correlateWithHttps) {
        if (!id.complete()) throw new IllegalArgumentException("SMS requires complete metadata");
        String lat = fix == null ? "+00.00000" : decimal(fix.latitude);
        String lon = fix == null ? "+000.00000" : decimal(fix.longitude);
        String method =
                fix == null
                        ? "N"
                        : fix.source.equals("gps") ? "G" : fix.source.equals("wifi") ? "W" : "C";
        // SMS-only profiles may use the eight-digit TAC instead of the device's full IMEI.
        String imei = correlateWithHttps ? id.imei : id.imei.substring(0, 8) + "0000000";
        String body =
                "A\"ML=1;lt="
                        + lat
                        + ";lg="
                        + lon
                        + ";rd="
                        + (fix == null ? "N" : Long.toString((long) Math.ceil(fix.accuracy)))
                        + ";top="
                        + TIME.format(Instant.ofEpochMilli(fix == null ? failureUtcMs : fix.utcMs))
                        + ";lc="
                        + (fix == null ? "0" : "68")
                        + ";pm="
                        + method
                        + ";si="
                        + id.partialImsi
                        + ";ei="
                        + imei
                        + ";mcc="
                        + id.networkMcc
                        + ";mnc="
                        + id.networkMnc;
        int length = body.length() + 6;
        for (int i = 0; i < 4; i++) {
            int next = body.length() + 4 + Integer.toString(length).length();
            if (next == length) break;
            length = next;
        }
        String result = body + ";ml=" + length;
        if (result.length() != length || length > 160)
            throw new IllegalArgumentException("AML exceeds one SMS");
        return result;
    }

    static byte[] https(Identity id, Fix fix, long callUtcMs, long failureUtcMs) {
        return https(id, fix, callUtcMs, failureUtcMs, "CALL", false);
    }

    static byte[] https(
            Identity id,
            Fix fix,
            long callUtcMs,
            long failureUtcMs,
            String source,
            boolean useFullImsi) {
        if (!(source.equals("CALL") || source.equals("SMS"))) throw new IllegalArgumentException();
        Map<String, String> fields = new LinkedHashMap<>();
        fields.put("v", "1");
        fields.put("location_latitude", fix == null ? "+00.00000" : decimal(fix.latitude));
        fields.put("location_longitude", fix == null ? "+000.00000" : decimal(fix.longitude));
        fields.put("location_accuracy", fix == null ? "0" : Double.toString(fix.accuracy));
        fields.put("location_time", Long.toString(fix == null ? failureUtcMs : fix.utcMs));
        fields.put("location_confidence", fix == null ? "0" : "0.68");
        fields.put("location_source", fix == null ? "unknown" : fix.source);
        // Full IMSI is the standard HTTPS form; partial/omitted metadata requires
        // an explicit receiver profile. Never invent zeros for unknown identifiers.
        putKnown(fields, "device_imsi", useFullImsi ? id.fullImsi : id.partialImsi);
        putKnown(fields, "device_imei", id.imei);
        putKnown(fields, "cell_network_mcc", id.networkMcc);
        putKnown(fields, "cell_network_mnc", id.networkMnc);
        fields.put("time", Long.toString(callUtcMs));
        fields.put("emergency_number", id.number);
        fields.put("source", source);
        StringBuilder body = new StringBuilder();
        fields.forEach(
                (k, v) -> {
                    if (body.length() > 0) body.append('&');
                    body.append(k).append('=').append(URLEncoder.encode(v, StandardCharsets.UTF_8));
                });
        byte[] bytes = body.toString().getBytes(StandardCharsets.UTF_8);
        if (bytes.length > 2048) throw new IllegalArgumentException("AML payload too large");
        return bytes;
    }
}
