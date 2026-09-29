/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright 2026 The DiamaneOS Project
 */

package de.diamaneos.imsbroker;

import android.net.ConnectivityManager;
import android.net.LinkAddress;
import android.net.LinkProperties;
import android.net.Network;
import android.net.NetworkCapabilities;
import android.net.NetworkSpecifier;
import android.net.TelephonyNetworkSpecifier;
import android.telephony.SubscriptionManager;

import de.diamaneos.ims.AddressPolicy;

import vendor.diamaneos.hardware.imsdcm.PdnInfo;
import vendor.diamaneos.hardware.imsdcm.PdnRequest;
import vendor.diamaneos.hardware.imsdcm.PdnType;

import java.net.InetAddress;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;

/**
 * One network request the broker holds for a (slot, type), and what it knows about the network that
 * satisfies it. Used only on the broker's looper thread; ConnectivityManager delivers the callbacks
 * there.
 */
final class PdnTracker extends ConnectivityManager.NetworkCallback {
    /** Called on the looper thread when the tracker's reportable state changes. */
    interface Listener {
        void onTrackerChanged(PdnTracker tracker);

        void onTrackerLost(PdnTracker tracker);

        void onTrackerUnavailable(PdnTracker tracker);
    }

    final int slot;
    final int type;

    /** Serial of the latest bring-up the daemon sent for this (slot, type). */
    int serial;

    /** The subscription the request carries, or INVALID_SUBSCRIPTION_ID. */
    int subId = SubscriptionManager.INVALID_SUBSCRIPTION_ID;

    /** True once the request is filed with ConnectivityManager. */
    boolean filed;

    /** The last PdnInfo reported up, or null when the network is not up. */
    PdnInfo reported;

    /** True after the network was lost, until it is up again. */
    boolean lost;

    private final Listener mListener;
    private final NetworkState<Network, NetworkCapabilities, LinkProperties> mState =
            new NetworkState<>();

    PdnTracker(Listener listener, PdnRequest request) {
        mListener = listener;
        slot = request.slot;
        type = request.type;
        serial = request.serial;
    }

    /** The request this tracker answers, as the daemon sent it last. */
    PdnRequest request() {
        PdnRequest request = new PdnRequest();
        request.slot = slot;
        request.type = type;
        request.serial = serial;
        return request;
    }

    int capability() {
        return type == PdnType.EMERGENCY
                ? NetworkCapabilities.NET_CAPABILITY_EIMS
                : NetworkCapabilities.NET_CAPABILITY_IMS;
    }

    /** Stops callbacks. The caller unregisters the request if it was filed. */
    void close() {
        mState.close();
    }

    /**
     * What to report up, or null if the network is not usable: it must be available (between
     * onAvailable and onLost), its capabilities and link properties must be known, and it must be a
     * cellular network with the requested capability and, when the request named one, the same
     * subscription.
     */
    PdnInfo currentInfo() {
        Network mNetwork = mState.network();
        NetworkCapabilities mCapabilities = mState.capabilities();
        LinkProperties mLinkProperties = mState.links();
        if (mNetwork == null
                || mCapabilities == null
                || mLinkProperties == null
                || !mState.usable()) {
            return null;
        }
        if (!mCapabilities.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR)
                || !mCapabilities.hasCapability(capability())
                || !specifierMatches(mCapabilities.getNetworkSpecifier())) {
            return null;
        }
        List<InetAddress> addresses = new ArrayList<>();
        for (LinkAddress linkAddress : mLinkProperties.getLinkAddresses()) {
            if (AddressPolicy.isPreferred(linkAddress.getFlags())) {
                addresses.add(linkAddress.getAddress());
            }
        }
        PdnInfo info = new PdnInfo();
        info.networkHandle = mNetwork.getNetworkHandle();
        info.ipv4Address = Addresses.pickIpv4(addresses);
        info.ipv6Address = Addresses.pickIpv6(addresses);
        info.mtu = Math.max(mLinkProperties.getMtu(), 0);
        return info;
    }

    private boolean specifierMatches(NetworkSpecifier specifier) {
        if (!SubscriptionManager.isUsableSubscriptionId(subId) || specifier == null) {
            // ConnectivityService only matches networks that satisfy the
            // request's specifier, so an absent one is not a mismatch.
            return true;
        }
        return specifier instanceof TelephonyNetworkSpecifier
                && ((TelephonyNetworkSpecifier) specifier).getSubscriptionId() == subId;
    }

    /**
     * True from onAvailable() until that network's capabilities and link properties are known and
     * the network is not blocked for this UID. ConnectivityService delivers them in the same
     * dispatch as onAvailable (ConnectivityManager's four-argument onAvailable), so this only spans
     * the calls of one dispatch.
     */
    boolean isSettling() {
        return mState.settling();
    }

    static boolean sameInfo(PdnInfo a, PdnInfo b) {
        if (a == null || b == null) {
            return a == b;
        }
        return a.networkHandle == b.networkHandle
                && a.mtu == b.mtu
                && Arrays.equals(a.ipv4Address, b.ipv4Address)
                && Arrays.equals(a.ipv6Address, b.ipv6Address);
    }

    @Override
    public void onAvailable(Network network) {
        mState.available(network);
    }

    @Override
    public void onCapabilitiesChanged(Network network, NetworkCapabilities capabilities) {
        if (!mState.capabilities(network, capabilities)) return;
        mListener.onTrackerChanged(this);
    }

    @Override
    public void onLinkPropertiesChanged(Network network, LinkProperties linkProperties) {
        if (!mState.links(network, linkProperties)) return;
        mListener.onTrackerChanged(this);
    }

    @Override
    public void onBlockedStatusChanged(Network network, boolean blocked) {
        if (mState.blocked(network, blocked)) mListener.onTrackerChanged(this);
    }

    @Override
    public void onLost(Network network) {
        if (!mState.lost(network)) return;
        mListener.onTrackerLost(this);
    }

    @Override
    public void onUnavailable() {
        if (mState.closed()) {
            return;
        }
        mListener.onTrackerUnavailable(this);
    }

}
