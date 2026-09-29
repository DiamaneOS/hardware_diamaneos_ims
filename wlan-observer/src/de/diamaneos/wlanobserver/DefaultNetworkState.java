// SPDX-License-Identifier: Apache-2.0
package de.diamaneos.wlanobserver;

/** Handler-confined metadata only; never grants network access or changes routes. */
final class DefaultNetworkState<N> {
    private N network;
    private boolean wifi;

    void available(N next) {
        if (!next.equals(network)) {
            network = next;
            wifi = false;
        }
    }

    void capabilities(N current, boolean wifiOnly) {
        if (current.equals(network)) wifi = wifiOnly;
    }

    void lost(N previous) {
        if (previous.equals(network)) {
            network = null;
            wifi = false;
        }
    }

    boolean usesWifi() { return wifi; }
}
