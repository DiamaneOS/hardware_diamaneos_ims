/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright 2026 The DiamaneOS Project
 */

package vendor.diamaneos.hardware.imsdcm;

import vendor.diamaneos.hardware.imsdcm.PdnType;

/** One PDN the daemon asks the broker for. The key is (slot, type). */
@VintfStability
parcelable PdnRequest {
    /** The modem gave no SIM slot: the request carries no subscription. */
    const int SLOT_UNSPECIFIED = -1;

    /** Logical SIM slot index (0-based), or SLOT_UNSPECIFIED. */
    int slot = SLOT_UNSPECIFIED;

    PdnType type = PdnType.IMS;

    /**
     * Chosen by the daemon for each bring-up and echoed in every report about
     * it. Any value; the broker only compares it for equality.
     */
    int serial;
}
