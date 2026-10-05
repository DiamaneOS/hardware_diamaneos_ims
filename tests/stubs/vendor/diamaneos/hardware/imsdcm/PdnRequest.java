// SPDX-License-Identifier: Apache-2.0
// Host-only fixture for the broker simulations. Never part of an Android module.
package vendor.diamaneos.hardware.imsdcm;

public class PdnRequest {
    public static final int SLOT_UNSPECIFIED = -1;
    public int slot = SLOT_UNSPECIFIED;
    public int type = PdnType.IMS;
    public int serial;
}
