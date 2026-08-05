#!/usr/bin/env bash
# Sparkle appcast seam: immutable tag URL, versions, length, signature, and checksum agree.
#
# Scenario:
#   A canonical DMG and appcast are checked before upload; mutable or internally
#   inconsistent metadata must be rejected even when every file exists.
#
# Data construction (including key numeric derivation):
#   fixture DMG bytes = "norma-update" = 12 bytes
#   short/build version = 1.2.3 / 10203
#   canonical tag URL = .../releases/download/v1.2.3/Norma-OS_aarch64.dmg
#
# Execution:
#   1. Create a 12-byte DMG fixture, matching checksum, plist, and signed enclosure.
#   2. Verify the consistent fixture succeeds.
#   3. Replace the tag URL with releases/latest/download and require failure.
#   4. Restore the URL, change enclosure length from 12 to 13, and require failure.
#
# Expected result:
#   - Positive: the exact tag URL, versions, 12-byte length, signature, and SHA pass.
#   - Negative: a mutable latest DMG URL must fail.
#   - Negative: a 13-byte enclosure claim for a 12-byte file must fail.

set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
verifier="$repo_root/scripts/verify-sparkle-appcast.py"
tmp_dir=$(mktemp -d)
trap 'rm -rf "$tmp_dir"' EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

printf 'norma-update' >"$tmp_dir/Norma-OS_aarch64.dmg"
(
  cd "$tmp_dir"
  shasum -a 256 Norma-OS_aarch64.dmg >Norma-OS_aarch64.dmg.sha256
)
cat >"$tmp_dir/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleShortVersionString</key><string>1.2.3</string>
<key>CFBundleVersion</key><string>10203</string>
</dict></plist>
PLIST
cat >"$tmp_dir/appcast.xml" <<'XML'
<?xml version="1.0" encoding="utf-8"?>
<rss version="2.0" xmlns:sparkle="https://sparkle-project.org/xml-namespaces/sparkle">
<channel><item><enclosure
url="https://github.com/multisoul-ai/norma-os-releases/releases/download/v1.2.3/Norma-OS_aarch64.dmg"
length="12" sparkle:version="10203" sparkle:shortVersionString="1.2.3"
sparkle:edSignature="fixture-signature" /></item></channel></rss>
XML

python3 "$verifier" --tag v1.2.3 --assets "$tmp_dir" --plist "$tmp_dir/Info.plist" \
  || fail "consistent appcast fixture should pass"

sed -i.bak 's#/releases/download/v1.2.3/#/releases/latest/download/#' "$tmp_dir/appcast.xml"
if python3 "$verifier" --tag v1.2.3 --assets "$tmp_dir" --plist "$tmp_dir/Info.plist" >/dev/null 2>&1; then
  fail "mutable latest DMG URL should be rejected"
fi
mv "$tmp_dir/appcast.xml.bak" "$tmp_dir/appcast.xml"

sed -i.bak 's/length="12"/length="13"/' "$tmp_dir/appcast.xml"
if python3 "$verifier" --tag v1.2.3 --assets "$tmp_dir" --plist "$tmp_dir/Info.plist" >/dev/null 2>&1; then
  fail "enclosure length 13 should be rejected for the 12-byte fixture"
fi
rm "$tmp_dir/appcast.xml.bak"

echo "Sparkle appcast test passed"
