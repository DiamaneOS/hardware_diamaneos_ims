/* SPDX-License-Identifier: Apache-2.0
 * Copyright 2026 The DiamaneOS Project
 */
package org.diamaneos.emergencylocation;

/** AML's ASCII subset packed into a data-SMS body; port/packing are receiver policy. */
final class SmsPayload {
    static byte[] pack(String value, boolean legacyMsbFirst) {
        int bits = Math.multiplyExact(value.length(), 7);
        byte[] output = new byte[(bits + 7) / 8];
        // A 16-bit application-port UDH uses seven of the 140 user-data octets.
        if (output.length > 133) throw new IllegalArgumentException("AML data SMS too long");
        for (int i = 0; i < value.length(); i++) {
            char c = value.charAt(i);
            // All AML fields emitted by AmlMessage use this unambiguous GSM-7 subset.
            if (!(Character.isDigit(c) && c < 128)
                    && !(c >= 'A' && c <= 'Z')
                    && !(c >= 'a' && c <= 'z')
                    && "\";=+-.".indexOf(c) < 0)
                throw new IllegalArgumentException("unsupported SMS character");
            for (int j = 0; j < 7; j++) {
                int bit = i * 7 + j;
                int source = legacyMsbFirst ? 6 - j : j;
                int destination = legacyMsbFirst ? 7 - (bit % 8) : bit % 8;
                if (((c >> source) & 1) != 0) output[bit / 8] |= (byte) (1 << destination);
            }
        }
        return output;
    }
}
