# SPDX-License-Identifier: Apache-2.0
# Opt-in only after receiver contract, framework bridge and enforcing policy pass.
ifneq ($(DIAMANEOS_AML_QUALIFIED_PROFILE),true)
$(error AML requires a qualified receiver profile and platform bridge)
endif
PRODUCT_PACKAGES += DiamaneOSEmergencyLocation
