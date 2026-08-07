#!/usr/bin/env bash
# Release plist seam: inject only caller-provided public Sparkle metadata.
#
# Scenario:
#   A release build receives a public Ed25519 key at runtime and produces a
#   complete plist without weakening existing privacy or minimum-OS metadata.
#
# Data construction (including key numeric derivation):
#   fixture base64 key length = 44 = 43 data characters + 1 padding character
#   invalid key length        = 12, which cannot encode a 32-byte Ed25519 public key
#
# Execution:
#   1. Generate the release plist with a syntactically valid ephemeral test value.
#   2. Read feed, public key, macOS version, and microphone usage keys.
#   3. Retry with a 12-character invalid value and require failure.
#
# Expected result:
#   - Positive: feed URL and 44-character public value are injected exactly once.
#   - Positive: macOS 12.0 and privacy descriptions remain present.
#   - Negative: invalid key material is rejected instead of entering the bundle.

set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
generated="$repo_root/build/packaging/Info.plist"
# 32 bytes encode to 44 base64 characters; build a syntactic fixture at runtime
# so no value that could be mistaken for the production public key is checked in.
fixture_public_key="$(printf '%043d' 0 | tr '0' 'A')="

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

SPARKLE_PUBLIC_ED_KEY="$fixture_public_key" "$repo_root/scripts/prepare-release-info-plist.sh" >/dev/null
feed=$(/usr/libexec/PlistBuddy -c 'Print :SUFeedURL' "$generated")
public_key=$(/usr/libexec/PlistBuddy -c 'Print :SUPublicEDKey' "$generated")
minimum=$(/usr/libexec/PlistBuddy -c 'Print :LSMinimumSystemVersion' "$generated")
microphone=$(/usr/libexec/PlistBuddy -c 'Print :NSMicrophoneUsageDescription' "$generated")

[ "$feed" = 'https://github.com/multisoul-ai/norma-os-releases/releases/latest/download/appcast.xml' ] \
  || fail "generated plist should use the canonical GitHub appcast feed"
[ "$public_key" = "$fixture_public_key" ] || fail "generated plist should preserve the runtime public key exactly"
[ "$minimum" = '12.0' ] || fail "generated plist should preserve macOS 12.0 minimum"
[ -n "$microphone" ] || fail "generated plist should preserve microphone privacy text"
if SPARKLE_PUBLIC_ED_KEY='invalid-key' "$repo_root/scripts/prepare-release-info-plist.sh" >/dev/null 2>&1; then
  fail "invalid 12-character public key should be rejected"
fi

echo "release Info.plist test passed"
