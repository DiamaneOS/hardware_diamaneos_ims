/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright 2026 The DiamaneOS Project
 */

package org.diamaneos.imsbroker;

import android.os.Handler;
import android.os.IBinder;
import android.os.RemoteException;
import android.os.ServiceManager;
import android.util.Log;

import vendor.diamaneos.hardware.imsdcm.IImsDcm;
import vendor.diamaneos.hardware.imsdcm.IPdnBroker;

/**
 * Finds the daemon's VINTF service, registers the broker with it and starts
 * over when the daemon dies.
 *
 * <p>The lookup uses ServiceManager.waitForDeclaredService(), a module-lib API
 * (the broker is built with sdk_version "module_current"). The same call is how
 * the stock system_ext QCRIL audio messenger reaches its vendor AIDL service on
 * this build.
 */
final class DcmConnection implements IBinder.DeathRecipient {
    private static final String TAG = "ImsBroker";
    private static final String SERVICE = IImsDcm.DESCRIPTOR + "/default";

    /** Called on the looper thread. */
    interface Listener {
        void onDaemonConnected(IImsDcm dcm);

        void onDaemonLost();
    }

    private final Handler mHandler;
    private final IPdnBroker.Stub mBroker;
    private final Listener mListener;

    DcmConnection(Handler handler, IPdnBroker.Stub broker, Listener listener) {
        mHandler = handler;
        mBroker = broker;
        mListener = listener;
    }

    /** Waits for the service on a background thread; the lookup blocks. */
    void connect() {
        Thread thread = new Thread(this::waitForService, "ImsBrokerLookup");
        thread.setDaemon(true);
        thread.start();
    }

    private void waitForService() {
        IBinder binder;
        try {
            // Blocks until the daemon registers. It stays blocked while the
            // daemon is stopped by its kill switch; nothing else is waiting.
            binder = ServiceManager.waitForDeclaredService(SERVICE);
        } catch (SecurityException e) {
            Log.e(TAG, "not allowed to look up the DCM service");
            return;
        }
        if (binder == null) {
            Log.e(TAG, "the DCM service is not declared in the VINTF manifest");
            return;
        }
        mHandler.post(() -> onServiceFound(binder));
    }

    private void onServiceFound(IBinder binder) {
        try {
            binder.linkToDeath(this, 0);
        } catch (RemoteException e) {
            connect();
            return;
        }
        IImsDcm dcm = IImsDcm.Stub.asInterface(binder);
        try {
            dcm.setBroker(mBroker);
        } catch (RemoteException e) {
            // The daemon died during the call; binderDied() starts over.
            return;
        } catch (RuntimeException e) {
            // EX_SECURITY and other refusals: a policy error, not a transient
            // one. Retrying would not help, so stay disconnected and say why.
            Log.e(TAG, "the DCM daemon refused the broker: " + e.getClass().getSimpleName());
            binder.unlinkToDeath(this, 0);
            return;
        }
        Log.i(TAG, "registered with the DCM daemon");
        mListener.onDaemonConnected(dcm);
    }

    @Override
    public void binderDied() {
        mHandler.post(() -> {
            Log.w(TAG, "the DCM daemon died");
            mListener.onDaemonLost();
            connect();
        });
    }
}
