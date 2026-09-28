/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright 2026 The DiamaneOS Project
 */

package de.diamaneos.imsbroker;

import static org.junit.Assert.assertArrayEquals;
import static org.junit.Assert.assertEquals;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.junit.runners.JUnit4;

import java.net.InetAddress;
import java.util.ArrayList;
import java.util.List;

/** Documentation-range addresses only (RFC 5737, RFC 3849). */
@RunWith(JUnit4.class)
public final class AddressesTest {
    private static List<InetAddress> addresses(String... literals) throws Exception {
        List<InetAddress> list = new ArrayList<>();
        for (String literal : literals) {
            list.add(InetAddress.getByName(literal));
        }
        return list;
    }

    private static byte[] bytes(String literal) throws Exception {
        return InetAddress.getByName(literal).getAddress();
    }

    @Test
    public void noAddresses() throws Exception {
        assertEquals(0, Addresses.pickIpv4(addresses()).length);
        assertEquals(0, Addresses.pickIpv6(addresses()).length);
    }

    @Test
    public void onePerFamily() throws Exception {
        List<InetAddress> list = addresses("192.0.2.10", "2001:db8::10");
        assertArrayEquals(bytes("192.0.2.10"), Addresses.pickIpv4(list));
        assertArrayEquals(bytes("2001:db8::10"), Addresses.pickIpv6(list));
    }

    @Test
    public void skipsLinkLocalAndLoopback() throws Exception {
        List<InetAddress> list = addresses("fe80::1", "169.254.1.1", "127.0.0.1", "::1");
        assertEquals(0, Addresses.pickIpv4(list).length);
        assertEquals(0, Addresses.pickIpv6(list).length);
    }

    @Test
    public void keepsLastMatchLikeStock() throws Exception {
        List<InetAddress> list = addresses(
                "2001:db8::1", "192.0.2.1", "fe80::2", "2001:db8::2", "192.0.2.2", "169.254.0.9");
        assertArrayEquals(bytes("192.0.2.2"), Addresses.pickIpv4(list));
        assertArrayEquals(bytes("2001:db8::2"), Addresses.pickIpv6(list));
    }

    @Test
    public void familyLengths() throws Exception {
        List<InetAddress> list = addresses("192.0.2.10", "2001:db8::10");
        assertEquals(4, Addresses.pickIpv4(list).length);
        assertEquals(16, Addresses.pickIpv6(list).length);
    }
}
