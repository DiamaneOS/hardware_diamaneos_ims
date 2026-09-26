/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright 2026 The DiamaneOS Project
 */

package org.diamaneos.imsbroker;

import android.app.Application;
import android.os.Handler;
import android.os.Looper;

/**
 * The broker has no activity, service or receiver. It is a persistent, direct-boot-aware system
 * app, so the system starts its process at boot, before the first unlock, and restarts it if it
 * dies; everything runs from here on the main looper.
 */
public final class ImsBrokerApp extends Application {
    @Override
    public void onCreate() {
        super.onCreate();
        if (!getSystemService(android.os.UserManager.class).isSystemUser()) return;
        Handler handler = new Handler(Looper.getMainLooper());
        new PdnBroker(this, handler).start();
    }
}
