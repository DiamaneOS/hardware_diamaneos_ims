/* SPDX-License-Identifier: Apache-2.0
 * Copyright 2026 The DiamaneOS Project
 */
package org.diamaneos.emergencylocation;

/** Same predicate used by the receiver and the host authorization test. */
final class TrustedEvent {
    static final String ACTION = "org.diamaneos.emergencylocation.action.CALL";

    static final String SMS_ACTION = "org.diamaneos.emergencylocation.action.SMS";

    static String source(String action) {
        return ACTION.equals(action) ? "CALL" : SMS_ACTION.equals(action) ? "SMS" : null;
    }

    static boolean fresh(long eventElapsed, long now) {
        return eventElapsed >= 0 && now >= eventElapsed && now - eventElapsed <= 10000;
    }

    static boolean accepts(int sender, String action, String number) {
        return sender == 1000
                && source(action) != null
                && number != null
                && number.matches("[0-9]{2,6}");
    }
}
