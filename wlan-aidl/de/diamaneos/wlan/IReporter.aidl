// SPDX-License-Identifier: Apache-2.0
package de.diamaneos.wlan;
import de.diamaneos.wlan.Snapshot;
import de.diamaneos.wlan.ReporterStatus;
interface IReporter {
    long registerObserver(IBinder lifetime);
    void observe(long generation, long sequence, in Snapshot snapshot);
    ReporterStatus getStatus(long generation);
}
