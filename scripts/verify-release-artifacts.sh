#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
tag=""
app=""
assets=""

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --tag) tag="${2:-}"; shift 2 ;;
    --app) app="${2:-}"; shift 2 ;;
    --assets) assets="${2:-}"; shift 2 ;;
    *) fail "unknown argument: $1" ;;
  esac
done

[ -n "$tag" ] || fail "--tag is required"
[ -d "$app" ] || fail "--app bundle not found: $app"
[ -d "$assets" ] || fail "--assets directory not found: $assets"

dmg="$assets/Norma-OS_aarch64.dmg"
expected_assets='Norma-OS_aarch64.dmg
Norma-OS_aarch64.dmg.sha256
appcast.xml'
actual_assets=$(find "$assets" -mindepth 1 -maxdepth 1 -exec basename {} \; | LC_ALL=C sort)
[ "$actual_assets" = "$expected_assets" ] \
  || fail "release directory must contain exactly the three canonical assets"

codesign --verify --deep --strict --verbose=2 "$app"
spctl --assess --type execute --verbose "$app"
xcrun stapler validate "$app"
hdiutil verify "$dmg"
"$repo_root/scripts/verify-dmg-layout.sh" "$dmg"
spctl --assess --type open --context context:primary-signature --verbose "$dmg"
xcrun stapler validate "$dmg"
python3 "$repo_root/scripts/verify-sparkle-appcast.py" \
  --tag "$tag" \
  --assets "$assets" \
  --plist "$app/Contents/Info.plist"

echo "verified notarized Sparkle release artifacts for $tag"
