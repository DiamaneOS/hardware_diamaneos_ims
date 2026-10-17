// SPDX-License-Identifier: Apache-2.0
// Host-only fixture for the broker simulations. Never part of an Android module.
package vendor.diamaneos.hardware.imsdcm;

public class PdnInfo {
    public long networkHandle;
    public byte[] ipv4Address = {};
    public byte[] ipv6Address = {};
    public int mtu;
}
