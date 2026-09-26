/* SPDX-License-Identifier: Apache-2.0
 * Copyright 2026 The DiamaneOS Project
 */
package org.diamaneos.emergencylocation;

import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.telephony.SubscriptionManager;

/** Receives only the protected event sent by system_server with shared sender identity. */
public final class EmergencyReceiver extends BroadcastReceiver {
    @Override
    public void onReceive(Context context, Intent intent) {
        if (intent == null) return;
        String number = intent.getStringExtra("number");
        if (!TrustedEvent.accepts(getSentFromUid(), intent.getAction(), number)) return;
        int subId = intent.getIntExtra("subscription", SubscriptionManager.INVALID_SUBSCRIPTION_ID);
        int phoneId = intent.getIntExtra("phone", -1);
        ((EmergencyLocationApp) context.getApplicationContext())
                .emergencyCommunication(
                        number,
                        subId,
                        phoneId,
                        TrustedEvent.source(intent.getAction()),
                        intent.getLongExtra("utc", -1),
                        intent.getLongExtra("elapsed", -1));
    }
}
