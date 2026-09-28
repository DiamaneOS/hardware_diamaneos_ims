// SPDX-License-Identifier: Apache-2.0
package de.diamaneos.wlan;
parcelable Snapshot {
    boolean enabled;
    boolean connected;
    boolean validated;
    byte[6] bssid;
    boolean hasIpv4;
    byte[4] ipv4;
    boolean hasIpv6;
    byte[16] ipv6;
    int ipv6Prefix;
    int dns4Count;
    byte[8] dns4;
    int dns6Count;
    byte[32] dns6;
}
