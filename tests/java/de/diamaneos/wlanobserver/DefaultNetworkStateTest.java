// SPDX-License-Identifier: Apache-2.0
package de.diamaneos.wlanobserver;

public final class DefaultNetworkStateTest {
    private static void expect(boolean condition) {
        if (!condition) throw new AssertionError();
    }
    public static void main(String[] args) {
        DefaultNetworkState<String> state = new DefaultNetworkState<>();
        expect(!state.usesWifi());
        state.capabilities("old", true);
        expect(!state.usesWifi());
        state.available("wifi");
        expect(!state.usesWifi());
        state.capabilities("wifi", true);
        expect(state.usesWifi());
        state.available("vpn");
        expect(!state.usesWifi());
        state.capabilities("wifi", true);
        expect(!state.usesWifi());
        state.capabilities("vpn", true);
        state.lost("wifi");
        expect(state.usesWifi());
        state.capabilities("vpn", false);
        expect(!state.usesWifi());
        state.capabilities("vpn", true);
        state.lost("vpn");
        expect(!state.usesWifi());
        state.capabilities("vpn", true);
        expect(!state.usesWifi());
        System.out.println("Default transport lifecycle: replacement, stale callbacks and loss passed");
    }
}
