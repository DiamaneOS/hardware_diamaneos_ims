// SPDX-License-Identifier: Apache-2.0
package de.diamaneos.wlan;
/** Minimal protocol progress/errors; no network, subscriber or measurement data. */
parcelable ReporterStatus {
    int primaryStage = -1;
    int primaryOperation;
    int primaryError;
    int secondaryStage = -1;
    int secondaryOperation;
    int secondaryError;
    int primaryKeepaliveError;
    int secondaryKeepaliveError;
    int primaryProfileError;
    int secondaryProfileError;
}
