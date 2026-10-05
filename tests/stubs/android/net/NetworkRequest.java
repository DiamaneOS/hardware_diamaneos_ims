// SPDX-License-Identifier: Apache-2.0
// Host-only fixture for the broker simulations. Never part of an Android module.
package android.net;

/** Records what was requested; the simulations inspect it. */
public final class NetworkRequest {
    private final NetworkCapabilities capabilities;

    private NetworkRequest(NetworkCapabilities capabilities) {
        this.capabilities = capabilities;
    }

    public boolean hasCapability(int capability) {
        return capabilities.hasCapability(capability);
    }

    public boolean hasTransport(int transport) {
        return capabilities.hasTransport(transport);
    }

    public NetworkSpecifier getNetworkSpecifier() {
        return capabilities.getNetworkSpecifier();
    }

    public int capabilityCount() {
        return capabilities.capabilityCount();
    }

    public int transportCount() {
        return capabilities.transportCount();
    }

    public static final class Builder {
        private final NetworkCapabilities capabilities = new NetworkCapabilities();

        public Builder addCapability(int capability) {
            capabilities.addCapability(capability);
            return this;
        }

        public Builder addTransportType(int transport) {
            capabilities.addTransportType(transport);
            return this;
        }

        public Builder setNetworkSpecifier(NetworkSpecifier specifier) {
            capabilities.setNetworkSpecifier(specifier);
            return this;
        }

        public NetworkRequest build() {
            return new NetworkRequest(capabilities);
        }
    }
}
