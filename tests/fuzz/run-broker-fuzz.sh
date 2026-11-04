#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
# Random-input run of the IMS broker's inputs against the host fixtures in
# tests/stubs. No emulator, modem, network or ADB. Needs JDK 17 or later.
#   SEED=1 ITERATIONS=10000 ./tests/fuzz/run-broker-fuzz.sh   (random inputs)
#   ./tests/fuzz/run-broker-fuzz.sh FILE...                    (replay inputs)
# A coverage-guided JVM fuzzer can call BrokerFuzzer.fuzzerTestOneInput(byte[])
# with tests/out/broker-fuzz on its class path.
set -eu
cd "$(dirname "$0")/../.."
JAVAC=${JAVA_HOME:+$JAVA_HOME/bin/}javac
JAVA=${JAVA_HOME:+$JAVA_HOME/bin/}java
OUT=tests/out/broker-fuzz
mkdir -p "$OUT"
"$JAVAC" -Xlint:all -Werror -d "$OUT" \
    $(find tests/stubs -name '*.java' | sort) \
    common/src/de/diamaneos/ims/AddressPolicy.java \
    broker/src/de/diamaneos/imsbroker/Addresses.java \
    broker/src/de/diamaneos/imsbroker/NetworkState.java \
    broker/src/de/diamaneos/imsbroker/PdnTracker.java \
    broker/src/de/diamaneos/imsbroker/DcmConnection.java \
    broker/src/de/diamaneos/imsbroker/PdnBroker.java \
    tests/java/de/diamaneos/imsbroker/EmergencyBrokerTest.java \
    tests/fuzz/java/de/diamaneos/imsbroker/BrokerFuzzer.java
exec "$JAVA" -cp "$OUT" de.diamaneos.imsbroker.BrokerFuzzer "$@"
