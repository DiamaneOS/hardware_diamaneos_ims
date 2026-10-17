#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
# Android mock tests: explicit emulator only. This invokes atest and may build.
set -eu
if [ "$#" -ne 1 ]; then
    echo "Usage: $0 emulator-SERIAL (after envsetup/lunch in the candidate Android tree)" >&2
    exit 2
fi
case "$1" in
    emulator-[0-9]*) ;;
    *) echo "Refusing a physical-device serial; use an isolated Android emulator." >&2; exit 2 ;;
esac
test -n "${ANDROID_BUILD_TOP:-}" || { echo "ANDROID_BUILD_TOP is not set" >&2; exit 2; }
test -f "$ANDROID_BUILD_TOP/hardware/diamaneos/ims/Android.bp" || {
    echo "The selected Android tree does not contain this IMS integration" >&2; exit 2;
}
command -v atest >/dev/null || { echo "atest is not available; run envsetup/lunch first" >&2; exit 2; }
qemu=$(adb -s "$1" shell getprop ro.kernel.qemu | tr -d '\r\n')
test "$qemu" = 1 || { echo "Target is not an Android emulator" >&2; exit 2; }
cd "$ANDROID_BUILD_TOP"
exec atest --serial "$1" \
    FrameworksTelephonyTests:com.android.internal.telephony.emergency.EmergencyStateTrackerTest \
    FrameworksTelephonyTests:com.android.internal.telephony.emergency.EmergencyNumberTrackerTest \
    FrameworksTelephonyTests:com.android.internal.telephony.data.PhoneSwitcherTest \
    FrameworksTelephonyTests:com.android.internal.telephony.imsphone.ImsPhoneCallTrackerTest \
    TeleServiceTests:com.android.services.telephony.TelephonyConnectionServiceTest \
    FrameworksMockingServicesTests:com.android.server.location.gnss.hal.GnssNativeTest \
    CarrierConfigTests ImsServiceEntitlementUnitTests
