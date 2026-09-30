#!/bin/bash
# Run only as NORMA_SWIFT_PROBE for mobile_notification_native's isolated TLS fixture.
set -euo pipefail
[[ $# == 2 && -f "$1" && "$2" == notification-ui ]]
: "${NORMA_IOS_SIMULATOR:?explicit isolated simulator required}"
root=$(cd "$(dirname "$0")/.." && pwd)
build=(xcodebuild -project "$root/ios/Norma.xcodeproj" -scheme Norma
  -destination "platform=iOS Simulator,id=$NORMA_IOS_SIMULATOR"
  -derivedDataPath "${NORMA_IOS_DERIVED_DATA:-$root/ios/DerivedData}"
  NORMA_TEST_QR="$1" NORMA_TEST_MODE="$2")
"${build[@]}" -only-testing:NormaDeviceTests/NotificationFlowTests/testPairedNotificationLifecycle test
"${build[@]}" -only-testing:NormaUITests/ChatVoiceInputFlowTests test
if [[ -z "${NORMA_CHAT_VOICE_KEEP_UI:-}" ]]; then
  touch "$(dirname "$1")/ui-finished"
fi
