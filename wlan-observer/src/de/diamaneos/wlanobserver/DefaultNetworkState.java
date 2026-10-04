// SPDX-License-Identifier: Apache-2.0
package de.diamaneos.wlanobserver;

/** Handler-confined metadata only; never grants network access or changes routes. */
final class DefaultNetworkState<N> {
    private N network;
    private boolean wifi;

    void available(N next) {
        // A replacement keeps the previous answer until its transports are known.
        if (!next.equals(network)) network = next;
    }

    /**
     * A default with neither Wi-Fi nor another physical transport is a VPN whose
     * underlying network is absent or not yet known. It says nothing about Wi-Fi.
     */
    void capabilities(N current, boolean hasWifi, boolean hasOther) {
        if (current.equals(network) && (hasWifi || hasOther)) wifi = hasWifi && !hasOther;
    }

    void lost(N previous) {
        if (previous.equals(network)) {
            network = null;
            wifi = false;
        }
    }

    boolean usesWifi() { return wifi; }
}
