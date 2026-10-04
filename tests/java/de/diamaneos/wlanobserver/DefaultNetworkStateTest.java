// SPDX-License-Identifier: Apache-2.0
package de.diamaneos.wlanobserver;

public final class DefaultNetworkStateTest {
    private static void expect(boolean condition) {
        if (!condition) throw new AssertionError();
    }
    public static void main(String[] args) {
        DefaultNetworkState<String> state = new DefaultNetworkState<>();
        expect(!state.usesWifi());
        state.capabilities("old", true, false);
        expect(!state.usesWifi());
        state.available("wifi");
        expect(!state.usesWifi());
        state.capabilities("wifi", true, false);
        expect(state.usesWifi());
        // A replacement keeps the previous answer until its own transports are known.
        state.available("vpn");
        expect(state.usesWifi());
        state.capabilities("wifi", false, true);
        expect(state.usesWifi());
        // A VPN without a known underlying network does not withdraw Wi-Fi.
        state.capabilities("vpn", false, false);
        expect(state.usesWifi());
        state.lost("wifi");
        expect(state.usesWifi());
        state.capabilities("vpn", false, true);
        expect(!state.usesWifi());
        // Nor does it claim Wi-Fi after a cellular underlay.
        state.capabilities("vpn", false, false);
        expect(!state.usesWifi());
        state.capabilities("vpn", true, false);
        expect(state.usesWifi());
        state.capabilities("vpn", true, true);
        expect(!state.usesWifi());
        state.capabilities("vpn", true, false);
        state.lost("vpn");
        expect(!state.usesWifi());
        state.capabilities("vpn", true, false);
        expect(!state.usesWifi());
        state.available("wifi");
        state.capabilities("wifi", true, false);
        state.available("cellular");
        state.capabilities("cellular", false, true);
        expect(!state.usesWifi());
        System.out.println(
                "Default transport lifecycle: replacement, VPN underlay, stale callbacks and loss passed");
    }
}
