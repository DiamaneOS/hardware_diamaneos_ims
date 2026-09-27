/* SPDX-License-Identifier: Apache-2.0
 * Copyright 2026 The DiamaneOS Project
 */
package org.diamaneos.emergencylocation;

/** Bounded, monotonic-time location collection. No telephony or transport control. */
final class AmlSession {
    private final long start, deadline, maxAge;
    private AmlMessage.Fix best;
    private boolean finished;

    AmlSession(long now, long timeoutMs, long maxAgeMs) {
        if (now < 0
                || timeoutMs < 1000
                || timeoutMs > 120000
                || maxAgeMs < 0
                || maxAgeMs > timeoutMs
                || now > Long.MAX_VALUE - timeoutMs)
            throw new IllegalArgumentException("invalid timing policy");
        start = now;
        deadline = now + timeoutMs;
        maxAge = maxAgeMs;
    }

    boolean offer(AmlMessage.Fix f, long now) {
        return offer(f, now, false);
    }

    boolean offer(AmlMessage.Fix f, long now, boolean mock) {
        if (mock
                || finished
                || now < start
                || now > deadline
                || f == null
                || f.elapsedMs < start
                || f.elapsedMs > now
                || now - f.elapsedMs > maxAge) return false;
        if (best == null || now - best.elapsedMs > maxAge || f.accuracy < best.accuracy) {
            best = f;
            return true;
        }
        return false;
    }

    AmlMessage.Fix finish(long now) {
        if (finished || now < deadline)
            throw new IllegalStateException("not due or already finished");
        finished = true;
        AmlMessage.Fix result = best != null && now - best.elapsedMs <= maxAge ? best : null;
        best = null;
        return result;
    }

    void cancel() {
        finished = true;
        best = null;
    }

    long deadline() {
        return deadline;
    }
}
