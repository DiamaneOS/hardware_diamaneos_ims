#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
# Exercises production logic only. No emulator, modem, network, SMS or ADB.
set -eu
cd "$(dirname "$0")/.."
JAVAC=${JAVA_HOME:+$JAVA_HOME/bin/}javac
JAVA=${JAVA_HOME:+$JAVA_HOME/bin/}java
cargo test --offline --locked
cargo clippy --offline --all-targets -- -D warnings
mkdir -p tests/out/aml tests/out/broker-core
"$JAVAC" -Xlint:all -Werror -d tests/out/aml \
    aml/src/org/diamaneos/emergencylocation/AmlMessage.java \
    aml/src/org/diamaneos/emergencylocation/AmlSession.java \
    aml/src/org/diamaneos/emergencylocation/AmlProfile.java \
    aml/src/org/diamaneos/emergencylocation/HttpsSender.java \
    aml/src/org/diamaneos/emergencylocation/TrustedEvent.java \
    aml/src/org/diamaneos/emergencylocation/SmsPayload.java \
    aml/tests/org/diamaneos/emergencylocation/SimulationTest.java \
    aml/tests/org/diamaneos/emergencylocation/TransportTest.java
"$JAVA" -cp tests/out/aml org.diamaneos.emergencylocation.SimulationTest
"$JAVA" -cp tests/out/aml org.diamaneos.emergencylocation.TransportTest
"$JAVAC" -Xlint:all -Werror -d tests/out/broker-core \
    broker/src/org/diamaneos/imsbroker/NetworkState.java \
    tests/java/org/diamaneos/imsbroker/NetworkStateTest.java
"$JAVA" -cp tests/out/broker-core org.diamaneos.imsbroker.NetworkStateTest

python3 -m unittest discover -s tests/python -v
