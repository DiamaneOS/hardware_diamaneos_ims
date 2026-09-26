/* SPDX-License-Identifier: Apache-2.0
 * Copyright 2026 The DiamaneOS Project
 */
package org.diamaneos.emergencylocation;

import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.net.URI;
import java.security.cert.Certificate;

import javax.net.ssl.HttpsURLConnection;
import javax.net.ssl.SSLException;

final class TransportTest {
    static final class Fake extends HttpsURLConnection {
        final int result;
        boolean disconnected;
        final ByteArrayOutputStream data = new ByteArrayOutputStream();

        Fake(int result) throws Exception {
            super(URI.create("https://receiver.invalid/aml").toURL());
            this.result = result;
        }

        public void connect() {}

        public void disconnect() {
            disconnected = true;
        }

        public boolean usingProxy() {
            return false;
        }

        public String getCipherSuite() {
            return "test";
        }

        public Certificate[] getLocalCertificates() {
            return new Certificate[0];
        }

        public Certificate[] getServerCertificates() {
            return new Certificate[0];
        }

        public ByteArrayOutputStream getOutputStream() {
            return data;
        }

        public int getResponseCode() {
            return result;
        }
    }

    public static void main(String[] args) throws Exception {
        URI uri = URI.create("https://receiver.invalid/aml");
        for (int code : new int[] {200, 204, 302, 307, 400, 401, 500}) {
            Fake f = new Fake(code);
            HttpsSender sender = new HttpsSender(u -> f);
            HttpsSender.Result r = sender.send(uri, new byte[] {1, 2});
            if (r != (code < 300 ? HttpsSender.Result.ACCEPTED : HttpsSender.Result.REJECTED))
                throw new AssertionError();
            if (f.getInstanceFollowRedirects()
                    || !f.disconnected
                    || f.getConnectTimeout() != 5000
                    || f.getReadTimeout() != 5000
                    || f.data.size() != 2) throw new AssertionError();
        }
        if (new HttpsSender(
                                u -> {
                                    throw new SSLException("synthetic");
                                })
                        .send(uri, new byte[1])
                != HttpsSender.Result.FAILED) throw new AssertionError();
        if (new HttpsSender(
                                u -> {
                                    throw new IOException("synthetic");
                                })
                        .send(uri, new byte[1])
                != HttpsSender.Result.FAILED) throw new AssertionError();
        System.out.println(
                "AML transport: 9 simulated outcomes passed, including TLS failure and redirects;"
                        + " no network used");
    }
}
