#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
# Exercises production logic only. No emulator, modem, network, SMS or ADB.
set -eu
cd "$(dirname "$0")/.."
JAVAC=${JAVA_HOME:+$JAVA_HOME/bin/}javac
JAVA=${JAVA_HOME:+$JAVA_HOME/bin/}java
cargo test --offline --locked
cargo clippy --offline --all-targets -- -D warnings
mkdir -p tests/out/broker-core
"$JAVAC" -Xlint:all -Werror -d tests/out/broker-core \
    broker/src/de/diamaneos/imsbroker/NetworkState.java \
    tests/java/de/diamaneos/imsbroker/NetworkStateTest.java
"$JAVA" -cp tests/out/broker-core de.diamaneos.imsbroker.NetworkStateTest

mkdir -p tests/out/address-policy
"$JAVAC" -Xlint:all -Werror -d tests/out/address-policy \
    tests/stubs/android/system/OsConstants.java \
    common/src/de/diamaneos/ims/AddressPolicy.java \
    tests/java/de/diamaneos/ims/AddressPolicyTest.java
"$JAVA" -cp tests/out/address-policy de.diamaneos.ims.AddressPolicyTest

python3 -m unittest discover -s tests/python -v
