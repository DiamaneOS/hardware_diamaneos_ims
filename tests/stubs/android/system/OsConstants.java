// SPDX-License-Identifier: Apache-2.0
package android.system;

// Host-only constants fixture. Never included in an Android product module.
public final class OsConstants {
    private OsConstants() {}
    public static final int IFA_F_OPTIMISTIC = 0x04;
    public static final int IFA_F_DADFAILED = 0x08;
    public static final int IFA_F_DEPRECATED = 0x20;
    public static final int IFA_F_TENTATIVE = 0x40;
}
