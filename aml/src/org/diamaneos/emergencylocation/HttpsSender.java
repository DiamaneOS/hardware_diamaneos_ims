/* SPDX-License-Identifier: Apache-2.0
 * Copyright 2026 The DiamaneOS Project
 */
package org.diamaneos.emergencylocation;

import java.io.IOException;
import java.io.OutputStream;
import java.net.URI;
import java.util.function.BooleanSupplier;

import javax.net.ssl.HttpsURLConnection;

/** Transport receipt only. A 2xx response is never labelled PSAP delivery. */
final class HttpsSender {
    enum Result {
        ACCEPTED,
        REJECTED,
        FAILED
    }

    interface ConnectionFactory {
        HttpsURLConnection open(URI uri) throws IOException;
    }

    private final ConnectionFactory connections;

    HttpsSender() {
        this(uri -> (HttpsURLConnection) uri.toURL().openConnection());
    }

    HttpsSender(ConnectionFactory factory) {
        connections = factory;
    }

    Result send(URI endpoint, byte[] body) {
        return send(endpoint, body, () -> true);
    }

    Result send(URI endpoint, byte[] body, BooleanSupplier permitted) {
        if (endpoint == null
                || !"https".equals(endpoint.getScheme())
                || body == null
                || body.length > 2048) throw new IllegalArgumentException("invalid HTTPS request");
        HttpsURLConnection c = null;
        try {
            if (!permitted.getAsBoolean()) return Result.FAILED;
            c = connections.open(endpoint);
            c.setInstanceFollowRedirects(false);
            c.setConnectTimeout(5000);
            c.setReadTimeout(5000);
            c.setUseCaches(false);
            c.setDoOutput(true);
            c.setRequestMethod("POST");
            c.setRequestProperty("Content-Type", "application/x-www-form-urlencoded");
            c.setFixedLengthStreamingMode(body.length);
            // Platform TLS validation/hostname checking stays installed unchanged.
            try (OutputStream out = c.getOutputStream()) {
                // DNS/TLS can outlast collection. Do not write a stale payload
                // when connection setup returns after its delivery window.
                if (!permitted.getAsBoolean()) return Result.FAILED;
                out.write(body);
            }
            int code = c.getResponseCode();
            return code >= 200 && code < 300 ? Result.ACCEPTED : Result.REJECTED;
        } catch (IOException | RuntimeException e) {
            return Result.FAILED;
        } finally {
            if (c != null) c.disconnect();
        }
    }
}
