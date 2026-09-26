/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright 2026 The DiamaneOS Project
 */

package vendor.diamaneos.hardware.imsdcm;

/** The Android network capability the broker requests. There is no other. */
@VintfStability
@Backing(type="int")
enum PdnType {
    /** NET_CAPABILITY_IMS on cellular (never with MMTEL). */
    IMS = 1,
    /** NET_CAPABILITY_EIMS on cellular. */
    EMERGENCY = 2,
}
