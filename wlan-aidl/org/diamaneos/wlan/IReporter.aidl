// SPDX-License-Identifier: Apache-2.0
package org.diamaneos.wlan;
import org.diamaneos.wlan.Snapshot;
import org.diamaneos.wlan.ReporterStatus;
interface IReporter {
    long registerObserver(IBinder lifetime);
    void observe(long generation, long sequence, in Snapshot snapshot);
    ReporterStatus getStatus(long generation);
}
