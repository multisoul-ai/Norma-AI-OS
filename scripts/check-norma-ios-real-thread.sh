#!/bin/bash
# Called only by mobile_thread_codex_real; identity preparation is isolated, task entry is through UI.
set -euo pipefail
test "$#" = 1 && test -f "$1"
: "${NORMA_IOS_SIMULATOR:?an explicit simulator is required}"
root=$(cd "$(dirname "$0")/.." && pwd)
fixture=$1
evidence=$(dirname "$fixture")
native=(xcodebuild test -project "$root/ios/Norma.xcodeproj" -scheme Norma
  -destination "platform=iOS Simulator,id=$NORMA_IOS_SIMULATOR"
  -derivedDataPath "$root/target/norma-ios-delivery-native"
  -clonedSourcePackagesDirPath "$root/target/ios-native-packages"
  -packageCachePath "$root/target/ios-native-cache"
  -disableAutomaticPackageResolution -parallel-testing-enabled NO
  NORMA_TEST_MODE=thread-real-ui "NORMA_THREAD_REAL_FIXTURE=$fixture")
cleanup() {
  "${native[@]}" -only-testing:NormaDeviceTests/RealMacThreadAppTests/testCleanupRealThreadUI \
    > "$evidence/native-cleanup.log" 2>&1
}
trap cleanup EXIT
"${native[@]}" -only-testing:NormaDeviceTests/RealMacThreadAppTests/testPrepareRealThreadUI \
  > "$evidence/native-prepare.log" 2>&1
touch "$evidence/ui-cua-requested"
"${native[@]}" -only-testing:NormaUITests/RealMacThreadFlowTests/testRealTaskFromPhoneUIAndRestore \
  -resultBundlePath "$evidence/real-phone-ui.xcresult" > "$evidence/native-ui.log" 2>&1
