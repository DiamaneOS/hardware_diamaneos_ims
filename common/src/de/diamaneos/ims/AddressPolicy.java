// SPDX-License-Identifier: Apache-2.0
package de.diamaneos.ims;

import android.system.OsConstants;

/** Android LinkAddress.isPreferred flag rule, without changing address scope or selection. */
public final class AddressPolicy {
    private AddressPolicy() {}

    public static boolean isPreferred(int flags) {
        return (flags & (OsConstants.IFA_F_DADFAILED | OsConstants.IFA_F_DEPRECATED)) == 0
                && ((flags & OsConstants.IFA_F_TENTATIVE) == 0
                        || (flags & OsConstants.IFA_F_OPTIMISTIC) != 0);
    }
}
