# SPDX-License-Identifier: Apache-2.0
# Include only after the product pins its verified QRTR modem node and adds the
# policy directories and UID to BoardConfig. No modem-node default is guessed.
ifeq ($(strip $(DIAMANEOS_IMS_MODEM_NODE)),)
$(error DIAMANEOS_IMS_MODEM_NODE must identify the qualified modem QRTR node)
endif
ifeq ($(strip $(DIAMANEOS_IMS_SLOTS)),)
$(error DIAMANEOS_IMS_SLOTS must specify the product's logical SIM slot count)
endif
PRODUCT_PACKAGES += imsdcmd DiamaneOSImsBroker
PRODUCT_VENDOR_PROPERTIES += \
    ro.vendor.diamaneos.ims.modem_node=$(DIAMANEOS_IMS_MODEM_NODE) \
    ro.vendor.diamaneos.ims.slots=$(DIAMANEOS_IMS_SLOTS) \
    ro.vendor.diamaneos.ims.emergency_pdn=serve \
    persist.vendor.diamaneos.ims.dcm_kill=0 \
    persist.vendor.diamaneos.ims.emergency_pdn_kill=0
