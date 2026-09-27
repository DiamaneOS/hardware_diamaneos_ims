// SPDX-License-Identifier: Apache-2.0
package org.diamaneos.wlan;
import org.diamaneos.wlan.Snapshot;
interface IReporter {
    long registerObserver(IBinder lifetime);
    void observe(long generation, long sequence, in Snapshot snapshot);
}
