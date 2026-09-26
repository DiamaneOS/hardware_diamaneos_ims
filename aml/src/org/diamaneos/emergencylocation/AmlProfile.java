/* SPDX-License-Identifier: Apache-2.0
 * Copyright 2026 The DiamaneOS Project
 */
package org.diamaneos.emergencylocation;

import java.net.URI;
import java.util.Set;

/** Country routes come only from a read-only, reviewed product resource. */
final class AmlProfile {
    final String country, evidence, sms;
    final Set<String> numbers;
    final URI https;
    final long expiresUtcMs, timeoutMs, maxAgeMs;
    final int smsPort;
    final boolean legacySmsPacking;
    final Set<String> sources;
    final boolean allowRoamingSms, allowNoSimHttps, allowMissingMetadata;
    final String httpsImsi;

    AmlProfile(
            String country,
            Set<String> numbers,
            String url,
            String sms,
            String evidence,
            long expiresUtcMs,
            long timeoutMs,
            long maxAgeMs,
            int smsPort,
            boolean legacySmsPacking) {
        this(
                country,
                numbers,
                url,
                sms,
                evidence,
                expiresUtcMs,
                timeoutMs,
                maxAgeMs,
                smsPort,
                legacySmsPacking,
                Set.of("CALL"),
                false,
                false,
                false,
                "full");
    }

    AmlProfile(
            String country,
            Set<String> numbers,
            String url,
            String sms,
            String evidence,
            long expiresUtcMs,
            long timeoutMs,
            long maxAgeMs,
            int smsPort,
            boolean legacySmsPacking,
            Set<String> sources,
            boolean allowRoamingSms,
            boolean allowNoSimHttps,
            boolean allowMissingMetadata,
            String httpsImsi) {
        if (country == null
                || !country.matches("[a-z]{2}")
                || numbers.isEmpty()
                || numbers.size() > 8
                || numbers.stream().anyMatch(n -> !n.matches("[0-9]{2,6}"))
                || evidence == null
                || evidence.isBlank()
                || evidence.length() > 160
                || expiresUtcMs <= 0) throw new IllegalArgumentException("unverified AML profile");
        URI uri = url.isEmpty() ? null : URI.create(url);
        if (uri != null
                && (!"https".equals(uri.getScheme())
                        || uri.getHost() == null
                        || uri.getUserInfo() != null
                        || uri.getFragment() != null
                        || uri.getQuery() != null
                        || (uri.getPort() != -1 && uri.getPort() != 443)))
            throw new IllegalArgumentException("invalid endpoint");
        if (!sms.isEmpty() && !sms.matches("\\+?[0-9]{3,15}"))
            throw new IllegalArgumentException("invalid SMS destination");
        if (!sms.isEmpty() && (smsPort < 0 || smsPort > 65535))
            throw new IllegalArgumentException("unverified SMS port");
        if (uri == null && sms.isEmpty()) throw new IllegalArgumentException("no transport");
        if (sources == null
                || sources.isEmpty()
                || !Set.of("CALL", "SMS").containsAll(sources)
                || !("full".equals(httpsImsi) || "partial".equals(httpsImsi))
                || (allowNoSimHttps && (uri == null || !allowMissingMetadata)))
            throw new IllegalArgumentException("invalid delivery policy");
        new AmlSession(0, timeoutMs, maxAgeMs);
        this.sources = Set.copyOf(sources);
        if (this.sources.contains("SMS") && numbers.contains(sms.replace("+", "")))
            throw new IllegalArgumentException("recursive AML SMS destination");
        this.allowRoamingSms = allowRoamingSms;
        this.allowNoSimHttps = allowNoSimHttps;
        this.allowMissingMetadata = allowMissingMetadata;
        this.httpsImsi = httpsImsi;
        this.country = country;
        this.numbers = Set.copyOf(numbers);
        this.https = uri;
        this.sms = sms;
        this.evidence = evidence;
        this.smsPort = smsPort;
        this.legacySmsPacking = legacySmsPacking;
        this.expiresUtcMs = expiresUtcMs;
        this.timeoutMs = timeoutMs;
        this.maxAgeMs = maxAgeMs;
    }

    boolean acceptsIdentity(AmlMessage.Identity id, boolean hasSim) {
        return (hasSim || allowNoSimHttps)
                && (allowMissingMetadata || id.complete())
                && (https == null
                        || !"full".equals(httpsImsi)
                        || !id.fullImsi.isEmpty()
                        || allowMissingMetadata);
    }

    boolean wouldTrigger(AmlProfile other) {
        return country.equals(other.country)
                && !sms.isEmpty()
                && other.sources.contains("SMS")
                && other.numbers.contains(sms.replace("+", ""));
    }

    boolean overlaps(AmlProfile other) {
        return country.equals(other.country)
                && numbers.stream().anyMatch(other.numbers::contains)
                && sources.stream().anyMatch(other.sources::contains);
    }

    boolean matches(String visitedCountry, String emergencyNumber, long utcMs) {
        return utcMs > 0
                && utcMs < expiresUtcMs
                && country.equals(visitedCountry)
                && numbers.contains(emergencyNumber);
    }
}
