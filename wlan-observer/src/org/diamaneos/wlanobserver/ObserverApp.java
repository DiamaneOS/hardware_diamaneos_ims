// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
package org.diamaneos.wlanobserver;

import android.app.Application;
import android.net.ConnectivityManager;
import android.net.LinkAddress;
import android.net.LinkProperties;
import android.net.Network;
import android.net.NetworkCapabilities;
import android.net.NetworkRequest;
import android.net.wifi.SupplicantState;
import android.net.wifi.WifiInfo;
import android.net.wifi.WifiManager;
import android.os.Binder;
import android.os.Handler;
import android.os.HandlerThread;
import android.os.IBinder;
import android.os.RemoteException;
import android.os.ServiceManager;
import android.os.UserManager;
import android.system.OsConstants;
import android.util.Log;
import java.net.Inet4Address;
import java.net.Inet6Address;
import java.net.InetAddress;
import org.diamaneos.wlan.IReporter;
import org.diamaneos.wlan.Snapshot;
import org.diamaneos.wlan.ReporterStatus;

/** Passive primary-user observations; no exported components, scans or network setters. */
public final class ObserverApp extends Application {
    private static final String SERVICE = "org.diamaneos.wlan.IReporter/default";
    private Handler handler;
    private ConnectivityManager connectivity;
    private WifiManager wifi;
    private IReporter reporter;
    private IBinder binder;
    private IBinder lifetime;
    private long generation;
    private long sequence;
    private String lastStatus;
    private int observationState = -1;
    private int lastObservationState = -1;
    private final Runnable heartbeat = new Runnable() {
        @Override public void run() {
            publish();
            handler.postDelayed(this, 10_000);
        }
    };

    @Override public void onCreate() {
        super.onCreate();
        if (!getSystemService(UserManager.class).isSystemUser()) return;
        connectivity = getSystemService(ConnectivityManager.class);
        wifi = getSystemService(WifiManager.class);
        HandlerThread thread = new HandlerThread("WlanObservation");
        thread.start();
        handler = new Handler(thread.getLooper());
        NetworkRequest request = new NetworkRequest.Builder()
                .addTransportType(NetworkCapabilities.TRANSPORT_WIFI)
                .addCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET).build();
        connectivity.registerNetworkCallback(request, new ConnectivityManager.NetworkCallback() {
            @Override public void onAvailable(Network network) { changed(); }
            @Override public void onLost(Network network) { changed(); }
            @Override public void onCapabilitiesChanged(Network n, NetworkCapabilities c) { changed(); }
            @Override public void onLinkPropertiesChanged(Network n, LinkProperties l) { changed(); }
            private void changed() {
                handler.removeCallbacks(heartbeat);
                handler.post(heartbeat);
            }
        }, handler);
        handler.post(heartbeat);
    }

    private Snapshot read() {
        Snapshot result = disconnected(wifi.isWifiEnabled());
        observationState = 0; // Wi-Fi disabled.
        if (!result.enabled) return result;
        observationState = 1; // No eligible Internet-capable non-VPN Wi-Fi link.
        Network selected = null;
        NetworkCapabilities selectedCaps = null;
        // Do not mix identities/addresses from multiple simultaneous Wi-Fi links.
        // Multi-STA requires a separately qualified primary-network association.
        for (Network network : connectivity.getAllNetworks()) {
            NetworkCapabilities caps = connectivity.getNetworkCapabilities(network);
            if (caps == null || !caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI)
                    || !caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
                    || !caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_VPN)) continue;
            if (selected != null) { observationState = 2; return result; }
            selected = network; selectedCaps = caps;
        }
        if (selected == null) return result;
        observationState = 3; // Association unavailable or incomplete.
        WifiInfo info = wifi.getConnectionInfo();
        if (info == null || info.getSupplicantState() != SupplicantState.COMPLETED) return result;
        observationState = 4; // Missing, redacted or unusable link identity.
        String value = info.getBSSID();
        if (value == null || !value.matches("(?i)[0-9a-f]{2}(:[0-9a-f]{2}){5}")) return result;
        String[] parts = value.split(":");
        for (int i = 0; i < 6; i++) result.bssid[i] = (byte) Integer.parseInt(parts[i], 16);
        if ((result.bssid[0] & 1) != 0 || value.equals("00:00:00:00:00:00")
                || value.equals("02:00:00:00:00:00")) return disconnected(result.enabled);
        observationState = 5; // Association does not match selected network.
        if (selectedCaps.getTransportInfo() instanceof WifiInfo transportInfo
                && transportInfo.getNetworkId() >= 0
                && transportInfo.getNetworkId() != info.getNetworkId()) return disconnected(result.enabled);
        observationState = 6; // Link properties unavailable.
        LinkProperties links = connectivity.getLinkProperties(selected);
        if (links == null) return disconnected(result.enabled);
        for (LinkAddress link : links.getLinkAddresses()) {
            int bad = OsConstants.IFA_F_TENTATIVE | OsConstants.IFA_F_DADFAILED | OsConstants.IFA_F_DEPRECATED;
            InetAddress ip = link.getAddress();
            if ((link.getFlags() & bad) != 0 || ip.isAnyLocalAddress() || ip.isLoopbackAddress()
                    || ip.isLinkLocalAddress() || ip.isMulticastAddress()) continue;
            if (ip instanceof Inet4Address && !result.hasIpv4) {
                result.ipv4 = ip.getAddress(); result.hasIpv4 = true;
            } else if (ip instanceof Inet6Address && !result.hasIpv6) {
                result.ipv6 = ip.getAddress(); result.ipv6Prefix = link.getPrefixLength(); result.hasIpv6 = true;
            }
        }
        // Match stock's same-link resolver metadata without performing DNS queries.
        // Bound each family to the two slots supported by the stock DSD request.
        for (InetAddress dns : new java.util.LinkedHashSet<>(links.getDnsServers())) {
            if (dns.isAnyLocalAddress() || dns.isLoopbackAddress() || dns.isMulticastAddress()) continue;
            if (dns instanceof Inet4Address && result.dns4Count < 2) {
                System.arraycopy(dns.getAddress(), 0, result.dns4, 4 * result.dns4Count++, 4);
            } else if (dns instanceof Inet6Address && result.dns6Count < 2) {
                System.arraycopy(dns.getAddress(), 0, result.dns6, 16 * result.dns6Count++, 16);
            }
        }
        observationState = 7; // Association changed while reading the snapshot.
        WifiInfo after = wifi.getConnectionInfo();
        NetworkCapabilities capsAfter = connectivity.getNetworkCapabilities(selected);
        if (after == null || after.getNetworkId() != info.getNetworkId()
                || !value.equals(after.getBSSID()) || capsAfter == null) return disconnected(result.enabled);
        result.connected = result.hasIpv4 || result.hasIpv6;
        observationState = result.connected ? 9 : 8; // Connected or no usable addresses.
        result.validated = selectedCaps.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED)
                && capsAfter.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED);
        if (result.connected && result.validated) observationState = 10;
        return result;
    }

    private static Snapshot disconnected(boolean enabled) {
        Snapshot value = new Snapshot();
        value.dns4 = new byte[8]; value.dns6 = new byte[32];
        value.enabled = enabled; value.bssid = new byte[6]; value.ipv4 = new byte[4]; value.ipv6 = new byte[16];
        return value;
    }
    private void unavailable(String category) {
        if (!category.equals(lastStatus)) {
            Log.w("WlanReporting", category);
            lastStatus = category;
        }
    }
    private void publish() {
        final Snapshot snapshot;
        try {
            snapshot = read();
            if (observationState != lastObservationState) {
                Log.i("WlanReporting", "observation=" + observationState);
                lastObservationState = observationState;
            }
        } catch (RuntimeException ignored) {
            unavailable("Observation read unavailable");
            return; // No refresh: the existing observation lease will expire.
        }
        try {
            if (reporter == null) {
                IBinder found = ServiceManager.checkService(SERVICE);
                if (found == null) { unavailable("Reporter unavailable"); return; }
                IReporter next = IReporter.Stub.asInterface(found);
                IBinder token = new Binder();
                long epoch = next.registerObserver(token);
                found.linkToDeath(() -> handler.post(() -> {
                    if (binder == found) { reporter = null; binder = null; lifetime = null; }
                }), 0);
                binder = found; reporter = next; lifetime = token; generation = epoch; sequence = 0;
                lastStatus = null;
            }
            if (sequence == Long.MAX_VALUE) { reporter = null; return; }
            reporter.observe(generation, ++sequence, snapshot);
            ReporterStatus status = reporter.getStatus(generation);
            String summary = "primary=" + status.primaryStage + "/" + status.primaryOperation
                    + "/" + status.primaryError + " secondary=" + status.secondaryStage
                    + "/" + status.secondaryOperation + "/" + status.secondaryError;
            if (!summary.equals(lastStatus)) {
                Log.i("WlanReporting", summary);
                lastStatus = summary;
            }
        } catch (RemoteException | RuntimeException ignored) {
            // No payload/exception logging. Failure cannot refresh the observation
            // lease; the reporter expires it and withdraws availability.
            reporter = null; binder = null; lifetime = null;
            unavailable("Observer delivery unavailable");
        }
    }
}
