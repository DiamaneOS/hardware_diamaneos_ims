# SPDX-License-Identifier: Apache-2.0
BOARD_VENDOR_SEPOLICY_DIRS += hardware/diamaneos/ims/sepolicy/vendor
SYSTEM_EXT_PUBLIC_SEPOLICY_DIRS += hardware/diamaneos/ims/sepolicy/system_ext/public
SYSTEM_EXT_PRIVATE_SEPOLICY_DIRS += hardware/diamaneos/ims/sepolicy/system_ext/private
TARGET_FS_CONFIG_GEN += hardware/diamaneos/ims/vintf/config.fs
DEVICE_FRAMEWORK_COMPATIBILITY_MATRIX_FILE += hardware/diamaneos/ims/vintf/framework_compatibility_matrix.xml
# The device owns its QRTR-domain inventory. Review and adapt
# sepolicy/fp6-device/qrtr.te rather than importing it for unrelated devices.
