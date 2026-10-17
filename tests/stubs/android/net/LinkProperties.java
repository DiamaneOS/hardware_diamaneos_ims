// SPDX-License-Identifier: Apache-2.0
// Host-only fixture for the broker simulations. Never part of an Android module.
package android.net;

import java.util.ArrayList;
import java.util.List;

public final class LinkProperties {
    private final List<LinkAddress> addresses = new ArrayList<>();
    private int mtu;

    public LinkProperties addLinkAddress(LinkAddress address) {
        addresses.add(address);
        return this;
    }

    public LinkProperties setMtu(int mtu) {
        this.mtu = mtu;
        return this;
    }

    public List<LinkAddress> getLinkAddresses() {
        return new ArrayList<>(addresses);
    }

    public int getMtu() {
        return mtu;
    }
}
