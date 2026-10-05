// SPDX-License-Identifier: Apache-2.0
// Host-only fixture for the broker simulations. Never part of an Android module.
package android.net;

public final class Network {
    private final long handle;

    public Network(long handle) {
        this.handle = handle;
    }

    public long getNetworkHandle() {
        return handle;
    }

    @Override
    public boolean equals(Object other) {
        return other instanceof Network && ((Network) other).handle == handle;
    }

    @Override
    public int hashCode() {
        return Long.hashCode(handle);
    }
}
