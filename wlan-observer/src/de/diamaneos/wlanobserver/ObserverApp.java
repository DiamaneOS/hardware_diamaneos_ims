// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
package de.diamaneos.wlanobserver;

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
import android.os.Build;
import android.os.Handler;
import android.os.HandlerThread;
import android.os.IBinder;
import android.os.RemoteException;
import android.os.ServiceManager;
import android.os.UserManager;
import android.util.Log;
import java.net.Inet4Address;
import java.net.Inet6Address;
import java.net.InetAddress;
import de.diamaneos.ims.AddressPolicy;
import de.diamaneos.wlan.IReporter;
import de.diamaneos.wlan.Snapshot;
import de.diamaneos.wlan.ReporterStatus;

/** Passive primary-user observations; no exported components, scans or network setters. */
public final class ObserverApp extends Application {
    private static final String SERVICE = "de.diamaneos.wlan.IReporter/default";
    private static final boolean DEBUG = Build.isDebuggable();
    private Handler handler;
    private ConnectivityManager connectivity;
    private WifiManager wifi;
    private IReporter reporter;
    private IBinder binder;
    private IBinder lifetime;
    private long generation;
    private long sequence;
    private String lastStatus;
    private final DefaultNetworkState<Network> defaultState = new DefaultNetworkState<>();
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
        connectivity.registerDefaultNetworkCallback(new ConnectivityManager.NetworkCallback() {
            @Override public void onAvailable(Network n) { defaultState.available(n); changed(); }
            @Override public void onLost(Network n) { defaultState.lost(n); changed(); }
            @Override public void onCapabilitiesChanged(Network n, NetworkCapabilities caps) {
                boolean wifiOnly = caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI);
                for (int transport : caps.getTransportTypes()) {
                    if (transport != NetworkCapabilities.TRANSPORT_WIFI
                            && transport != NetworkCapabilities.TRANSPORT_VPN) wifiOnly = false;
                }
                defaultState.capabilities(n, wifiOnly);
                changed();
            }
            private void changed() {
                handler.removeCallbacks(heartbeat);
                handler.post(heartbeat);
            }
        }, handler);
        handler.post(heartbeat);
    }

    private Snapshot read() {
        Snapshot result = disconnected(wifi.isWifiEnabled());
        if (!result.enabled) return result;
        // Use the same primary Wi-Fi link as WifiManager's connection identity.
        // Never join global WifiInfo to an arbitrary enumerated network.
        Network selected = wifi.getCurrentNetwork();
        if (selected == null) return result;
        NetworkCapabilities selectedCaps = connectivity.getNetworkCapabilities(selected);
        if (selectedCaps == null || !selectedCaps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI)
                || !selectedCaps.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
                || !selectedCaps.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_VPN)) return result;
        // The synchronous capabilities getter redacts WifiInfo's network ID.
        // Bind the global connection identity to this raw Wi-Fi Network through
        // the read-only primary-network getter instead of treating redaction as
        // evidence of association. No Settings or location permission is needed.
        boolean defaultBefore = defaultState.usesWifi();
        WifiInfo info = wifi.getConnectionInfo();
        if (info == null || info.getNetworkId() < 0
                || info.getSupplicantState() != SupplicantState.COMPLETED) return result;
        String value = info.getBSSID();
        if (value == null || !value.matches("(?i)[0-9a-f]{2}(:[0-9a-f]{2}){5}")) return result;
        String[] parts = value.split(":");
        for (int i = 0; i < 6; i++) result.bssid[i] = (byte) Integer.parseInt(parts[i], 16);
        if ((result.bssid[0] & 1) != 0 || value.equals("00:00:00:00:00:00")
                || value.equals("02:00:00:00:00:00")) return disconnected(result.enabled);
        if (selectedCaps.getTransportInfo() instanceof WifiInfo transportInfo
                && transportInfo.getNetworkId() >= 0
                && transportInfo.getNetworkId() != info.getNetworkId()) return disconnected(result.enabled);
        LinkProperties links = connectivity.getLinkProperties(selected);
        if (links == null) return disconnected(result.enabled);
        for (LinkAddress link : links.getLinkAddresses()) {
            InetAddress ip = link.getAddress();
            if (!AddressPolicy.isPreferred(link.getFlags())
                    || ip.isAnyLocalAddress() || ip.isLoopbackAddress()
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
        WifiInfo after = wifi.getConnectionInfo();
        NetworkCapabilities capsAfter = connectivity.getNetworkCapabilities(selected);
        if (!selected.equals(wifi.getCurrentNetwork()) || after == null
                || after.getSupplicantState() != SupplicantState.COMPLETED
                || after.getNetworkId() != info.getNetworkId()
                || !value.equals(after.getBSSID()) || capsAfter == null) return disconnected(result.enabled);
        result.connected = result.hasIpv4 || result.hasIpv6;
        result.defaultRoute = result.connected && defaultBefore && defaultState.usesWifi();
        result.validated = selectedCaps.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED)
                && capsAfter.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED);
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
            if (DEBUG) {
                try {
                    ReporterStatus status = reporter.getStatus(generation);
                    // Numeric connectivity facts only; no link or SIM identifiers.
                    int flags = (snapshot.enabled ? 1 : 0) | (snapshot.connected ? 2 : 0)
                            | (snapshot.validated ? 4 : 0) | (snapshot.defaultRoute ? 8 : 0);
                    String summary = "observation=" + flags + " primary=" + status.primaryStage
                            + "/" + status.primaryOperation
                            + "/" + status.primaryError + " secondary=" + status.secondaryStage
                            + "/" + status.secondaryOperation + "/" + status.secondaryError;
                    summary += " headers=" + status.primaryResponseHeaders + "/"
                            + status.primaryIndicationHeaders + "/" + status.primaryLastIndication
                            + "," + status.secondaryResponseHeaders + "/"
                            + status.secondaryIndicationHeaders + "/" + status.secondaryLastIndication;
                    summary += " header-enabled=" + status.diagnosticHeadersEnabled
                            + " registration-enabled=" + status.diagnosticRegistrationEnabled
                            + " notifications=" + indicationCounts(status.primaryIndicationHistogram)
                            + "," + indicationCounts(status.secondaryIndicationHistogram);
                    summary += " keepalive-failure=" + status.primaryKeepaliveFailureSent
                            + "/" + status.primaryKeepaliveFailureAcknowledged + "/"
                            + status.primaryKeepaliveFailureError + "/"
                            + status.primaryKeepaliveFailureDropped + ","
                            + status.secondaryKeepaliveFailureSent + "/"
                            + status.secondaryKeepaliveFailureAcknowledged + "/"
                            + status.secondaryKeepaliveFailureError + "/"
                            + status.secondaryKeepaliveFailureDropped;
                    if (!summary.equals(lastStatus)) {
                        Log.i("WlanReporting", summary);
                        lastStatus = summary;
                    }
                } catch (RemoteException | RuntimeException ignored) {
                    // A diagnostic read must not invalidate successful delivery.
                    unavailable("Reporter status unavailable");
                }
            } else {
                // Successful delivery ends any preceding operational error episode.
                lastStatus = null;
            }
        } catch (RemoteException | RuntimeException ignored) {
            // No payload/exception logging. Failure cannot refresh the observation
            // lease; the reporter expires it and withdraws availability.
            reporter = null; binder = null; lifetime = null;
            unavailable("Observer delivery unavailable");
        }
    }

    private static String indicationCounts(int[] values) {
        if (values == null || values.length != 64) return "unavailable";
        StringBuilder text = new StringBuilder();
        for (int index = 0; index < values.length; index++) {
            if (values[index] == 0) continue;
            if (text.length() != 0) text.append(';');
            text.append(Integer.toHexString(index + 0x20)).append(':').append(values[index]);
        }
        return text.length() == 0 ? "none" : text.toString();
    }
}
