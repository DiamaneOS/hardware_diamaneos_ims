/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright 2026 The DiamaneOS Project
 */

package vendor.diamaneos.hardware.imsdcm;

/** Why the broker dropped a request (IImsDcm.onPdnFailed). */
@VintfStability
@Backing(type="int")
enum PdnFailure {
    /** Unknown PdnType, or a slot outside 0..2 other than SLOT_UNSPECIFIED. */
    INVALID_REQUEST = 1,
    /** ConnectivityManager refused the request (for example, no permission). */
    REQUEST_REJECTED = 2,
    /** ConnectivityManager reported the network unavailable (onUnavailable). */
    UNAVAILABLE = 3,
}
