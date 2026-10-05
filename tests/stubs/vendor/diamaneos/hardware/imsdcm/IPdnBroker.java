// SPDX-License-Identifier: Apache-2.0
// Host-only fixture for the broker simulations. Never part of an Android module.
package vendor.diamaneos.hardware.imsdcm;

import android.os.RemoteException;

public interface IPdnBroker {
    int VERSION = 1;
    String HASH = "host-fixture";

    void bringUp(PdnRequest request) throws RemoteException;

    void release(PdnRequest request) throws RemoteException;

    int getInterfaceVersion() throws RemoteException;

    String getInterfaceHash() throws RemoteException;

    abstract class Stub implements IPdnBroker {}
}
