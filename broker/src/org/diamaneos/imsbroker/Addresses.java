/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright 2026 The DiamaneOS Project
 */

package org.diamaneos.imsbroker;

import java.net.Inet4Address;
import java.net.Inet6Address;
import java.net.InetAddress;
import java.util.List;

/**
 * Picks the one address per IP family that the broker reports to the daemon.
 *
 * <p>The rule is stock CneApp's (RatInfo.setLinkProperties, classes.dex code at
 * 0x014dd8): walk LinkProperties.getLinkAddresses() in order, skip
 * link-local and loopback addresses, and keep the last remaining address of each
 * family. A cellular IMS or emergency network normally carries at most one
 * address per family, so "last" and "first" agree there.
 *
 * <p>Plain java.net only, so it is unit-tested on the host.
 */
final class Addresses {
    private static final byte[] NONE = new byte[0];

    private Addresses() {}

    /** The IPv4 address to report, in network byte order, or an empty array. */
    static byte[] pickIpv4(List<InetAddress> addresses) {
        return pick(addresses, Inet4Address.class);
    }

    /** The IPv6 address to report, in network byte order, or an empty array. */
    static byte[] pickIpv6(List<InetAddress> addresses) {
        return pick(addresses, Inet6Address.class);
    }

    private static byte[] pick(List<InetAddress> addresses, Class<? extends InetAddress> family) {
        InetAddress picked = null;
        for (InetAddress address : addresses) {
            if (family.isInstance(address)
                    && !address.isLinkLocalAddress()
                    && !address.isLoopbackAddress()) {
                picked = address;
            }
        }
        return picked == null ? NONE : picked.getAddress();
    }
}
