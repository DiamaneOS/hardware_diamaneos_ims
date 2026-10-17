// SPDX-License-Identifier: Apache-2.0
// Host-only fixture for the broker simulations. Never part of an Android module.
package android.net;

import java.net.InetAddress;

public final class LinkAddress {
    private final InetAddress address;
    private final int flags;

    public LinkAddress(InetAddress address, int flags) {
        this.address = address;
        this.flags = flags;
    }

    public InetAddress getAddress() {
        return address;
    }

    public int getFlags() {
        return flags;
    }
}
