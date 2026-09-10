#!/bin/bash
# Build only the shared PAKE primitive; no GPUI/runtime code is linked into iOS.
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root"
export MACOSX_DEPLOYMENT_TARGET=14.0
export IPHONEOS_DEPLOYMENT_TARGET=17.0
for triple in aarch64-apple-darwin x86_64-apple-darwin aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios; do
  cargo build --locked -p mobile_pairing --release --target "$triple"
done
stage=$(mktemp -d "$root/ios/.pairing-build.XXXXXX")
trap 'rm -rf "$stage"' EXIT
mkdir -p "$stage/mac" "$stage/simulator"
xcrun lipo -create target/aarch64-apple-darwin/release/libmobile_pairing.a target/x86_64-apple-darwin/release/libmobile_pairing.a -output "$stage/mac/libmobile_pairing.a"
xcrun lipo -create target/aarch64-apple-ios-sim/release/libmobile_pairing.a target/x86_64-apple-ios/release/libmobile_pairing.a -output "$stage/simulator/libmobile_pairing.a"
xcodebuild -create-xcframework \
  -library "$stage/mac/libmobile_pairing.a" -headers "$root/ios/PairingBridge/include" \
  -library "$stage/simulator/libmobile_pairing.a" -headers "$root/ios/PairingBridge/include" \
  -library "$root/target/aarch64-apple-ios/release/libmobile_pairing.a" -headers "$root/ios/PairingBridge/include" \
  -output "$stage/NormaPairing.xcframework"
mkdir -p ios/Libraries
rm -rf ios/Libraries/NormaPairing.xcframework
mv "$stage/NormaPairing.xcframework" ios/Libraries/NormaPairing.xcframework
