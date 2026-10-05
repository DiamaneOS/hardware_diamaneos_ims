# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 The DiamaneOS Project
#
# Vendor user and group of the IMS DCM daemon (imsdcmd.rc), in the OEM range.
# Stock FP6 /vendor/etc/passwd uses 2901-2917. Add this file to the device's
# TARGET_FS_CONFIG_GEN. The broker accepts calls only from this UID
# (PdnBroker.DAEMON_UID); keep the two equal.

[AID_VENDOR_IMSDCM]
value: 2990

# Vendor user and group of the one-shot IMEI provisioning tool (imeiprovd.rc).
# 2990/2991 are the IMS DCM daemon and Wi-Fi reporter; 2993 is reserved for a
# planned tqftpserv UID, so this takes 2992.
[AID_VENDOR_IMEIPROV]
value: 2992
