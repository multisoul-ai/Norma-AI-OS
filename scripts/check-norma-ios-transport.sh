#!/bin/bash
# Called ONLY as NORMA_SWIFT_PROBE by mobile tests, with an isolated QR fixture.
set -euo pipefail
if [[ $# != 2 || ! -f "$1" ]]; then
  echo "usage: check-norma-ios-transport.sh <isolated-qr-path> <code|normal|wrong-pin|conversation|messages|interrupts|queues|approvals|external|events|large|chat-events|plans|todos|reviews|feedback|notifications|notification-ui>" >&2
  exit 2
fi
test_target=NormaDeviceTests/NativeTransportTests
if [[ "$2" == notifications || "$2" == notification-ui ]]; then
  : "${NORMA_IOS_SIMULATOR:?notification acceptance requires an explicit isolated simulator}"
  test_target=NormaDeviceTests/NotificationFlowTests
fi
root=$(cd "$(dirname "$0")/.." && pwd)
exec xcodebuild -project "$root/ios/Norma.xcodeproj" -scheme Norma \
  -destination "platform=iOS Simulator,id=${NORMA_IOS_SIMULATOR:-AF8B48D2-975E-4F81-97AB-B5FB6F555618}" \
  -derivedDataPath "$root/ios/DerivedData" \
  -packageCachePath "$root/target/xcode-packages" \
  NORMA_TEST_QR="$1" NORMA_TEST_MODE="$2" \
  -only-testing:"$test_target" test 2>&1
