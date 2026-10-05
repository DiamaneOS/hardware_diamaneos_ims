// SPDX-License-Identifier: Apache-2.0
// Host-only fixture for the broker simulations. Never part of an Android module.
package android.net;

import android.os.Handler;

import java.util.ArrayList;
import java.util.List;

/**
 * Records filed and withdrawn requests. Only the overload without a timeout
 * exists, so broker code that adds a timeout of its own does not compile here.
 */
public class ConnectivityManager {
    public final List<NetworkRequest> requests = new ArrayList<>();
    public final List<NetworkCallback> callbacks = new ArrayList<>();
    public final List<NetworkCallback> unregistered = new ArrayList<>();
    public RuntimeException refuseNext;

    public void requestNetwork(NetworkRequest request, NetworkCallback callback, Handler handler) {
        if (refuseNext != null) {
            RuntimeException refusal = refuseNext;
            refuseNext = null;
            throw refusal;
        }
        requests.add(request);
        callbacks.add(callback);
    }

    public void unregisterNetworkCallback(NetworkCallback callback) {
        unregistered.add(callback);
    }

    public static class NetworkCallback {
        public void onAvailable(Network network) {}

        public void onCapabilitiesChanged(Network network, NetworkCapabilities capabilities) {}

        public void onLinkPropertiesChanged(Network network, LinkProperties linkProperties) {}

        public void onBlockedStatusChanged(Network network, boolean blocked) {}

        public void onLost(Network network) {}

        public void onUnavailable() {}
    }
}
