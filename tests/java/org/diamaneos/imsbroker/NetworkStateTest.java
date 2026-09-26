/* SPDX-License-Identifier: Apache-2.0
 * Copyright 2026 The DiamaneOS Project
 */
package org.diamaneos.imsbroker;

public final class NetworkStateTest {
    public static void main(String[] args) {
        NetworkState<Integer, String, String> s = new NetworkState<>();
        check(!s.capabilities(1, "old"));
        s.available(1);
        check(s.settling());
        check(s.capabilities(1, "IMS:sub1"));
        check(s.settling());
        check(s.links(1, "v4"));
        check(s.settling());
        check(!s.usable());
        check(s.blocked(1, false));
        check(s.usable());
        check(!s.settling());
        s.available(2);
        check(s.settling());
        check(!s.lost(1));
        check(!s.links(1, "stale"));
        check(s.network() == 2);
        check(s.capabilities() == null);
        check(s.links() == null);
        check(s.links(2, "v6"));
        check(s.settling());
        check(s.capabilities(2, "IMS:sub1"));
        check(s.settling());
        check(!s.blocked(1, false));
        check(s.blocked(2, true));
        check(!s.usable());
        check(s.blocked(2, false));
        check(s.usable());
        check(!s.settling());
        check(s.lost(2));
        check(s.network() == null);
        s.available(3);
        s.close();
        s.available(4);
        check(!s.capabilities(4, "stale"));
        check(s.network() == null);
        check(!s.lost(3));
        System.out.println("Broker network-switch, stale-event and close simulations passed");
    }

    static void check(boolean value) {
        if (!value) throw new AssertionError();
    }
}
