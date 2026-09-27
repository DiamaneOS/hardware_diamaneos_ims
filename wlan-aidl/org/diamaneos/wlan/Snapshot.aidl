// SPDX-License-Identifier: Apache-2.0
package org.diamaneos.wlan;
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
}
