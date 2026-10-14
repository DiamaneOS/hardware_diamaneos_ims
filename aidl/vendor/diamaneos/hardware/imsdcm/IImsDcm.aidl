/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright 2026 The DiamaneOS Project
 */

package vendor.diamaneos.hardware.imsdcm;

import vendor.diamaneos.hardware.imsdcm.IPdnBroker;
import vendor.diamaneos.hardware.imsdcm.PdnFailure;
import vendor.diamaneos.hardware.imsdcm.PdnInfo;
import vendor.diamaneos.hardware.imsdcm.PdnRequest;

/**
 * Served by the IMS DCM daemon as "vendor.diamaneos.hardware.imsdcm.IImsDcm/default".
 *
 * The daemon answers the modem's QMI IMS DCM requests. For a PDN activation it
 * needs the Android IMS (or emergency) network, which only an app holding
 * CONNECTIVITY_USE_RESTRICTED_NETWORKS can request. That app, the broker, is
 * the only client of this service: SELinux lets no other domain call the
 * daemon, and the daemon pins the authorized registration UID. The daemon refuses
 * permissive SELinux because vendor Binder does not expose the caller SID.
 *
 * Flow: the broker registers itself with setBroker(). The daemon then calls
 * IPdnBroker.bringUp() and IPdnBroker.release(); the broker answers each
 * bring-up with onPdnUp(), onPdnDown() or onPdnFailed() on this interface.
 * Every report echoes the PdnRequest of the bring-up it belongs to, including
 * its serial, so the daemon can drop reports that crossed a release.
 *
 * The broker reports only the network handle, one address per IP family and
 * the MTU. It never reports interface names, P-CSCF addresses or other link
 * details, and neither side logs addresses.
 */
@VintfStability
interface IImsDcm {
    /**
     * Registers the broker, replacing any earlier registration. After it
     * returns, the daemon calls broker.bringUp() for every PDN the modem
     * currently wants.
     *
     * @throws EX_SECURITY for an invalid or different registered app UID.
     * SELinux permits calls from the broker domain only.
     */
    void setBroker(in IPdnBroker broker);

    /**
     * The network for the request is connected, or its handle, addresses or
     * MTU changed since the last report. Sent only while the network is
     * available and has the requested capability on cellular.
     */
    oneway void onPdnUp(in PdnRequest request, in PdnInfo info);

    /**
     * The network for the request was lost. The broker still holds the
     * request, so onPdnUp() follows if Android brings a network back. As on
     * stock, the daemon ends the affected modem sessions but keeps the
     * request until the modem releases it or goes away. The modem's next
     * activation for the same slot and type uses it, with the same serial.
     */
    oneway void onPdnDown(in PdnRequest request);

    /**
     * The broker could not file the request, or Android declared it
     * unavailable. The broker holds nothing for it any more. The daemon ends
     * its sessions, and a new bringUp() is needed to try again.
     */
    oneway void onPdnFailed(in PdnRequest request, PdnFailure reason);
}
