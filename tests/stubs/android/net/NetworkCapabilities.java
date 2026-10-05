// SPDX-License-Identifier: Apache-2.0
// Host-only fixture for the broker simulations. Never part of an Android module.
package android.net;

import java.util.HashSet;
import java.util.Set;

public final class NetworkCapabilities {
    public static final int NET_CAPABILITY_IMS = 4;
    public static final int NET_CAPABILITY_EIMS = 10;
    public static final int TRANSPORT_CELLULAR = 0;
    public static final int TRANSPORT_WIFI = 1;

    private final Set<Integer> capabilities = new HashSet<>();
    private final Set<Integer> transports = new HashSet<>();
    private NetworkSpecifier specifier;

    public NetworkCapabilities addCapability(int capability) {
        capabilities.add(capability);
        return this;
    }

    public NetworkCapabilities addTransportType(int transport) {
        transports.add(transport);
        return this;
    }

    public NetworkCapabilities setNetworkSpecifier(NetworkSpecifier specifier) {
        this.specifier = specifier;
        return this;
    }

    public boolean hasCapability(int capability) {
        return capabilities.contains(capability);
    }

    public boolean hasTransport(int transport) {
        return transports.contains(transport);
    }

    public NetworkSpecifier getNetworkSpecifier() {
        return specifier;
    }

    public int capabilityCount() {
        return capabilities.size();
    }

    public int transportCount() {
        return transports.size();
    }
}
