// SPDX-License-Identifier: Apache-2.0
// Host-only fixture for the broker simulations. Never part of an Android module.
package android.os;

import java.util.concurrent.BlockingQueue;
import java.util.concurrent.LinkedBlockingQueue;

/** Each lookup returns the next daemon instance the simulation provides. */
public final class ServiceManager {
    public static final BlockingQueue<IBinder> daemons = new LinkedBlockingQueue<>();

    private ServiceManager() {}

    public static IBinder waitForDeclaredService(String name) {
        try {
            return daemons.take();
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            return null;
        }
    }
}
