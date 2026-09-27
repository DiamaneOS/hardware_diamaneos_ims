/* SPDX-License-Identifier: Apache-2.0
 * Copyright 2026 The DiamaneOS Project
 */
package org.diamaneos.emergencylocation;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;

/** Shared production/lab catalogue. No I/O, mutable routes or caller-chosen destinations. */
final class AmlProfiles {
    static final int MAX_PROFILES = 1024;
    private static final Set<String> FIELDS =
            Set.of(
                    "country",
                    "numbers",
                    "https",
                    "sms",
                    "evidence",
                    "expiresUtcMs",
                    "timeoutMs",
                    "maxFixAgeMs",
                    "smsPort",
                    "smsPacking",
                    "sources",
                    "allowRoamingSms",
                    "allowNoSimHttps",
                    "allowMissingMetadata",
                    "httpsImsi");
    private final Map<String, List<AmlProfile>> countries = new HashMap<>();

    AmlProfiles(List<AmlProfile> profiles) {
        if (profiles.size() > MAX_PROFILES)
            throw new IllegalArgumentException("too many AML profiles");
        for (AmlProfile p : profiles) {
            List<AmlProfile> existing =
                    countries.computeIfAbsent(p.country, k -> new ArrayList<>());
            for (AmlProfile q : existing) {
                if (p.overlaps(q) || p.wouldTrigger(q) || q.wouldTrigger(p))
                    throw new IllegalArgumentException("ambiguous or recursive AML routes");
            }
            existing.add(p);
        }
    }

    AmlProfile select(String country, String number, String source, long utc) {
        for (AmlProfile p : countries.getOrDefault(country, List.of())) {
            if (p.sources.contains(source) && p.matches(country, number, utc)) return p;
        }
        return null;
    }

    static boolean smsAllowed(
            AmlProfile profile, AmlMessage.Identity id, boolean hasSim, String homeCountry) {
        return hasSim
                && id.complete()
                && !profile.sms.isEmpty()
                && (profile.country.equals(homeCountry) || profile.allowRoamingSms);
    }

    static AmlProfile parse(Map<String, String> attrs) {
        if (!FIELDS.containsAll(attrs.keySet())
                || attrs.values().stream().anyMatch(v -> v == null || v.length() > 2048))
            throw new IllegalArgumentException("unknown or oversized AML profile attribute");
        String packing = attrs.getOrDefault("smsPacking", "gsm7-lsb");
        if (!Set.of("gsm7-lsb", "gsm7-msb-legacy").contains(packing))
            throw new IllegalArgumentException("unknown SMS packing");
        return new AmlProfile(
                attrs.get("country"),
                csv(attrs.getOrDefault("numbers", "")),
                attrs.getOrDefault("https", ""),
                attrs.getOrDefault("sms", ""),
                attrs.get("evidence"),
                Long.parseLong(attrs.getOrDefault("expiresUtcMs", "0")),
                Long.parseLong(attrs.getOrDefault("timeoutMs", "30000")),
                Long.parseLong(attrs.getOrDefault("maxFixAgeMs", "10000")),
                Integer.parseInt(attrs.getOrDefault("smsPort", "-1")),
                packing.equals("gsm7-msb-legacy"),
                csv(attrs.getOrDefault("sources", "CALL")),
                flag(attrs, "allowRoamingSms"),
                flag(attrs, "allowNoSimHttps"),
                flag(attrs, "allowMissingMetadata"),
                attrs.getOrDefault("httpsImsi", "full"));
    }

    private static Set<String> csv(String text) {
        Set<String> result = new HashSet<>();
        for (String part : text.split(",", -1)) {
            if (!result.add(part) || part.isEmpty())
                throw new IllegalArgumentException("invalid list");
        }
        return result;
    }

    private static boolean flag(Map<String, String> attrs, String name) {
        String value = attrs.getOrDefault(name, "false");
        if (!value.equals("true") && !value.equals("false"))
            throw new IllegalArgumentException("invalid profile flag");
        return value.equals("true");
    }
}
