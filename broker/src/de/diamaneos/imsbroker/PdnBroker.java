/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright 2026 The DiamaneOS Project
 */

package de.diamaneos.imsbroker;

import android.content.Context;
import android.net.ConnectivityManager;
import android.net.NetworkCapabilities;
import android.net.NetworkRequest;
import android.net.TelephonyNetworkSpecifier;
import android.os.Binder;
import android.os.Handler;
import android.os.RemoteException;
import android.telephony.SubscriptionManager;
import android.util.Log;
import android.util.SparseArray;

import vendor.diamaneos.hardware.imsdcm.IImsDcm;
import vendor.diamaneos.hardware.imsdcm.IPdnBroker;
import vendor.diamaneos.hardware.imsdcm.PdnFailure;
import vendor.diamaneos.hardware.imsdcm.PdnInfo;
import vendor.diamaneos.hardware.imsdcm.PdnRequest;
import vendor.diamaneos.hardware.imsdcm.PdnType;

import java.util.ArrayList;
import java.util.List;

/**
 * Files and holds the IMS and emergency network requests the IMS DCM daemon asks for, and reports
 * each network's handle, addresses and MTU back to it.
 *
 * <p>Filing matches stock CneApp (DataCallAgent in its classes.dex):
 *
 * <ul>
 *   <li>Only IMS or EIMS, on TRANSPORT_CELLULAR, built in the same order as
 *       Tracker.createNetworkRequest (0x00f8e0, 0x00f964). Never MMTEL.
 *   <li>With a slot, the request carries that slot's subscription; without a slot, it carries none
 *       (startDataCall, 0x00fe74).
 *   <li>While the slot has no active subscription, the request waits and is filed on the next
 *       subscription change (startDataCall "pending list", updateSubInfoReady 0x010188). This
 *       applies to emergency requests too.
 *   <li>No timeout of our own: ConnectivityManager.requestNetwork() without one.
 *   <li>The request is held until the daemon releases it; a lost network does not drop it (Tracker
 *       callbacks 0x00f418 to 0x00f5fc, tearDownDataCall 0x0100ac).
 * </ul>
 *
 * <p>It never reports a network as up unless ConnectivityManager says it is available now and it
 * has the requested capability on cellular.
 *
 * <p>All state lives on one looper thread. Binder calls from the daemon and death notifications are
 * posted to it.
 */
final class PdnBroker implements PdnTracker.Listener, DcmConnection.Listener {
    private static final String TAG = "ImsBroker";

    /**
     * The daemon's UID: AID_VENDOR_IMSDCM in vintf/config.fs. Keep the two equal; a mismatch makes
     * the broker ignore the daemon (and log the UID).
     */
    private static final int DAEMON_UID = 2990;

    /** Stock CneApp accepts slots 0..2 (RatInfo.isSlotIdValid, 0x014ad0). */
    private static final int MAX_SLOTS = 3;

    private final Handler mHandler;
    private final ConnectivityManager mConnectivityManager;
    private final SubscriptionManager mSubscriptionManager;
    private final DcmConnection mConnection;
    private final SparseArray<PdnTracker> mTrackers = new SparseArray<>();
    private final SubscriptionManager.OnSubscriptionsChangedListener mSubscriptionsListener =
            new SubscriptionManager.OnSubscriptionsChangedListener() {
                @Override
                public void onSubscriptionsChanged() {
                    fileWaitingRequests();
                }
            };

    private IImsDcm mDcm;
    private long mBinderEpoch;

    PdnBroker(Context context, Handler handler) {
        mHandler = handler;
        mConnectivityManager = context.getSystemService(ConnectivityManager.class);
        mSubscriptionManager = context.getSystemService(SubscriptionManager.class);
        mConnection = new DcmConnection(handler, () -> new BrokerBinder(++mBinderEpoch), this);
    }

    void start() {
        mSubscriptionManager.addOnSubscriptionsChangedListener(
                mHandler::post, mSubscriptionsListener);
        mConnection.connect();
    }

    // DcmConnection.Listener

    @Override
    public void onDaemonConnected(IImsDcm dcm) {
        mDcm = dcm;
    }

    @Override
    public void onDaemonLost() {
        // The daemon tells the modem again when it restarts; hold nothing for
        // a daemon that is gone.
        mDcm = null;
        ++mBinderEpoch;
        for (int i = 0; i < mTrackers.size(); i++) {
            unregister(mTrackers.valueAt(i));
        }
        mTrackers.clear();
    }

    // Requests from the daemon

    private void bringUp(PdnRequest request) {
        if (mDcm == null) {
            return;
        }
        if (!isValid(request)) {
            Log.w(TAG, "invalid bring-up: slot=" + request.slot + " type=" + request.type);
            reportFailed(request, PdnFailure.INVALID_REQUEST);
            return;
        }
        int key = key(request.slot, request.type);
        PdnTracker tracker = mTrackers.get(key);
        if (tracker != null) {
            // Already held: no new request (telephony counts request churn as
            // an anomaly). Answer the new serial with the current state.
            tracker.serial = request.serial;
            Log.i(TAG, tracker + ": bring-up for a held request");
            if (tracker.reported != null && !tracker.isSettling()) {
                reportUp(tracker, tracker.reported);
            } else if (tracker.lost) {
                reportDown(tracker);
            }
            return;
        }
        tracker = new PdnTracker(this, request);
        mTrackers.put(key, tracker);
        Log.i(TAG, tracker + ": bring-up");
        fileRequest(tracker);
    }

    private void release(PdnRequest request) {
        if (!isValid(request)) {
            return;
        }
        int key = key(request.slot, request.type);
        PdnTracker tracker = mTrackers.get(key);
        if (tracker == null || tracker.serial != request.serial) {
            return;
        }
        Log.i(TAG, tracker + ": release");
        mTrackers.remove(key);
        unregister(tracker);
    }

    private static boolean isValid(PdnRequest request) {
        if (request == null || request.serial <= 0) return false;
        boolean slotOk =
                request.slot == PdnRequest.SLOT_UNSPECIFIED
                        || (request.slot >= 0 && request.slot < MAX_SLOTS);
        boolean typeOk = request.type == PdnType.IMS || request.type == PdnType.EMERGENCY;
        return slotOk
                && typeOk
                && (request.slot != PdnRequest.SLOT_UNSPECIFIED
                        || request.type == PdnType.EMERGENCY);
    }

    /** One key per (slot, type); slot is -1..2 and type 1..2 after isValid(). */
    private static int key(int slot, int type) {
        return ((slot + 1) << 8) | (type & 0xff);
    }

    // Filing

    private void fileWaitingRequests() {
        // fileRequest() may remove a tracker, so iterate over a copy.
        List<PdnTracker> waiting = new ArrayList<>();
        for (int i = 0; i < mTrackers.size(); i++) {
            PdnTracker current = mTrackers.valueAt(i);
            if (current.filed
                    && current.slot != PdnRequest.SLOT_UNSPECIFIED
                    && SubscriptionManager.getSubscriptionId(current.slot) != current.subId) {
                // Do not move a held IMS request to a different subscription silently.
                mTrackers.remove(key(current.slot, current.type));
                unregister(current);
                reportFailed(current.request(), PdnFailure.UNAVAILABLE);
                i--;
            } else if (!current.filed) {
                waiting.add(mTrackers.valueAt(i));
            }
        }
        for (PdnTracker tracker : waiting) {
            fileRequest(tracker);
        }
    }

    /**
     * Files the tracker's request, or leaves it waiting if its slot has no active subscription. A
     * valid SubscriptionManager.getSubscriptionId(slot) means an active subscription on that slot,
     * the same test stock makes with isActiveSubscriptionId(), without needing a phone-state
     * permission.
     */
    private void fileRequest(PdnTracker tracker) {
        NetworkRequest.Builder builder =
                new NetworkRequest.Builder()
                        .addCapability(tracker.capability())
                        .addTransportType(NetworkCapabilities.TRANSPORT_CELLULAR);
        if (tracker.slot != PdnRequest.SLOT_UNSPECIFIED) {
            int subId = SubscriptionManager.getSubscriptionId(tracker.slot);
            if (!SubscriptionManager.isUsableSubscriptionId(subId)) {
                Log.i(TAG, tracker + ": waiting for an active subscription");
                return;
            }
            tracker.subId = subId;
            builder.setNetworkSpecifier(
                    new TelephonyNetworkSpecifier.Builder().setSubscriptionId(subId).build());
        }
        try {
            mConnectivityManager.requestNetwork(builder.build(), tracker, mHandler);
        } catch (RuntimeException e) {
            // SecurityException without the permission; also
            // IllegalArgumentException and TooManyRequestsException.
            Log.e(TAG, tracker + ": request refused: " + e.getClass().getSimpleName());
            mTrackers.remove(key(tracker.slot, tracker.type));
            tracker.close();
            reportFailed(tracker.request(), PdnFailure.REQUEST_REJECTED);
            return;
        }
        tracker.filed = true;
        Log.i(TAG, tracker + ": requested");
    }

    private void unregister(PdnTracker tracker) {
        tracker.close();
        if (!tracker.filed) {
            return;
        }
        tracker.filed = false;
        try {
            mConnectivityManager.unregisterNetworkCallback(tracker);
        } catch (IllegalArgumentException e) {
            // Already unregistered by ConnectivityService.
        }
    }

    // PdnTracker.Listener

    @Override
    public void onTrackerChanged(PdnTracker tracker) {
        if (mTrackers.get(key(tracker.slot, tracker.type)) != tracker) return;
        PdnInfo info = tracker.currentInfo();
        if (info == null && tracker.isSettling()) {
            // ConnectivityService moved the request to a new network: decide once
            // its capabilities and link properties are both known. Stock reports
            // the new network without a down; a mismatch still fails closed then.
            return;
        }
        if (info == null) {
            if (tracker.reported != null) {
                // Up before, but no longer a matching network: fail closed.
                Log.w(TAG, tracker + ": network no longer matches");
                tracker.reported = null;
                tracker.lost = true;
                reportDown(tracker);
            }
            return;
        }
        if (PdnTracker.sameInfo(info, tracker.reported)) {
            return;
        }
        tracker.reported = info;
        tracker.lost = false;
        Log.i(
                TAG,
                tracker
                        + ": up, v4="
                        + (info.ipv4Address.length > 0)
                        + " v6="
                        + (info.ipv6Address.length > 0)
                        + " mtu="
                        + info.mtu);
        reportUp(tracker, info);
    }

    @Override
    public void onTrackerLost(PdnTracker tracker) {
        if (mTrackers.get(key(tracker.slot, tracker.type)) != tracker) return;
        Log.i(TAG, tracker + ": lost");
        tracker.reported = null;
        tracker.lost = true;
        reportDown(tracker);
    }

    @Override
    public void onTrackerUnavailable(PdnTracker tracker) {
        if (mTrackers.get(key(tracker.slot, tracker.type)) != tracker) return;
        Log.w(TAG, tracker + ": unavailable");
        // ConnectivityService has already removed the request.
        tracker.filed = false;
        tracker.close();
        mTrackers.remove(key(tracker.slot, tracker.type));
        reportFailed(tracker.request(), PdnFailure.UNAVAILABLE);
    }

    // Reports to the daemon (oneway; a dead daemon is handled by binderDied)

    private void reportUp(PdnTracker tracker, PdnInfo info) {
        try {
            if (mDcm != null) {
                mDcm.onPdnUp(tracker.request(), info);
            }
        } catch (RemoteException e) {
            // binderDied() follows.
        }
    }

    private void reportDown(PdnTracker tracker) {
        try {
            if (mDcm != null) {
                mDcm.onPdnDown(tracker.request());
            }
        } catch (RemoteException e) {
            // binderDied() follows.
        }
    }

    private void reportFailed(PdnRequest request, int reason) {
        try {
            if (mDcm != null) {
                mDcm.onPdnFailed(request, reason);
            }
        } catch (RemoteException e) {
            // binderDied() follows.
        }
    }

    /** The broker's binder, held only by the daemon. Calls from any other UID are ignored. */
    private final class BrokerBinder extends IPdnBroker.Stub {
        private final long epoch;

        BrokerBinder(long epoch) {
            this.epoch = epoch;
        }

        @Override
        public void bringUp(PdnRequest request) {
            if (request != null && isFromDaemon()) {
                mHandler.post(
                        () -> {
                            if (epoch == mBinderEpoch) PdnBroker.this.bringUp(request);
                        });
            }
        }

        @Override
        public void release(PdnRequest request) {
            if (request != null && isFromDaemon()) {
                mHandler.post(
                        () -> {
                            if (epoch == mBinderEpoch) PdnBroker.this.release(request);
                        });
            }
        }

        @Override
        public int getInterfaceVersion() {
            return IPdnBroker.VERSION;
        }

        @Override
        public String getInterfaceHash() {
            return IPdnBroker.HASH;
        }

        private boolean isFromDaemon() {
            int uid = Binder.getCallingUid();
            if (uid != DAEMON_UID) {
                Log.w(TAG, "ignoring a call from uid " + uid);
                return false;
            }
            return true;
        }
    }
}
