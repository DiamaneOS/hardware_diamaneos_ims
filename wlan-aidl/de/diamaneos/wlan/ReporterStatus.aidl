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
    // Private userdebug observation only. Zero/-1 on a user build.
    boolean diagnosticHeadersEnabled;
    // True only with the private registration cfg AND immutable debug-build flag.
    boolean diagnosticRegistrationEnabled;
    long primaryResponseHeaders;
    long primaryIndicationHeaders;
    int primaryLastIndication = -1;
    long secondaryResponseHeaders;
    long secondaryIndicationHeaders;
    int secondaryLastIndication = -1;
    // Fixed producer bounds:64 bins for message IDs0x20..0x5f; empty when disabled.
    int[] primaryIndicationHistogram;
    int[] secondaryIndicationHistogram;
    // Failure-completion diagnostics only; no timer, address or port data.
    long primaryKeepaliveFailureSent;
    long primaryKeepaliveFailureAcknowledged;
    int primaryKeepaliveFailureError;
    long secondaryKeepaliveFailureSent;
    long secondaryKeepaliveFailureAcknowledged;
    int secondaryKeepaliveFailureError;
    long primaryKeepaliveFailureDropped;
    long secondaryKeepaliveFailureDropped;
    // Private compile/debug gated observations only. Fixed40 bins for types4..43.
    // These are NOT active/accepted profiles; identities and repeats collapse.
    int[] primaryProfileInitializationCounts;
    int[] secondaryProfileInitializationCounts;
    int[] primaryProfileSelectionCounts;
    int[] secondaryProfileSelectionCounts;
    long primaryProfileSelectionMessages;
    long secondaryProfileSelectionMessages;
    long primaryProfileRejectedMessages;
    long secondaryProfileRejectedMessages;
    long primaryProfileUnmappedSelections;
    long secondaryProfileUnmappedSelections;
}
