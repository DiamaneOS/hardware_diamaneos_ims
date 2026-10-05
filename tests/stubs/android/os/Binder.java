// SPDX-License-Identifier: Apache-2.0
// Host-only fixture for the broker simulations. Never part of an Android module.
package android.os;

public class Binder {
    public static volatile int callingUid;

    public static int getCallingUid() {
        return callingUid;
    }
}
