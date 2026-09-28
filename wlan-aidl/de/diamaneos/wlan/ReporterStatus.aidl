// SPDX-License-Identifier: Apache-2.0
package de.diamaneos.wlan;
/** Protocol progress only. Never add network or subscriber identifiers here. */
parcelable ReporterStatus {
    int primaryStage = -1;
    int primaryOperation;
    int primaryError;
    int secondaryStage = -1;
    int secondaryOperation;
    int secondaryError;
}
