// SPDX-License-Identifier: Apache-2.0
// Host-only fixture for the broker simulations. Never part of an Android module.
package vendor.diamaneos.hardware.imsdcm;

import android.os.IBinder;
import android.os.RemoteException;

public interface IImsDcm {
    String DESCRIPTOR = "vendor.diamaneos.hardware.imsdcm.IImsDcm";
    int VERSION = 1;

    void setBroker(IPdnBroker broker) throws RemoteException;

    void onPdnUp(PdnRequest request, PdnInfo info) throws RemoteException;

    void onPdnDown(PdnRequest request) throws RemoteException;

    void onPdnFailed(PdnRequest request, int reason) throws RemoteException;

    int getInterfaceVersion() throws RemoteException;

    abstract class Stub {
        /** The simulated daemon object is both the binder and the interface. */
        public static IImsDcm asInterface(IBinder binder) {
            return binder instanceof IImsDcm ? (IImsDcm) binder : null;
        }
    }
}
