// SPDX-License-Identifier: Apache-2.0
// Host-only fixture for the broker simulations. Never part of an Android module.
package android.util;

public final class Log {
    private Log() {}

    public static int i(String tag, String message) {
        return 0;
    }

    public static int w(String tag, String message) {
        return 0;
    }

    public static int e(String tag, String message) {
        return 0;
    }
}
