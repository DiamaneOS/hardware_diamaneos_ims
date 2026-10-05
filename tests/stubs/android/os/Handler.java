// SPDX-License-Identifier: Apache-2.0
// Host-only fixture for the broker simulations. Never part of an Android module.
package android.os;

import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.List;

/** A looper the simulation runs by hand; delayed work runs only when asked. */
public class Handler {
    private final ArrayDeque<Runnable> queue = new ArrayDeque<>();
    private final List<Runnable> delayed = new ArrayList<>();

    public synchronized boolean post(Runnable work) {
        queue.add(work);
        notifyAll();
        return true;
    }

    public synchronized boolean postDelayed(Runnable work, long delayMillis) {
        delayed.add(work);
        return true;
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
        queue.addAll(delayed);
        delayed.clear();
    }
}
