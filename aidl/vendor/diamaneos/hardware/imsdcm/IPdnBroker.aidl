/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright 2026 The DiamaneOS Project
 */

package vendor.diamaneos.hardware.imsdcm;

import vendor.diamaneos.hardware.imsdcm.PdnRequest;

/**
 * Implemented by the broker app and passed to IImsDcm.setBroker(). Only the
 * daemon holds a reference; the broker also ignores calls from any other UID.
 *
 * Both calls are oneway, so a slow or dead broker never blocks the daemon.
 */
@VintfStability
interface IPdnBroker {
    /**
     * Requests the network for (slot, type) and keeps it requested until
     * release(). Filing follows stock CneApp: with a slot, the request carries
     * that slot's subscription and waits while the slot has no active
     * subscription; without a slot, it carries no subscription.
     *
     * A second bringUp() for a (slot, type) the broker already holds does not
     * file a new request. It adopts the new serial and reports the current
     * state again (up or down) if there is one.
     */
    oneway void bringUp(in PdnRequest request);

    /**
     * Releases the network for (slot, type) if the broker holds it under the
     * same serial; otherwise it is ignored. No report follows.
     */
    oneway void release(in PdnRequest request);
}
