// SPDX-License-Identifier: Apache-2.0
// Host-only fixture for the broker simulations. Never part of an Android module.
package android.os;

public interface IBinder {
    interface DeathRecipient {
        void binderDied();
    }

    void linkToDeath(DeathRecipient recipient, int flags) throws RemoteException;

    boolean unlinkToDeath(DeathRecipient recipient, int flags);
}
