/* SPDX-License-Identifier: Apache-2.0
 * Copyright 2026 The DiamaneOS Project
 */
package de.diamaneos.imsbroker;

import java.util.Objects;

/** Callback ordering shared by the Android tracker and host simulations. */
final class NetworkState<N, C, L> {
    private N network;
    private C capabilities;
    private L links;
    private boolean closed;

    void available(N n) {
        if (!closed) {
            network = n;
            capabilities = null;
            links = null;
        }
    }

    boolean capabilities(N n, C c) {
        if (!matches(n)) return false;
        capabilities = c;
        return true;
    }

    boolean links(N n, L l) {
        if (!matches(n)) return false;
        links = l;
        return true;
    }

    boolean lost(N n) {
        if (!matches(n)) return false;
        network = null;
        capabilities = null;
        links = null;
        return true;
    }

    boolean matches(N n) {
        return !closed && network != null && Objects.equals(network, n);
    }

    boolean settling() {
        return network != null && (capabilities == null || links == null);
    }

    boolean closed() {
        return closed;
    }

    void close() {
        closed = true;
        network = null;
        capabilities = null;
        links = null;
    }

    N network() {
        return network;
    }

    C capabilities() {
        return capabilities;
    }

    L links() {
        return links;
    }
}
