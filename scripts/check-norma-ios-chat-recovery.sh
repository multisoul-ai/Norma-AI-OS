#!/bin/bash
# NORMA_SWIFT_PROBE for the existing isolated mobile_notification_native fixture.
set -euo pipefail
[[ $# == 2 && -f "$1" && "$2" == notification-ui ]]
: "${NORMA_IOS_SIMULATOR:?explicit isolated simulator required}"
root=$(cd "$(dirname "$0")/.." && pwd)
build=(xcodebuild -project "$root/ios/Norma.xcodeproj" -scheme Norma
  -destination "platform=iOS Simulator,id=$NORMA_IOS_SIMULATOR"
  -derivedDataPath "${NORMA_IOS_DERIVED_DATA:-$root/ios/DerivedData}")
"${build[@]}" NORMA_TEST_QR="$1" NORMA_TEST_MODE="$2" \
  -only-testing:NormaDeviceTests/NotificationFlowTests/testPairedNotificationLifecycle test
status=0
"${build[@]}" NORMA_TEST_QR="$1" NORMA_TEST_MODE="$2" \
  -only-testing:NormaUITests/ChatInteractionFlowTests/testReconnectKeepsDraftAndKeyboardOnOriginalCard test || status=$?
# Clean only the exact fixture identity using the existing native recovery path, even on red tests.
container=$(xcrun simctl get_app_container "$NORMA_IOS_SIMULATOR" com.norma-ai.os.ios data)
cp "$container/Library/Application Support/Norma/machines.json" "$(dirname "$1")/cleanup.json"
"${build[@]}" NORMA_TEST_QR="$(dirname "$1")/cleanup.json" NORMA_TEST_MODE=notification-cleanup \
  -only-testing:NormaDeviceTests/NotificationFlowTests/testRecoverExpiredNotificationFixture test
touch "$(dirname "$1")/ui-finished"
exit "$status"
