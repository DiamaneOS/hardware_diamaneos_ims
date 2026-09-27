/* SPDX-License-Identifier: Apache-2.0
 * Copyright 2026 The DiamaneOS Project
 */
package org.diamaneos.emergencylocation;

import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.Map;

public final class CatalogTest {
    public static void main(String[] args) {
        List<AmlProfile> profiles = new ArrayList<>();
        long expiry = Long.MAX_VALUE;
        for (String code : Locale.getISOCountries()) {
            profiles.add(
                    AmlProfiles.parse(
                            Map.of(
                                    "country",
                                    code.toLowerCase(Locale.ROOT),
                                    "numbers",
                                    "112",
                                    "https",
                                    "https://receiver.invalid/aml",
                                    "evidence",
                                    "synthetic",
                                    "expiresUtcMs",
                                    Long.toString(expiry))));
        }
        AmlProfiles catalog = new AmlProfiles(profiles);
        if (profiles.size() <= 64) throw new AssertionError();
        for (AmlProfile profile : profiles) {
            if (catalog.select(profile.country, "112", "CALL", 1) != profile)
                throw new AssertionError();
            if (catalog.select(profile.country, "112", "SMS", 1) != null)
                throw new AssertionError();
        }
        profiles.add(profiles.get(0));
        rejects(() -> new AmlProfiles(profiles));
        rejects(() -> AmlProfiles.parse(Map.of("typo", "true")));
        List<AmlProfile> tooMany = new ArrayList<>();
        for (int i = 0; i <= AmlProfiles.MAX_PROFILES; i++) tooMany.add(profiles.get(0));
        rejects(() -> new AmlProfiles(tooMany));
        AmlSession session = new AmlSession(0, 1000, 1000);
        if (session.offer(new AmlMessage.Fix(1, 1, 1, 900, 1, "gps"), 900, true))
            throw new AssertionError();
        if (session.finish(1000) != null) throw new AssertionError();
        System.out.println(
                "Worldwide catalogue, ambiguity, size limits and mock-fix rejection passed");
    }

    private static void rejects(Runnable action) {
        try {
            action.run();
            throw new AssertionError("accepted invalid catalogue");
        } catch (IllegalArgumentException expected) {
        }
    }
}
