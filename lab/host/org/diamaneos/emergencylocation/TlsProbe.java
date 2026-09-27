/* SPDX-License-Identifier: Apache-2.0
 * Copyright 2026 The DiamaneOS Project
 */
package org.diamaneos.emergencylocation;

import java.net.URI;
import java.nio.charset.StandardCharsets;

/** Host-only transport probe; the only permitted destination is IPv4 loopback. */
public final class TlsProbe {
    public static void main(String[] args) {
        URI endpoint = URI.create(args[0]);
        if (!"https".equals(endpoint.getScheme())
                || !"127.0.0.1".equals(endpoint.getHost())
                || endpoint.getUserInfo() != null
                || endpoint.getPort() < 1024
                || endpoint.getPort() > 65535
                || !endpoint.getPath().equals("/aml"))
            throw new IllegalArgumentException("loopback test endpoint required");
        HttpsSender.Result result =
                new HttpsSender()
                        .send(
                                endpoint,
                                "v=1&source=CALL&test=synthetic".getBytes(StandardCharsets.UTF_8));
        System.out.println(result.name());
    }
}
