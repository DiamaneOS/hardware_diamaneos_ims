# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 The DiamaneOS Project
#
# Vendor user and group of the IMS DCM daemon (imsdcmd.rc), in the OEM range.
# Stock FP6 /vendor/etc/passwd uses 2901-2917. Add this file to the device's
# TARGET_FS_CONFIG_GEN. The broker accepts calls only from this UID
# (PdnBroker.DAEMON_UID); keep the two equal.

[AID_VENDOR_IMSDCM]
value: 2990
