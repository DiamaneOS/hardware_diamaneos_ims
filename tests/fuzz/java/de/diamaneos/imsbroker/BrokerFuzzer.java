/* SPDX-License-Identifier: Apache-2.0
 * Copyright 2026 The DiamaneOS Project
 */
package de.diamaneos.imsbroker;

import android.net.ConnectivityManager;
import android.net.LinkAddress;
import android.net.LinkProperties;
import android.net.Network;
import android.net.NetworkCapabilities;
import android.net.NetworkRequest;
import android.net.TelephonyNetworkSpecifier;
import android.os.Binder;

import vendor.diamaneos.hardware.imsdcm.PdnInfo;

import java.net.Inet4Address;
import java.net.InetAddress;
import java.net.UnknownHostException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.Random;

/**
 * Fuzz target for the IMS broker's inputs from other processes: the daemon's bring-up and release
 * calls (any caller UID, any request fields) and ConnectivityService callbacks (any network,
 * capabilities, link addresses, address flags and MTU). Runs the real broker classes against the
 * host fixtures in tests/stubs through EmergencyBrokerTest.Rig. Made-up values only.
 *
 * <p>{@link #fuzzerTestOneInput(byte[])} is the entry point a coverage-guided JVM fuzzer such as
 * Jazzer calls. {@link #main} replays files given as arguments, or runs seeded random inputs.
 *
 * <p>Checks: no exception escapes the broker, every report to the daemon carries 0 or 4 and 0 or
 * 16 address bytes and a non-negative MTU, and every filed request is cellular IMS or EIMS only.
 */
public final class BrokerFuzzer {
    private static final int DAEMON_UID = 2990;

    private BrokerFuzzer() {}

    /** Reads typed values from the input. An exhausted input reads as zeros. */
    private static final class Bytes {
        private final byte[] b;
        private int pos;

        Bytes(byte[] b) {
            this.b = b;
        }

        int remaining() {
            return b.length - pos;
        }

        int u8() {
            return pos < b.length ? b[pos++] & 0xff : 0;
        }

        boolean bool() {
            return (u8() & 1) != 0;
        }

        int anyInt() {
            return u8() | u8() << 8 | u8() << 16 | u8() << 24;
        }

        /** A value in lo..hi (inclusive, at most 256 values). */
        int in(int lo, int hi) {
            return lo + u8() % (hi - lo + 1);
        }

        byte[] take(int n) {
            byte[] out = new byte[n];
            for (int i = 0; i < n; i++) out[i] = (byte) u8();
            return out;
        }
    }

    private static int slot(Bytes in) {
        return in.bool() ? in.in(-2, 3) : in.anyInt();
    }

    private static int type(Bytes in) {
        return in.bool() ? in.in(0, 3) : in.anyInt();
    }

    private static NetworkCapabilities capabilities(Bytes in) {
        NetworkCapabilities c = new NetworkCapabilities();
        int[] known = {NetworkCapabilities.NET_CAPABILITY_IMS, NetworkCapabilities.NET_CAPABILITY_EIMS, 0};
        for (int i = in.in(0, 3); i > 0; i--) c.addTransportType(in.in(0, 2));
        for (int i = in.in(0, 3); i > 0; i--) c.addCapability(known[in.in(0, 2)]);
        if (in.bool()) {
            c.setNetworkSpecifier(
                    new TelephonyNetworkSpecifier.Builder().setSubscriptionId(in.in(-1, 3)).build());
        }
        return c;
    }

    private static LinkProperties links(Bytes in) throws UnknownHostException {
        LinkProperties lp = new LinkProperties().setMtu(in.bool() ? in.in(0, 3) * 1280 : in.anyInt());
        for (int i = in.in(0, 4); i > 0; i--) {
            InetAddress address = InetAddress.getByAddress(in.take(in.bool() ? 4 : 16));
            lp.addLinkAddress(new LinkAddress(address, in.bool() ? 0 : in.anyInt()));
        }
        return lp;
    }

    private static void checkReports(EmergencyBrokerTest.Daemon daemon) throws UnknownHostException {
        for (PdnInfo info : daemon.infos) {
            check(info.ipv4Address.length == 0 || info.ipv4Address.length == 4, "IPv4 width");
            check(info.ipv6Address.length == 0 || info.ipv6Address.length == 16, "IPv6 width");
            check(info.mtu >= 0, "MTU");
            for (byte[] raw : new byte[][] {info.ipv4Address, info.ipv6Address}) {
                if (raw.length == 0) continue;
                InetAddress a = InetAddress.getByAddress(raw);
                check(!a.isLinkLocalAddress()
                                && !a.isLoopbackAddress()
                                && !a.isAnyLocalAddress()
                                && !a.isMulticastAddress(),
                        "reportable address");
                check((raw == info.ipv4Address) == (a instanceof Inet4Address), "address family");
            }
        }
        daemon.infos.clear();
        daemon.reports.clear();
    }

    private static void checkRequests(ConnectivityManager cm) {
        for (NetworkRequest r : cm.requests) {
            check(r.transportCount() == 1 && r.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR),
                    "cellular only");
            check(r.capabilityCount() == 1
                            && (r.hasCapability(NetworkCapabilities.NET_CAPABILITY_IMS)
                                    || r.hasCapability(NetworkCapabilities.NET_CAPABILITY_EIMS)),
                    "IMS or EIMS only");
        }
    }

    private static void check(boolean value, String what) {
        if (!value) throw new AssertionError(what);
    }

    public static void fuzzerTestOneInput(byte[] input) throws Exception {
        Bytes in = new Bytes(input);
        EmergencyBrokerTest.Rig rig = new EmergencyBrokerTest.Rig();
        Network[] networks = {new Network(100), new Network(101), new Network(0)};
        for (int op = 0; op < 64 && in.remaining() > 0; op++) {
            Binder.callingUid = in.in(0, 9) == 0 ? in.anyInt() : DAEMON_UID;
            switch (in.in(0, 12)) {
                case 0:
                    rig.bringUp(slot(in), type(in), in.anyInt());
                    break;
                case 1:
                    rig.release(slot(in), type(in), in.anyInt());
                    break;
                case 2:
                    rig.daemon.broker.bringUp(null);
                    rig.daemon.broker.release(null);
                    rig.handler.drain();
                    break;
                case 3: case 4: case 5: case 6: case 7: {
                    if (rig.cm.callbacks.isEmpty()) break;
                    ConnectivityManager.NetworkCallback cb =
                            rig.cm.callbacks.get(in.u8() % rig.cm.callbacks.size());
                    Network n = networks[in.in(0, networks.length - 1)];
                    switch (in.in(0, 5)) {
                        case 0: cb.onAvailable(n); break;
                        case 1: cb.onCapabilitiesChanged(n, capabilities(in)); break;
                        case 2: cb.onLinkPropertiesChanged(n, links(in)); break;
                        case 3: cb.onLost(n); break;
                        case 4: cb.onUnavailable(); break;
                        default: cb.onBlockedStatusChanged(n, in.bool()); break;
                    }
                    rig.handler.drain();
                    break;
                }
                case 8:
                    rig.subscriptions(in.in(-1, 3), in.bool() ? null : in.in(-1, 4));
                    break;
                case 9:
                    rig.daemon.failNext = in.in(0, 3);
                    break;
                case 10:
                    rig.handler.runDelayed();
                    rig.handler.drain();
                    break;
                case 11:
                    checkReports(rig.daemon);
                    Binder.callingUid = DAEMON_UID;
                    rig.restartDaemon();
                    break;
                default:
                    rig.cm.refuseNext = new SecurityException();
                    break;
            }
            checkReports(rig.daemon);
            checkRequests(rig.cm);
        }
    }

    /** Replays the given files, or runs ITERATIONS seeded random inputs (env SEED, ITERATIONS). */
    public static void main(String[] args) throws Exception {
        if (args.length > 0) {
            for (String file : args) fuzzerTestOneInput(Files.readAllBytes(Paths.get(file)));
            System.out.println("replayed " + args.length + " inputs");
            return;
        }
        long seed = Long.parseLong(System.getenv().getOrDefault("SEED", "1"));
        long iterations = Long.parseLong(System.getenv().getOrDefault("ITERATIONS", "10000"));
        Random random = new Random(seed);
        for (long i = 0; i < iterations; i++) {
            byte[] input = new byte[random.nextInt(512)];
            random.nextBytes(input);
            try {
                fuzzerTestOneInput(input);
            } catch (Throwable t) {
                Path crash = Paths.get("broker-crash-" + seed + "-" + i);
                Files.write(crash, input);
                System.out.println("failure on input " + i + ", saved to " + crash);
                throw t;
            }
        }
        System.out.println("seed " + seed + ": " + iterations + " random inputs passed");
    }
}
