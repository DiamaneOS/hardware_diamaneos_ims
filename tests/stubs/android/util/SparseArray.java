// SPDX-License-Identifier: Apache-2.0
// Host-only fixture for the broker simulations. Never part of an Android module.
package android.util;

import java.util.ArrayList;
import java.util.Map;
import java.util.TreeMap;

/** Key-ordered, like the platform class, so index iteration matches. */
public class SparseArray<E> {
    private final TreeMap<Integer, E> map = new TreeMap<>();

    public E get(int key) {
        return map.get(key);
    }

    public void put(int key, E value) {
        map.put(key, value);
    }

    public void remove(int key) {
        map.remove(key);
    }

    public int size() {
        return map.size();
    }

    public E valueAt(int index) {
        return new ArrayList<Map.Entry<Integer, E>>(map.entrySet()).get(index).getValue();
    }

    public void clear() {
        map.clear();
    }
}
