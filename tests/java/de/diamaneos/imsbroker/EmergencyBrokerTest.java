/* SPDX-License-Identifier: Apache-2.0
 * Copyright 2026 The DiamaneOS Project
 */
package de.diamaneos.imsbroker;

import android.content.Context;
import android.net.ConnectivityManager;
import android.net.LinkAddress;
import android.net.LinkProperties;
import android.net.Network;
import android.net.NetworkCapabilities;
import android.net.NetworkRequest;
import android.net.TelephonyNetworkSpecifier;
import android.os.Binder;
import android.os.Handler;
import android.os.IBinder;
import android.os.RemoteException;
import android.os.ServiceManager;
import android.telephony.SubscriptionManager;

import vendor.diamaneos.hardware.imsdcm.IImsDcm;
import vendor.diamaneos.hardware.imsdcm.IPdnBroker;
import vendor.diamaneos.hardware.imsdcm.PdnFailure;
import vendor.diamaneos.hardware.imsdcm.PdnInfo;
import vendor.diamaneos.hardware.imsdcm.PdnRequest;
import vendor.diamaneos.hardware.imsdcm.PdnType;

import java.net.InetAddress;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;

/**
 * Emergency (EIMS) simulations of the real PdnBroker, PdnTracker and DcmConnection, run on the
 * host against recording fixtures (tests/stubs). Nothing here can file a real network request,
 * reach the daemon or touch a modem.
 */
public final class EmergencyBrokerTest {
    private static final int DAEMON_UID = 2990;
    private static final int SOS = PdnType.EMERGENCY;
    private static final int IMS = PdnType.IMS;
    private static final int NO_SLOT = PdnRequest.SLOT_UNSPECIFIED;
    private static final int EIMS_CAP = NetworkCapabilities.NET_CAPABILITY_EIMS;
    private static final int IMS_CAP = NetworkCapabilities.NET_CAPABILITY_IMS;
    private static final int CELLULAR = NetworkCapabilities.TRANSPORT_CELLULAR;

    /** One daemon instance: the binder the broker looks up and the interface it serves. */
    static final class Daemon implements IBinder, IImsDcm {
        IPdnBroker broker;
        IBinder.DeathRecipient death;
        final List<String> reports = new ArrayList<>();
        final List<PdnInfo> infos = new ArrayList<>();
        /** Reports that fail while the daemon lives (a full binder buffer): no death follows. */
        int failNext;

        private void deliver() throws RemoteException {
            if (failNext > 0) {
                failNext--;
                throw new RemoteException();
            }
        }

        @Override
        public void linkToDeath(IBinder.DeathRecipient recipient, int flags) {
            death = recipient;
        }

        @Override
        public boolean unlinkToDeath(IBinder.DeathRecipient recipient, int flags) {
            return true;
        }

        @Override
        public void setBroker(IPdnBroker broker) {
            this.broker = broker;
        }

        @Override
        public void onPdnUp(PdnRequest request, PdnInfo info) throws RemoteException {
            deliver();
            reports.add("up " + id(request) + " " + info.networkHandle);
            infos.add(info);
        }

        @Override
        public void onPdnDown(PdnRequest request) throws RemoteException {
            deliver();
            reports.add("down " + id(request));
        }

        @Override
        public void onPdnFailed(PdnRequest request, int reason) throws RemoteException {
            deliver();
            reports.add("failed " + id(request) + " " + reason);
        }

        @Override
        public int getInterfaceVersion() {
            return IImsDcm.VERSION;
        }

        List<String> take() {
            List<String> out = new ArrayList<>(reports);
            reports.clear();
            return out;
        }
    }

    static String id(PdnRequest r) {
        String type = r.type == SOS ? "sos" : r.type == IMS ? "ims" : "type" + r.type;
        return type + r.slot + "#" + r.serial;
    }

    static PdnRequest request(int slot, int type, int serial) {
        PdnRequest r = new PdnRequest();
        r.slot = slot;
        r.type = type;
        r.serial = serial;
        return r;
    }

    /** The broker and its fixtures, registered with a first daemon. */
    static final class Rig {
        final Handler handler = new Handler();
        final ConnectivityManager cm = new ConnectivityManager();
        final SubscriptionManager subs = new SubscriptionManager();
        Daemon daemon = new Daemon();

        Rig() throws Exception {
            SubscriptionManager.active.clear();
            ServiceManager.daemons.clear();
            Binder.callingUid = DAEMON_UID;
            Context context = new Context();
            context.putSystemService(ConnectivityManager.class, cm);
            context.putSystemService(SubscriptionManager.class, subs);
            ServiceManager.daemons.add(daemon);
            new PdnBroker(context, handler).start();
            handler.awaitWork(5000);
            handler.drain();
            check(daemon.broker != null, "broker registers with the daemon");
        }

        void bringUp(int slot, int type, int serial) throws Exception {
            daemon.broker.bringUp(request(slot, type, serial));
            handler.drain();
        }

        void release(int slot, int type, int serial) throws Exception {
            daemon.broker.release(request(slot, type, serial));
            handler.drain();
        }

        ConnectivityManager.NetworkCallback callback(int index) {
            return cm.callbacks.get(index);
        }

        NetworkRequest filed(int index) {
            return cm.requests.get(index);
        }

        void subscriptions(int slot, Integer subId) {
            if (subId == null) {
                SubscriptionManager.active.remove(slot);
            } else {
                SubscriptionManager.active.put(slot, subId);
            }
            subs.notifyChanged();
            handler.drain();
        }

        void up(ConnectivityManager.NetworkCallback cb, Network n, NetworkCapabilities caps,
                LinkProperties links) {
            cb.onAvailable(n);
            cb.onCapabilitiesChanged(n, caps);
            cb.onLinkPropertiesChanged(n, links);
            handler.drain();
        }

        /** The daemon dies and a new one registers; returns the old one. */
        Daemon restartDaemon() throws Exception {
            Daemon next = new Daemon();
            ServiceManager.daemons.add(next);
            daemon.death.binderDied();
            handler.drain();
            handler.runDelayed();
            handler.drain();
            handler.awaitWork(5000);
            handler.drain();
            Daemon old = daemon;
            daemon = next;
            check(next.broker != null && next.broker != old.broker, "new broker binder");
            return old;
        }
    }

    static NetworkCapabilities caps(int transport, int capability, int subId) {
        NetworkCapabilities c =
                new NetworkCapabilities().addTransportType(transport).addCapability(capability);
        if (subId >= 0) {
            c.setNetworkSpecifier(
                    new TelephonyNetworkSpecifier.Builder().setSubscriptionId(subId).build());
        }
        return c;
    }

    static NetworkCapabilities cellular(int capability, int subId) {
        return caps(CELLULAR, capability, subId);
    }

    static LinkProperties links(String... addresses) throws Exception {
        LinkProperties lp = new LinkProperties().setMtu(1280);
        for (String a : addresses) {
            String[] parts = a.split("/");
            int flags = parts.length > 1 ? Integer.decode(parts[1]) : 0;
            lp.addLinkAddress(new LinkAddress(InetAddress.getByName(parts[0]), flags));
        }
        return lp;
    }

    static LinkProperties dual() throws Exception {
        return links("192.0.2.1", "2001:db8::1");
    }

    static void requestIs(NetworkRequest r, int capability, Integer subId) {
        check(r.hasCapability(capability) && r.capabilityCount() == 1, "only the one capability");
        check(r.hasTransport(CELLULAR) && r.transportCount() == 1, "cellular only");
        if (subId == null) {
            check(r.getNetworkSpecifier() == null, "no subscription in the request");
        } else {
            check(r.getNetworkSpecifier() instanceof TelephonyNetworkSpecifier
                            && ((TelephonyNetworkSpecifier) r.getNetworkSpecifier())
                                            .getSubscriptionId()
                                    == subId,
                    "the slot's subscription");
        }
    }

    static void reports(Daemon d, String... expected) {
        List<String> actual = d.take();
        check(actual.equals(Arrays.asList(expected)), "reports " + actual);
    }

    // Scenarios

    static void noSimEmergencyIsFiledAtOnceWithoutASubscription() throws Exception {
        Rig rig = new Rig();
        rig.bringUp(NO_SLOT, SOS, 1);
        check(rig.cm.requests.size() == 1, "filed with no active subscription at all");
        requestIs(rig.filed(0), EIMS_CAP, null);
        rig.up(rig.callback(0), new Network(100), cellular(EIMS_CAP, -1), dual());
        reports(rig.daemon, "up sos-1#1 100");
        PdnInfo info = rig.daemon.infos.get(0);
        check(Arrays.equals(info.ipv4Address, InetAddress.getByName("192.0.2.1").getAddress()),
                "IPv4 address");
        check(Arrays.equals(info.ipv6Address, InetAddress.getByName("2001:db8::1").getAddress()),
                "IPv6 address");
        check(info.mtu == 1280, "MTU");
    }

    static void simEmergencyWaitsForItsOwnSubscription() throws Exception {
        Rig rig = new Rig();
        rig.bringUp(0, SOS, 1);
        check(rig.cm.requests.isEmpty(), "waits: no subscription on slot 0");
        reports(rig.daemon); // Waiting is not a failure, as in stock.
        rig.subscriptions(1, 6);
        check(rig.cm.requests.isEmpty(), "never borrows the other SIM's subscription");
        rig.subscriptions(0, 5);
        check(rig.cm.requests.size() == 1, "filed once slot 0 is active");
        requestIs(rig.filed(0), EIMS_CAP, 5);
        rig.bringUp(1, SOS, 2);
        requestIs(rig.filed(1), EIMS_CAP, 6);
    }

    static void imsAndEmergencyOnOneSimNeverSatisfyEachOther() throws Exception {
        Rig rig = new Rig();
        SubscriptionManager.active.put(0, 5);
        rig.bringUp(0, IMS, 1);
        rig.bringUp(0, SOS, 2);
        check(rig.cm.requests.size() == 2, "two requests");
        requestIs(rig.filed(0), IMS_CAP, 5);
        requestIs(rig.filed(1), EIMS_CAP, 5);
        rig.up(rig.callback(1), new Network(200), cellular(IMS_CAP, 5), dual());
        rig.up(rig.callback(0), new Network(201), cellular(EIMS_CAP, 5), dual());
        reports(rig.daemon); // Each fails closed on the other's network.
        rig.callback(1).onCapabilitiesChanged(new Network(200), cellular(EIMS_CAP, 5));
        rig.handler.drain();
        reports(rig.daemon, "up sos0#2 200");
    }

    static void emergencyNeedsCellularEimsOnItsOwnSubscription() throws Exception {
        Rig rig = new Rig();
        SubscriptionManager.active.put(0, 5);
        rig.bringUp(0, SOS, 1);
        ConnectivityManager.NetworkCallback cb = rig.callback(0);
        Network n = new Network(300);
        rig.up(cb, n, caps(NetworkCapabilities.TRANSPORT_WIFI, EIMS_CAP, 5), dual());
        reports(rig.daemon);
        cb.onCapabilitiesChanged(n, cellular(EIMS_CAP, 6));
        rig.handler.drain();
        reports(rig.daemon);
        cb.onCapabilitiesChanged(n, cellular(EIMS_CAP, 5));
        rig.handler.drain();
        reports(rig.daemon, "up sos0#1 300");
        cb.onCapabilitiesChanged(n, cellular(IMS_CAP, 5));
        rig.handler.drain();
        reports(rig.daemon, "down sos0#1");
        check(rig.cm.unregistered.isEmpty(), "the request stays held");
    }

    static void lostEmergencyNetworkReportsDownAndKeepsTheRequest() throws Exception {
        Rig rig = new Rig();
        rig.bringUp(NO_SLOT, SOS, 1);
        ConnectivityManager.NetworkCallback cb = rig.callback(0);
        Network first = new Network(400);
        rig.up(cb, first, cellular(EIMS_CAP, -1), dual());
        cb.onLost(first);
        rig.handler.drain();
        reports(rig.daemon, "up sos-1#1 400", "down sos-1#1");
        check(rig.cm.unregistered.isEmpty() && rig.cm.requests.size() == 1, "still requested");
        rig.up(cb, new Network(401), cellular(EIMS_CAP, -1), links("192.0.2.9"));
        reports(rig.daemon, "up sos-1#1 401");
        cb.onLost(first); // Stale loss of the old network.
        cb.onCapabilitiesChanged(first, cellular(EIMS_CAP, -1));
        rig.handler.drain();
        reports(rig.daemon);
    }

    static void networkSwitchIsReportedOnceSettledWithoutADown() throws Exception {
        Rig rig = new Rig();
        rig.bringUp(NO_SLOT, SOS, 1);
        ConnectivityManager.NetworkCallback cb = rig.callback(0);
        rig.up(cb, new Network(500), cellular(EIMS_CAP, -1), dual());
        reports(rig.daemon, "up sos-1#1 500");
        Network next = new Network(501);
        cb.onAvailable(next);
        cb.onCapabilitiesChanged(next, cellular(EIMS_CAP, -1));
        rig.handler.drain();
        reports(rig.daemon); // Settling: links not known yet.
        cb.onLinkPropertiesChanged(next, links("192.0.2.2", "2001:db8::2"));
        rig.handler.drain();
        reports(rig.daemon, "up sos-1#1 501");
    }

    static void repeatedBringUpAnswersWithTheCurrentStateWithoutRefiling() throws Exception {
        Rig rig = new Rig();
        rig.bringUp(NO_SLOT, SOS, 1);
        rig.bringUp(NO_SLOT, SOS, 2);
        reports(rig.daemon);
        ConnectivityManager.NetworkCallback cb = rig.callback(0);
        Network n = new Network(600);
        rig.up(cb, n, cellular(EIMS_CAP, -1), dual());
        reports(rig.daemon, "up sos-1#2 600");
        rig.bringUp(NO_SLOT, SOS, 3);
        reports(rig.daemon, "up sos-1#3 600");
        cb.onLost(n);
        rig.handler.drain();
        reports(rig.daemon, "down sos-1#3");
        rig.bringUp(NO_SLOT, SOS, 4);
        reports(rig.daemon, "down sos-1#4");
        check(rig.cm.requests.size() == 1, "never filed twice");
    }

    static void releaseNeedsTheCurrentSerial() throws Exception {
        Rig rig = new Rig();
        rig.bringUp(NO_SLOT, SOS, 1);
        rig.bringUp(NO_SLOT, SOS, 2);
        rig.release(NO_SLOT, SOS, 1);
        check(rig.cm.unregistered.isEmpty(), "a stale release keeps the request");
        rig.release(NO_SLOT, SOS, 2);
        check(rig.cm.unregistered.equals(List.of(rig.callback(0))), "released");
        rig.bringUp(NO_SLOT, SOS, 3);
        check(rig.cm.requests.size() == 2, "a later bring-up files again");
        reports(rig.daemon);
    }

    static void unavailableEmergencyFailsAndCanBeRetried() throws Exception {
        Rig rig = new Rig();
        rig.bringUp(NO_SLOT, SOS, 1);
        rig.callback(0).onUnavailable();
        rig.handler.drain();
        reports(rig.daemon, "failed sos-1#1 " + PdnFailure.UNAVAILABLE);
        check(rig.cm.unregistered.isEmpty(), "ConnectivityService already removed it");
        rig.bringUp(NO_SLOT, SOS, 2);
        check(rig.cm.requests.size() == 2, "retry files again");
    }

    static void refusedRequestFailsAtOnceAndCanBeRetried() throws Exception {
        Rig rig = new Rig();
        rig.cm.refuseNext = new SecurityException();
        rig.bringUp(NO_SLOT, SOS, 1);
        reports(rig.daemon, "failed sos-1#1 " + PdnFailure.REQUEST_REJECTED);
        rig.bringUp(NO_SLOT, SOS, 2);
        check(rig.cm.requests.size() == 1, "retry filed");
        reports(rig.daemon);
    }

    static void invalidRequestsAreRefused() throws Exception {
        Rig rig = new Rig();
        rig.bringUp(NO_SLOT, IMS, 1); // Only emergency may go without a SIM.
        rig.bringUp(3, SOS, 2);
        rig.bringUp(0, 3, 3);
        rig.bringUp(0, SOS, 0);
        String invalid = " " + PdnFailure.INVALID_REQUEST;
        reports(rig.daemon, "failed ims-1#1" + invalid, "failed sos3#2" + invalid,
                "failed type30#3" + invalid, "failed sos0#0" + invalid);
        check(rig.cm.requests.isEmpty(), "nothing filed");
    }

    static void untrustedCallersCannotFileOrRelease() throws Exception {
        Rig rig = new Rig();
        Binder.callingUid = 10123;
        rig.bringUp(NO_SLOT, SOS, 1);
        check(rig.cm.requests.isEmpty(), "ignored");
        Binder.callingUid = DAEMON_UID;
        rig.bringUp(NO_SLOT, SOS, 2);
        Binder.callingUid = 10123;
        rig.release(NO_SLOT, SOS, 2);
        check(rig.cm.unregistered.isEmpty(), "release ignored");
        Binder.callingUid = DAEMON_UID;
    }

    static void daemonLossWithdrawsEveryRequestAndTheNextDaemonStartsClean() throws Exception {
        Rig rig = new Rig();
        SubscriptionManager.active.put(0, 5);
        rig.bringUp(0, IMS, 1);
        rig.bringUp(NO_SLOT, SOS, 2);
        rig.bringUp(0, SOS, 3);
        rig.up(rig.callback(2), new Network(700), cellular(EIMS_CAP, 5), dual());
        reports(rig.daemon, "up sos0#3 700");
        Daemon old = rig.restartDaemon();
        check(rig.cm.unregistered.size() == 3, "every request withdrawn");
        old.broker.bringUp(request(NO_SLOT, SOS, 4)); // The dead daemon's binder.
        rig.handler.drain();
        check(rig.cm.requests.size() == 3, "an old binder files nothing");
        rig.callback(2).onLinkPropertiesChanged(new Network(700), dual());
        rig.handler.drain();
        reports(old);
        reports(rig.daemon);
        rig.bringUp(NO_SLOT, SOS, 1);
        check(rig.cm.requests.size() == 4, "the new daemon's request is filed");
        requestIs(rig.filed(3), EIMS_CAP, null);
        rig.up(rig.callback(3), new Network(701), cellular(EIMS_CAP, -1), dual());
        reports(rig.daemon, "up sos-1#1 701");
    }

    static void subscriptionChangeWithdrawsTheSimEmergencyButNotTheNoSimOne() throws Exception {
        Rig rig = new Rig();
        SubscriptionManager.active.put(0, 5);
        rig.bringUp(0, SOS, 1);
        rig.bringUp(NO_SLOT, SOS, 2);
        rig.subscriptions(0, 6);
        reports(rig.daemon, "failed sos0#1 " + PdnFailure.UNAVAILABLE);
        check(rig.cm.unregistered.equals(List.of(rig.callback(0))), "only slot 0 withdrawn");
        rig.bringUp(0, SOS, 3);
        requestIs(rig.filed(2), EIMS_CAP, 6);
        rig.subscriptions(0, null); // SIM removed.
        reports(rig.daemon, "failed sos0#3 " + PdnFailure.UNAVAILABLE);
        rig.up(rig.callback(1), new Network(800), cellular(EIMS_CAP, -1), dual());
        reports(rig.daemon, "up sos-1#2 800");
    }

    static void onlyPreferredAddressesAreReported() throws Exception {
        Rig rig = new Rig();
        rig.bringUp(NO_SLOT, SOS, 1);
        rig.up(rig.callback(0), new Network(900), cellular(EIMS_CAP, -1),
                links("2001:db8::bad/0x40", "fe80::1", "2001:db8::dead/0x20", "2001:db8::2",
                        "192.0.2.7"));
        reports(rig.daemon, "up sos-1#1 900");
        PdnInfo info = rig.daemon.infos.get(0);
        check(Arrays.equals(info.ipv6Address, InetAddress.getByName("2001:db8::2").getAddress()),
                "preferred global IPv6 only");
        check(Arrays.equals(info.ipv4Address, InetAddress.getByName("192.0.2.7").getAddress()),
                "IPv4");
    }

    static void blockedStatusDoesNotAffectTheEmergencyBearer() throws Exception {
        Rig rig = new Rig();
        rig.bringUp(NO_SLOT, SOS, 1);
        ConnectivityManager.NetworkCallback cb = rig.callback(0);
        Network n = new Network(1000);
        rig.up(cb, n, cellular(EIMS_CAP, -1), dual());
        cb.onBlockedStatusChanged(n, true); // For example VPN lockdown for the broker's UID.
        rig.handler.drain();
        reports(rig.daemon, "up sos-1#1 1000");
        check(rig.cm.unregistered.isEmpty(), "still held");
    }

    static void undeliveredReportIsSentAgainWithTheCurrentState() throws Exception {
        Rig rig = new Rig();
        rig.bringUp(NO_SLOT, SOS, 1);
        ConnectivityManager.NetworkCallback cb = rig.callback(0);
        Network first = new Network(1100);
        rig.daemon.failNext = 1;
        rig.up(cb, first, cellular(EIMS_CAP, -1), dual());
        reports(rig.daemon);
        rig.handler.runDelayed();
        rig.handler.drain();
        reports(rig.daemon, "up sos-1#1 1100");
        rig.handler.runDelayed();
        rig.handler.drain();
        reports(rig.daemon); // Delivered: nothing left to retry.
        rig.daemon.failNext = 1;
        cb.onLost(first);
        rig.handler.drain();
        reports(rig.daemon);
        rig.up(cb, new Network(1101), cellular(EIMS_CAP, -1), dual());
        reports(rig.daemon, "up sos-1#1 1101");
        rig.handler.runDelayed();
        rig.handler.drain();
        reports(rig.daemon); // The current state replaced the lost down.
        rig.daemon.failNext = 1;
        cb.onLost(new Network(1101));
        rig.handler.drain();
        rig.release(NO_SLOT, SOS, 1);
        rig.handler.runDelayed();
        rig.handler.drain();
        reports(rig.daemon); // Released: no retry.
    }

    static void undeliveredFailureIsSentAgainButNotToANewDaemon() throws Exception {
        Rig rig = new Rig();
        rig.bringUp(NO_SLOT, SOS, 1);
        rig.daemon.failNext = 1;
        rig.callback(0).onUnavailable();
        rig.handler.drain();
        reports(rig.daemon);
        rig.handler.runDelayed();
        rig.handler.drain();
        reports(rig.daemon, "failed sos-1#1 " + PdnFailure.UNAVAILABLE);
        rig.bringUp(NO_SLOT, SOS, 2);
        rig.daemon.failNext = 1;
        rig.callback(1).onUnavailable();
        rig.handler.drain();
        Daemon old = rig.restartDaemon();
        rig.handler.runDelayed();
        rig.handler.drain();
        reports(old);
        reports(rig.daemon);
    }

    static void check(boolean value, String what) {
        if (!value) throw new AssertionError(what);
    }

    interface Scenario {
        void run() throws Exception;
    }

    public static void main(String[] args) throws Exception {
        Scenario[] scenarios = {
            EmergencyBrokerTest::noSimEmergencyIsFiledAtOnceWithoutASubscription,
            EmergencyBrokerTest::simEmergencyWaitsForItsOwnSubscription,
            EmergencyBrokerTest::imsAndEmergencyOnOneSimNeverSatisfyEachOther,
            EmergencyBrokerTest::emergencyNeedsCellularEimsOnItsOwnSubscription,
            EmergencyBrokerTest::lostEmergencyNetworkReportsDownAndKeepsTheRequest,
            EmergencyBrokerTest::networkSwitchIsReportedOnceSettledWithoutADown,
            EmergencyBrokerTest::repeatedBringUpAnswersWithTheCurrentStateWithoutRefiling,
            EmergencyBrokerTest::releaseNeedsTheCurrentSerial,
            EmergencyBrokerTest::unavailableEmergencyFailsAndCanBeRetried,
            EmergencyBrokerTest::refusedRequestFailsAtOnceAndCanBeRetried,
            EmergencyBrokerTest::invalidRequestsAreRefused,
            EmergencyBrokerTest::untrustedCallersCannotFileOrRelease,
            EmergencyBrokerTest::daemonLossWithdrawsEveryRequestAndTheNextDaemonStartsClean,
            EmergencyBrokerTest::subscriptionChangeWithdrawsTheSimEmergencyButNotTheNoSimOne,
            EmergencyBrokerTest::onlyPreferredAddressesAreReported,
            EmergencyBrokerTest::blockedStatusDoesNotAffectTheEmergencyBearer,
            EmergencyBrokerTest::undeliveredReportIsSentAgainWithTheCurrentState,
            EmergencyBrokerTest::undeliveredFailureIsSentAgainButNotToANewDaemon,
        };
        for (Scenario s : scenarios) {
            s.run();
        }
        System.out.println(
                "Broker emergency simulations: " + scenarios.length + " scenarios passed");
    }
}
