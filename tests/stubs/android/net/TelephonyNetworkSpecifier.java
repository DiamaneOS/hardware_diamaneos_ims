// SPDX-License-Identifier: Apache-2.0
// Host-only fixture for the broker simulations. Never part of an Android module.
package android.net;

public final class TelephonyNetworkSpecifier extends NetworkSpecifier {
    private final int subId;

    private TelephonyNetworkSpecifier(int subId) {
        this.subId = subId;
    }

    public int getSubscriptionId() {
        return subId;
    }

    public static final class Builder {
        private int subId = -1;

        public Builder setSubscriptionId(int subId) {
            this.subId = subId;
            return this;
        }

        public TelephonyNetworkSpecifier build() {
            return new TelephonyNetworkSpecifier(subId);
        }
    }
}
