# SPDX-License-Identifier: Apache-2.0
# Opt-in native Qualcomm IWLAN reporter candidate. Do not select with AOSP IKE.
ifneq ($(DIAMANEOS_IWLAN_IMPLEMENTATION),qti)
$(error Wi-Fi modem reporting requires the qualified QTI IWLAN product path)
endif
PRODUCT_PACKAGES += wlanreportd DiamaneOSWlanObserver passwd_vendor group_vendor
