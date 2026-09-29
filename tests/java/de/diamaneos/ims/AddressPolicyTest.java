// SPDX-License-Identifier: Apache-2.0
package de.diamaneos.ims;

import android.system.OsConstants;

public final class AddressPolicyTest {
    public static void main(String[] args) {
        final int[] bits = {OsConstants.IFA_F_TENTATIVE, OsConstants.IFA_F_OPTIMISTIC,
                OsConstants.IFA_F_DADFAILED, OsConstants.IFA_F_DEPRECATED};
        // Neither bad flag may be overridden by optimistic status.
        final boolean[] expected = {true, false, true, true,
                false, false, false, false, false, false, false, false,
                false, false, false, false};
        for (int combination = 0; combination < expected.length; combination++) {
            int flags = 0;
            for (int bit = 0; bit < bits.length; bit++) {
                if ((combination & (1 << bit)) != 0) flags |= bits[bit];
            }
            for (int unrelated : new int[] {0, 0x80}) {
                if (AddressPolicy.isPreferred(flags | unrelated) != expected[combination]) {
                    throw new AssertionError("Address eligibility combination " + combination);
                }
            }
        }
        System.out.println("Address eligibility: all flag combinations and unrelated-bit checks passed");
    }
}
