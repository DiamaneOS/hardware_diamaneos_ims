/* SPDX-License-Identifier: Apache-2.0
 * Copyright 2026 The DiamaneOS Project
 */
package org.diamaneos.emergencylocation;

import java.nio.charset.StandardCharsets;
import java.util.Locale;
import java.util.Set;

public final class SimulationTest {
    static int checks;

    static void check(boolean b) {
        checks++;
        if (!b) throw new AssertionError("check " + checks);
    }

    static void rejects(Runnable r) {
        checks++;
        try {
            r.run();
            throw new AssertionError("expected rejection");
        } catch (IllegalArgumentException | IllegalStateException expected) {
        }
    }

    public static void main(String[] args) {
        check(TrustedEvent.accepts(1000, TrustedEvent.ACTION, "112"));
        check(!TrustedEvent.accepts(10001, TrustedEvent.ACTION, "112"));
        check(!TrustedEvent.accepts(1000, "fake.action", "112"));
        check(!TrustedEvent.accepts(1000, TrustedEvent.ACTION, "112&evil"));
        long utc = 1800000000000L;
        AmlMessage.Identity id =
                new AmlMessage.Identity("262", "01", "123456789012345", "262", "02", "112");
        AmlMessage.Fix f = new AmlMessage.Fix(48.1, 7.2, 7.1, 1000, utc, "gps");
        AmlSession session = new AmlSession(0, 30000, 10000);
        check(session.offer(f, 1000));
        rejects(() -> session.finish(1001));
        check(session.finish(30000) == null);
        rejects(() -> session.finish(30001));
        AmlSession fresh = new AmlSession(1000, 30000, 10000);
        check(!fresh.offer(new AmlMessage.Fix(48, 7, 5, 999, utc, "gps"), 1001));
        check(!fresh.offer(new AmlMessage.Fix(48, 7, 5, 40000, utc, "gps"), 2000));
        AmlMessage.Fix recent = new AmlMessage.Fix(48, 7, 4, 29000, utc, "gps");
        check(fresh.offer(recent, 29000));
        check(fresh.finish(31000) == recent);
        check(java.util.Arrays.equals(SmsPayload.pack("AB", false), new byte[] {0x41, 0x21}));
        check(java.util.Arrays.equals(SmsPayload.pack("AB", true), new byte[] {(byte) 0x83, 0x08}));
        rejects(() -> SmsPayload.pack("A".repeat(153), false));
        rejects(() -> SmsPayload.pack("\u00e9", false));
        Locale.setDefault(Locale.GERMANY);
        String sms = AmlMessage.sms(id, f, utc, false);
        check(sms.startsWith("A\"ML=1;lt=+48.10000;lg=+7.20000;"));
        check(sms.contains(";rd=8;"));
        check(sms.endsWith(";ml=" + sms.length()));
        check(sms.length() <= 160);
        check(sms.contains("ei=123456780000000"));
        check(AmlMessage.sms(id, f, utc, true).contains("ei=123456789012345"));
        String unknown = AmlMessage.sms(id, null, utc, false);
        check(unknown.contains("lt=+00.00000;lg=+000.00000;rd=N;"));
        check(unknown.contains(";lc=0;pm=N;"));
        String https = new String(AmlMessage.https(id, f, utc, utc), StandardCharsets.UTF_8);
        check(https.contains("location_confidence=0.68"));
        check(https.contains("location_latitude=%2B48.10000"));
        check(!https.contains("device_iccid") && !https.contains("device_number"));
        AmlProfile p =
                new AmlProfile(
                        "de",
                        Set.of("112"),
                        "https://receiver.invalid/aml",
                        "",
                        "synthetic test contract",
                        utc + 100000,
                        30000,
                        10000,
                        -1,
                        false);
        check(p.matches("de", "112", utc));
        check(!p.matches("fr", "112", utc));
        check(!p.matches("de", "110", utc));
        check(!p.matches("de", "112", utc + 100001));
        rejects(
                () ->
                        new AmlProfile(
                                "de",
                                Set.of("112"),
                                "http://receiver.invalid/aml",
                                "",
                                "test",
                                utc,
                                30000,
                                10000,
                                -1,
                                false));
        rejects(
                () ->
                        new AmlProfile(
                                "de",
                                Set.of("112"),
                                "https://u:p@receiver.invalid/aml",
                                "",
                                "test",
                                utc,
                                30000,
                                10000,
                                -1,
                                false));
        rejects(() -> new AmlMessage.Fix(Double.NaN, 0, 1, 0, utc, "gps"));
        rejects(() -> new AmlMessage.Fix(91, 0, 1, 0, utc, "gps"));
        rejects(() -> new AmlMessage.Identity("262", "01", "not-an-id", "262", "02", "112"));
        AmlSession cancelled = new AmlSession(0, 1000, 1000);
        cancelled.cancel();
        check(!cancelled.offer(f, 1000));
        check(TrustedEvent.accepts(1000, TrustedEvent.SMS_ACTION, "112"));
        check(!TrustedEvent.accepts(10001, TrustedEvent.SMS_ACTION, "112"));
        check(TrustedEvent.fresh(100, 200));
        check(!TrustedEvent.fresh(-1, 200));
        check(!TrustedEvent.fresh(201, 200));
        check(!TrustedEvent.fresh(100, 10101));
        AmlMessage.Identity complete =
                new AmlMessage.Identity(
                        "262",
                        "01",
                        "262011234567890",
                        "123456789012345",
                        "262",
                        "02",
                        "112",
                        false);
        String full =
                new String(
                        AmlMessage.https(complete, f, utc, utc, "SMS", true),
                        StandardCharsets.UTF_8);
        check(full.contains("device_imsi=262011234567890") && full.contains("source=SMS"));
        String partial =
                new String(
                        AmlMessage.https(complete, f, utc, utc, "CALL", false),
                        StandardCharsets.UTF_8);
        check(partial.contains("device_imsi=262010000000000"));
        check(!partial.contains("262011234567890"));
        AmlMessage.Identity noSim =
                new AmlMessage.Identity("", "", "", "123456789012345", "", "", "112", true);
        String sparse =
                new String(
                        AmlMessage.https(noSim, f, utc, utc, "CALL", true), StandardCharsets.UTF_8);
        check(!sparse.contains("device_imsi") && !sparse.contains("cell_network_mcc"));
        check(sparse.contains("device_imei=123456789012345"));
        rejects(() -> AmlMessage.sms(noSim, f, utc, true));
        check(!p.acceptsIdentity(noSim, false));
        check(!p.acceptsIdentity(id, true)); // HTTPS full IMSI is required by default.
        check(p.acceptsIdentity(complete, true));
        AmlProfile missing =
                new AmlProfile(
                        "de",
                        Set.of("112"),
                        "https://receiver.invalid/aml",
                        "",
                        "synthetic no-SIM contract",
                        utc + 100000,
                        30000,
                        10000,
                        -1,
                        false,
                        Set.of("CALL", "SMS"),
                        false,
                        true,
                        true,
                        "full");
        check(missing.acceptsIdentity(noSim, false));
        check(missing.overlaps(p));
        check(!missing.allowRoamingSms);
        rejects(
                () ->
                        new AmlProfile(
                                "de",
                                Set.of("112"),
                                "https://receiver.invalid/aml",
                                "",
                                "test",
                                utc,
                                30000,
                                10000,
                                -1,
                                false,
                                Set.of("CALL"),
                                false,
                                true,
                                false,
                                "full"));
        rejects(
                () ->
                        new AmlMessage.Identity(
                                "262",
                                "01",
                                "999991234567890",
                                "123456789012345",
                                "262",
                                "02",
                                "112",
                                false));
        rejects(
                () ->
                        new AmlProfile(
                                "de",
                                Set.of("112"),
                                "https://receiver.invalid/aml",
                                "112",
                                "test",
                                utc,
                                30000,
                                10000,
                                1234,
                                false,
                                Set.of("SMS"),
                                false,
                                false,
                                false,
                                "full"));
        AmlProfile sendsToTrigger =
                new AmlProfile(
                        "de", Set.of("110"), "", "112", "test", utc, 30000, 10000, 1234, false);
        check(sendsToTrigger.wouldTrigger(missing));
        System.out.println(
                "AML simulations: "
                        + checks
                        + " checks passed; no sockets, SMS or telephony were used");
    }
}
