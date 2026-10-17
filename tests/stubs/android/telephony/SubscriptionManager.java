// SPDX-License-Identifier: Apache-2.0
// Host-only fixture for the broker simulations. Never part of an Android module.
package android.telephony;

import java.util.HashMap;
import java.util.Map;
import java.util.concurrent.Executor;

/** Active subscription per logical slot, set by the simulation. */
public class SubscriptionManager {
    public static final int INVALID_SUBSCRIPTION_ID = -1;
    public static final Map<Integer, Integer> active = new HashMap<>();
    public Executor executor;
    public OnSubscriptionsChangedListener listener;

    public static int getSubscriptionId(int slot) {
        return active.getOrDefault(slot, INVALID_SUBSCRIPTION_ID);
    }

    public static boolean isUsableSubscriptionId(int subId) {
        return subId >= 0 && subId != Integer.MAX_VALUE;
    }

    public void addOnSubscriptionsChangedListener(
            Executor executor, OnSubscriptionsChangedListener listener) {
        this.executor = executor;
        this.listener = listener;
    }

    /** As the platform does: the callback arrives through the given executor. */
    public void notifyChanged() {
        executor.execute(listener::onSubscriptionsChanged);
    }

    public static class OnSubscriptionsChangedListener {
        public void onSubscriptionsChanged() {}
    }
}
