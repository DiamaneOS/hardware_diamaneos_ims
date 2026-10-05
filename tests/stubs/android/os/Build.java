// SPDX-License-Identifier: Apache-2.0
// Host-only fixture for the broker simulations. Never part of an Android module.
package android.os;

public final class Build {
    private Build() {}

    public static boolean isDebuggable() {
        return false;
    }
}
