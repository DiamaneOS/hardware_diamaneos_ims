// SPDX-License-Identifier: Apache-2.0
// Host-only fixture for the broker simulations. Never part of an Android module.
package android.os;

import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.List;

/** A looper the simulation runs by hand; delayed work runs only when asked. */
public class Handler {
    private final ArrayDeque<Runnable> queue = new ArrayDeque<>();
    private final List<Delayed> delayed = new ArrayList<>();

    private static final class Delayed {
        final Runnable work;
        final Object token;

        Delayed(Runnable work, Object token) {
            this.work = work;
            this.token = token;
        }
    }

    public synchronized boolean post(Runnable work) {
        queue.add(work);
        notifyAll();
        return true;
    }

    public synchronized boolean postDelayed(Runnable work, long delayMillis) {
        return postDelayed(work, null, delayMillis);
    }

    public synchronized boolean postDelayed(Runnable work, Object token, long delayMillis) {
        delayed.add(new Delayed(work, token));
        return true;
    }

    /** Removes pending posts of {@code work}, as the platform does. */
    public synchronized void removeCallbacks(Runnable work) {
        queue.removeIf(r -> r == work);
        delayed.removeIf(d -> d.work == work);
    }

    /** Removes delayed work posted with {@code token}; null removes everything. */
    public synchronized void removeCallbacksAndMessages(Object token) {
        if (token == null) {
            queue.clear();
        }
        delayed.removeIf(d -> token == null || d.token == token);
    }

    /** Runs queued work, including work it posts, until the queue is empty. */
    public void drain() {
        while (true) {
            Runnable next;
            synchronized (this) {
                next = queue.poll();
            }
            if (next == null) return;
            next.run();
        }
    }

    /** Waits for work posted by another thread (the service lookup). */
    public synchronized void awaitWork(long timeoutMillis) throws InterruptedException {
        long end = System.currentTimeMillis() + timeoutMillis;
        while (queue.isEmpty()) {
            long left = end - System.currentTimeMillis();
            if (left <= 0) throw new AssertionError("no work posted");
            wait(left);
        }
    }

    public synchronized void runDelayed() {
        for (Delayed d : delayed) {
            queue.add(d.work);
        }
        delayed.clear();
    }
}
