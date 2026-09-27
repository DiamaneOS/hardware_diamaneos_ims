/* SPDX-License-Identifier: Apache-2.0
 * Copyright 2026 The DiamaneOS Project
 */
package org.diamaneos.emergencylocation;

import android.app.Application;
import android.content.res.XmlResourceParser;
import android.location.Location;
import android.location.LocationListener;
import android.location.LocationManager;
import android.location.LocationRequest;
import android.os.Handler;
import android.os.HandlerThread;
import android.os.PowerManager;
import android.os.SystemClock;
import android.telephony.SmsManager;
import android.telephony.SubscriptionManager;
import android.telephony.TelephonyManager;
import android.util.Log;

import org.xmlpull.v1.XmlPullParser;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.RejectedExecutionException;
import java.util.concurrent.ThreadPoolExecutor;
import java.util.concurrent.TimeUnit;

/** Separate UID from IMS: no QRTR, daemon access, call placement or call routing. */
public final class EmergencyLocationApp extends Application {
    private static final long DELIVERY_BUDGET_MS = 30000;
    private Handler handler;
    private LocationManager locations;
    private TelephonyManager telephony;
    private AmlProfiles profiles = new AmlProfiles(List.of());
    private final List<Collection> active = new ArrayList<>();
    private final ThreadPoolExecutor delivery =
            new ThreadPoolExecutor(
                    1,
                    1,
                    30,
                    TimeUnit.SECONDS,
                    new ArrayBlockingQueue<>(2),
                    r -> {
                        Thread t = new Thread(r, "AmlDelivery");
                        t.setDaemon(true);
                        return t;
                    },
                    new ThreadPoolExecutor.AbortPolicy());

    @Override
    public void onCreate() {
        super.onCreate();
        if (!getSystemService(android.os.UserManager.class).isSystemUser()) return;
        HandlerThread thread = new HandlerThread("AmlEvents");
        thread.start();
        handler = new Handler(thread.getLooper());
        locations = createAttributionContext("emergency").getSystemService(LocationManager.class);
        telephony = getSystemService(TelephonyManager.class);
        handler.post(this::loadProfiles);
    }

    void emergencyCommunication(
            String number,
            int subId,
            int phoneId,
            String source,
            long eventUtc,
            long eventElapsed) {
        if (handler != null)
            handler.post(
                    () -> {
                        if (TrustedEvent.fresh(eventElapsed, SystemClock.elapsedRealtime()))
                            begin(number, subId, phoneId, source, eventUtc);
                    });
    }

    private void loadProfiles() {
        List<AmlProfile> candidate = new ArrayList<>();
        try (XmlResourceParser x = getResources().getXml(R.xml.aml_profiles)) {
            while (x.next() != XmlPullParser.END_DOCUMENT) {
                if (x.getEventType() != XmlPullParser.START_TAG) continue;
                if (x.getDepth() == 1
                        && x.getName().equals("aml-profiles")
                        && x.getAttributeCount() == 0) continue;
                if (x.getDepth() != 2
                        || !x.getName().equals("profile")
                        || candidate.size() >= AmlProfiles.MAX_PROFILES)
                    throw new IllegalArgumentException("invalid profile structure");
                Map<String, String> attrs = new HashMap<>();
                for (int i = 0; i < x.getAttributeCount(); i++) {
                    String namespace = x.getAttributeNamespace(i);
                    if (namespace != null && !namespace.isEmpty())
                        throw new IllegalArgumentException("unexpected attribute namespace");
                    attrs.put(x.getAttributeName(i), x.getAttributeValue(i));
                }
                candidate.add(AmlProfiles.parse(attrs));
            }
            profiles = new AmlProfiles(candidate);
        } catch (Exception e) {
            profiles = new AmlProfiles(List.of());
            Log.e("EmergencyLocation", "Invalid AML route configuration");
        }
    }

    private void begin(String number, int subId, int phoneId, String source, long eventUtc) {
        // Bound independent work; never block, reject or redial a voice call.
        if (phoneId < 0 || phoneId >= telephony.getActiveModemCount() || eventUtc <= 0) return;
        if (active.stream()
                .anyMatch(
                        c ->
                                c.phoneId == phoneId
                                        && c.subId == subId
                                        && c.source.equals(source)
                                        && c.identity.number.equals(number))) return;
        if (active.size() >= 2) {
            Log.w("EmergencyLocation", "Location capacity reached");
            return;
        }
        try {
            boolean hasSim = SubscriptionManager.isUsableSubscriptionId(subId);
            if (hasSim && SubscriptionManager.getSlotIndex(subId) != phoneId) return;
            // Slot-specific network country can be available even without a SIM.
            // Never use the locale, home SIM country or the other subscription.
            String country = telephony.getNetworkCountryIso(phoneId);
            AmlProfile profile =
                    profiles.select(country, number, source, System.currentTimeMillis());
            if (profile == null || (!hasSim && !profile.allowNoSimHttps)) return;
            TelephonyManager tm = hasSim ? telephony.createForSubscriptionId(subId) : null;
            String home = tm == null ? "" : value(tm.getSimOperator());
            String network = tm == null ? "" : value(tm.getNetworkOperator());
            String fullImsi =
                    tm != null && profile.https != null && profile.httpsImsi.equals("full")
                            ? value(tm.getSubscriberId())
                            : "";
            AmlMessage.Identity id =
                    new AmlMessage.Identity(
                            mcc(home),
                            mnc(home),
                            fullImsi,
                            telephony.getImei(phoneId),
                            mcc(network),
                            mnc(network),
                            number,
                            profile.allowMissingMetadata);
            if (!profile.acceptsIdentity(id, hasSim)) return;
            // Unknown home country is not evidence of domestic SMS routing.
            Collection c =
                    new Collection(
                            profile,
                            id,
                            subId,
                            phoneId,
                            source,
                            eventUtc,
                            AmlProfiles.smsAllowed(
                                    profile, id, hasSim, tm == null ? "" : tm.getSimCountryIso()));
            active.add(c);
            c.start();
        } catch (RuntimeException e) {
            Log.w("EmergencyLocation", "Location setup unavailable");
        }
    }

    private static String value(String s) {
        return s == null ? "" : s;
    }

    private static String mcc(String plmn) {
        if (plmn.isEmpty()) return "";
        if (!plmn.matches("[0-9]{5,6}")) throw new IllegalArgumentException();
        return plmn.substring(0, 3);
    }

    private static String mnc(String plmn) {
        return plmn.isEmpty() ? "" : plmn.substring(3);
    }

    private final class Collection implements LocationListener {
        final AmlProfile profile;
        final AmlMessage.Identity identity;
        final int subId, phoneId;
        final String source;
        final long eventUtc;
        final AmlSession session;
        final PowerManager.WakeLock wake;
        final boolean smsAllowed;
        boolean closed;

        Collection(
                AmlProfile p,
                AmlMessage.Identity id,
                int sid,
                int pid,
                String src,
                long utc,
                boolean sms) {
            profile = p;
            identity = id;
            subId = sid;
            phoneId = pid;
            source = src;
            eventUtc = utc;
            smsAllowed = sms;
            session = new AmlSession(SystemClock.elapsedRealtime(), p.timeoutMs, p.maxAgeMs);
            wake =
                    getSystemService(PowerManager.class)
                            .newWakeLock(
                                    PowerManager.PARTIAL_WAKE_LOCK, "DiamaneOS:EmergencyLocation");
            wake.setReferenceCounted(false);
        }

        void start() {
            handler.postDelayed(this::finish, profile.timeoutMs);
            try {
                wake.acquire(profile.timeoutMs + DELIVERY_BUDGET_MS + 2000);
                LocationRequest request =
                        new LocationRequest.Builder(1000)
                                .setQuality(LocationRequest.QUALITY_HIGH_ACCURACY)
                                .setDurationMillis(profile.timeoutMs)
                                .setLocationSettingsIgnored(true)
                                .build();
                locations.requestLocationUpdates(
                        LocationManager.GPS_PROVIDER, request, handler::post, this);
            } catch (RuntimeException e) {
                Log.w("EmergencyLocation", "GNSS unavailable; will report positioning failure");
            }
        }

        @Override
        public void onLocationChanged(Location l) {
            if (closed || !l.hasAccuracy()) return;
            try {
                session.offer(
                        new AmlMessage.Fix(
                                l.getLatitude(),
                                l.getLongitude(),
                                l.getAccuracy(),
                                l.getElapsedRealtimeNanos() / 1000000,
                                l.getTime(),
                                "gps"),
                        SystemClock.elapsedRealtime(),
                        l.isMock());
            } catch (IllegalArgumentException ignored) {
            }
        }

        void cleanup() {
            if (wake.isHeld()) wake.release();
            active.remove(this);
        }

        void finish() {
            if (closed) return;
            long now = SystemClock.elapsedRealtime();
            if (now < session.deadline()) {
                handler.postDelayed(this::finish, session.deadline() - now);
                return;
            }
            closed = true;
            try {
                locations.removeUpdates(this);
            } catch (RuntimeException ignored) {
            }
            AmlMessage.Fix fix = session.finish(now);
            long expires = now + DELIVERY_BUDGET_MS;
            // A stalled network cannot hold a wakelock or occupy both collection slots forever.
            Runnable job =
                    () -> {
                        try {
                            deliver(this, fix, expires);
                        } catch (RuntimeException e) {
                            Log.w("EmergencyLocation", "Delivery unavailable");
                        } finally {
                            handler.post(this::cleanup);
                        }
                    };
            handler.postDelayed(
                    () -> {
                        delivery.remove(job);
                        cleanup();
                    },
                    DELIVERY_BUDGET_MS);
            try {
                delivery.execute(job);
            } catch (RejectedExecutionException e) {
                cleanup();
                Log.w("EmergencyLocation", "Location delivery capacity reached");
            }
        }
    }

    private boolean deliverable(Collection c, long expires) {
        return SystemClock.elapsedRealtime() < expires
                && c.profile.matches(
                        telephony.getNetworkCountryIso(c.phoneId),
                        c.identity.number,
                        System.currentTimeMillis());
    }

    private void deliver(Collection c, AmlMessage.Fix fix, long expires) {
        if (!deliverable(c, expires)) return;
        AmlProfile p = c.profile;
        // SMS does not wait behind DNS/TLS failure. No SMS without the same active subscription.
        if (c.smsAllowed
                && !p.sms.isEmpty()
                && SubscriptionManager.getSlotIndex(c.subId) == c.phoneId) {
            try {
                TelephonyManager tm = telephony.createForSubscriptionId(c.subId);
                if (p.allowRoamingSms || p.country.equals(tm.getSimCountryIso())) {
                    byte[] payload =
                            SmsPayload.pack(
                                    AmlMessage.sms(
                                            c.identity,
                                            fix,
                                            System.currentTimeMillis(),
                                            p.https != null),
                                    p.legacySmsPacking);
                    try {
                        getSystemService(SmsManager.class)
                                .createForSubscriptionId(c.subId)
                                .sendDataMessage(
                                        p.sms, null, (short) p.smsPort, payload, null, null);
                        Log.i("EmergencyLocation", "SMS submitted; delivery unconfirmed");
                    } finally {
                        Arrays.fill(payload, (byte) 0);
                    }
                }
            } catch (RuntimeException e) {
                Log.w("EmergencyLocation", "SMS submission failed");
            }
        }
        if (p.https != null && deliverable(c, expires)) {
            byte[] body =
                    AmlMessage.https(
                            c.identity,
                            fix,
                            c.eventUtc,
                            System.currentTimeMillis(),
                            c.source,
                            p.httpsImsi.equals("full"));
            try {
                HttpsSender.Result result =
                        new HttpsSender().send(p.https, body, () -> deliverable(c, expires));
                Log.i("EmergencyLocation", "HTTPS transport result: " + result.name());
            } finally {
                Arrays.fill(body, (byte) 0);
            }
        }
    }
}
