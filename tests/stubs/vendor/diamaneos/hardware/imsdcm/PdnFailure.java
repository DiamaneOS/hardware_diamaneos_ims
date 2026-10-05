// SPDX-License-Identifier: Apache-2.0
// Host-only fixture for the broker simulations. Never part of an Android module.
package vendor.diamaneos.hardware.imsdcm;

public @interface PdnFailure {
    int INVALID_REQUEST = 1;
    int REQUEST_REJECTED = 2;
    int UNAVAILABLE = 3;
}
