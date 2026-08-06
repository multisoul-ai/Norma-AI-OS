#!/usr/bin/env bash
# Verify that a packaged DMG retained the Finder metadata used by the branded
# drag-to-Applications installer window.

set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
dmg="${1:-}"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

[ "$#" -eq 1 ] || fail "usage: scripts/verify-dmg-layout.sh <path-to.dmg>"
[ -f "$dmg" ] || fail "DMG not found: $dmg"
command -v hdiutil >/dev/null 2>&1 || fail "hdiutil is required to inspect the DMG"

expected_background="$repo_root/assets/dmg-background.png"
[ -f "$expected_background" ] || fail "expected DMG background not found: $expected_background"

inspection_root=$(mktemp -d "${TMPDIR:-/tmp}/norma-dmg-layout.XXXXXX")
mount_point="$inspection_root/mount"
mkdir "$mount_point"
mounted=false

cleanup() {
  if [ "$mounted" = true ]; then
    hdiutil detach "$mount_point" >/dev/null 2>&1 || true
  fi
  rmdir "$mount_point" "$inspection_root" >/dev/null 2>&1 || true
}
trap cleanup EXIT INT TERM

hdiutil attach -nobrowse -readonly -mountpoint "$mount_point" "$dmg" >/dev/null
mounted=true

finder_metadata="$mount_point/.DS_Store"
packaged_background="$mount_point/.background/dmg-background.png"
applications_link="$mount_point/Applications"
app_bundle="$mount_point/Norma OS.app"

[ -s "$finder_metadata" ] \
  || fail "DMG is missing non-empty .DS_Store; Finder will show the generic folder layout"
[ -f "$packaged_background" ] \
  || fail "DMG is missing .background/dmg-background.png"
cmp -s "$expected_background" "$packaged_background" \
  || fail "packaged DMG background differs from assets/dmg-background.png"
[ -L "$applications_link" ] \
  || fail "DMG Applications target must be a symlink"
[ "$(readlink "$applications_link")" = "/Applications" ] \
  || fail "DMG Applications symlink must target /Applications"
[ -d "$app_bundle" ] \
  || fail "DMG is missing Norma OS.app"

for marker in '.bwspblob' '.icvpblob' 'dmg-background.png' 'Ilocblob'; do
  LC_ALL=C grep -aFq "$marker" "$finder_metadata" \
    || fail "DMG .DS_Store is missing Finder layout marker: $marker"
done

echo "verified customized Finder DMG layout: $dmg"
