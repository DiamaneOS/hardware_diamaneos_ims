// SPDX-License-Identifier: Apache-2.0
// Host-only fixture for the broker simulations. Never part of an Android module.
package android.content;

import java.util.HashMap;
import java.util.Map;

public class Context {
    private final Map<Class<?>, Object> services = new HashMap<>();

    public <T> void putSystemService(Class<T> type, T service) {
        services.put(type, service);
    }

    public <T> T getSystemService(Class<T> type) {
        return type.cast(services.get(type));
    }
}
