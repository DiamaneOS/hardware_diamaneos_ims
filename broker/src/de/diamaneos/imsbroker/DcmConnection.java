/* SPDX-License-Identifier: Apache-2.0
 * Copyright 2026 The DiamaneOS Project
 */
package de.diamaneos.imsbroker;

import android.os.Handler;
import android.os.IBinder;
import android.os.RemoteException;
import android.os.ServiceManager;
import android.util.Log;

import vendor.diamaneos.hardware.imsdcm.IImsDcm;
import vendor.diamaneos.hardware.imsdcm.IPdnBroker;

import java.util.function.Supplier;

/** One lookup and one death link at a time. All mutable state is on the handler. */
final class DcmConnection {
    interface Listener {
        void onDaemonConnected(IImsDcm dcm);

        void onDaemonLost();
    }

    private static final String SERVICE = IImsDcm.DESCRIPTOR + "/default";
    private final Handler handler;
    private final Supplier<IPdnBroker.Stub> brokerFactory;
    private final Listener listener;
    private IBinder connected;
    private IBinder.DeathRecipient death;
    private boolean looking;
    private int attempt;

    DcmConnection(Handler handler, Supplier<IPdnBroker.Stub> factory, Listener listener) {
        this.handler = handler;
        brokerFactory = factory;
        this.listener = listener;
    }

    void connect() {
        if (looking || connected != null) return;
        looking = true;
        final int generation = ++attempt;
        Thread t =
                new Thread(
                        () -> {
                            IBinder found = null;
                            try {
                                found = ServiceManager.waitForDeclaredService(SERVICE);
                            } catch (RuntimeException ignored) {
                                /* No exception text or addresses. */
                            }
                            final IBinder result = found;
                            handler.post(() -> attach(generation, result));
                        },
                        "ImsBrokerLookup");
        t.setDaemon(true);
        t.start();
    }

    private void attach(int generation, IBinder b) {
        if (generation != attempt) return;
        looking = false;
        if (b == null) {
            Log.e("ImsBroker", "DCM service not declared or not accessible");
            handler.postDelayed(this::connect, 5000);
            return;
        }
        connected = b;
        death = () -> handler.post(() -> lost(b));
        try {
            b.linkToDeath(death, 0);
            IImsDcm dcm = IImsDcm.Stub.asInterface(b);
            if (dcm.getInterfaceVersion() != IImsDcm.VERSION) {
                Log.e("ImsBroker", "unsupported DCM interface version");
                lost(b);
                return;
            }
            dcm.setBroker(brokerFactory.get());
            listener.onDaemonConnected(dcm);
        } catch (RemoteException | RuntimeException e) {
            lost(b);
        }
    }

    private void lost(IBinder b) {
        if (b != connected) return; // A stale death notification must not clear its successor.
        if (death != null) {
            try {
                b.unlinkToDeath(death, 0);
            } catch (java.util.NoSuchElementException ignored) {
                /* linkToDeath failed. */
            }
        }
        connected = null;
        death = null;
        listener.onDaemonLost();
        handler.postDelayed(this::connect, 1000);
    }
}
