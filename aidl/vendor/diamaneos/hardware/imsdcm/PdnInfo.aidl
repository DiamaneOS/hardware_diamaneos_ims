/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright 2026 The DiamaneOS Project
 */

package vendor.diamaneos.hardware.imsdcm;

/**
 * What the broker reports about a connected network. The addresses are the
 * ones Android's LinkProperties carry for it; they are carrier-assigned, so
 * neither side may log them.
 */
@VintfStability
parcelable PdnInfo {
    /** android.net.Network.getNetworkHandle() of the network. */
    long networkHandle;

    /**
     * IPv4 address in network byte order (4 bytes), or empty when the network
     * has none. Never null.
     */
    byte[] ipv4Address = {};

    /**
     * IPv6 address in network byte order (16 bytes), or empty when the network
     * has none. Never link-local. Never null.
     */
    byte[] ipv6Address = {};

    /** LinkProperties.getMtu(), or 0 when the network reports none. */
    int mtu;
}
