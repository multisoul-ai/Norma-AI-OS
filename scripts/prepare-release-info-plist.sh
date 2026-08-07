#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
source_plist="$repo_root/packaging/macos/Info.plist"
metadata="$repo_root/packaging/macos/sparkle-release.env"
output_plist="$repo_root/build/packaging/Info.plist"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

: "${SPARKLE_PUBLIC_ED_KEY:?SPARKLE_PUBLIC_ED_KEY must be set for a release build}"
printf '%s' "$SPARKLE_PUBLIC_ED_KEY" | grep -Eq '^[A-Za-z0-9+/]{43}=$' \
  || fail "SPARKLE_PUBLIC_ED_KEY must be a 44-character base64 Ed25519 public key"

[ -f "$source_plist" ] || fail "base Info.plist not found: $source_plist"
[ -f "$metadata" ] || fail "Sparkle metadata not found: $metadata"
# shellcheck source=/dev/null
source "$metadata"
: "${SPARKLE_FEED_URL:?missing SPARKLE_FEED_URL}"

mkdir -p "$(dirname "$output_plist")"
cp "$source_plist" "$output_plist"
/usr/libexec/PlistBuddy -c "Delete :SUFeedURL" "$output_plist" >/dev/null 2>&1 || true
/usr/libexec/PlistBuddy -c "Delete :SUPublicEDKey" "$output_plist" >/dev/null 2>&1 || true
/usr/libexec/PlistBuddy -c "Add :SUFeedURL string $SPARKLE_FEED_URL" "$output_plist"
/usr/libexec/PlistBuddy -c "Add :SUPublicEDKey string $SPARKLE_PUBLIC_ED_KEY" "$output_plist"
plutil -lint "$output_plist" >/dev/null

echo "generated release Info.plist at $output_plist"
