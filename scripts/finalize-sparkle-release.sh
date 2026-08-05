#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
tag=""
app=""
dmg=""
output_dir="$repo_root/build/release-assets"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --tag) tag="${2:-}"; shift 2 ;;
    --app) app="${2:-}"; shift 2 ;;
    --dmg) dmg="${2:-}"; shift 2 ;;
    --output) output_dir="${2:-}"; shift 2 ;;
    *) fail "unknown argument: $1" ;;
  esac
done

: "${SPARKLE_EDDSA_PRIVATE_KEY:?SPARKLE_EDDSA_PRIVATE_KEY must be set}"
sparkle_private_key="$SPARKLE_EDDSA_PRIVATE_KEY"
unset SPARKLE_EDDSA_PRIVATE_KEY
[ -n "$tag" ] || fail "--tag is required"
[ -d "$app" ] || fail "--app bundle not found: $app"
[ -f "$dmg" ] || fail "--dmg not found: $dmg"

version=$("$repo_root/scripts/release-version.sh")
[ "$tag" = "v$version" ] || fail "tag $tag must match release version v$version"
case "$tag" in
  v[0-9]*.[0-9]*.[0-9]*) ;;
  *) fail "tag must use vMAJOR.MINOR.PATCH: $tag" ;;
esac

generate_appcast="$repo_root/build/sparkle/bin/generate_appcast"
[ -x "$generate_appcast" ] || fail "verified generate_appcast is missing; run scripts/prepare-sparkle.sh"

mkdir -p "$output_dir"
canonical_dmg="$output_dir/Norma-OS_aarch64.dmg"
checksum="$output_dir/Norma-OS_aarch64.dmg.sha256"
appcast="$output_dir/appcast.xml"
for existing in "$canonical_dmg" "$checksum" "$appcast"; do
  [ ! -e "$existing" ] || fail "refusing to overwrite existing release asset: $existing"
done

ditto "$dmg" "$canonical_dmg"
download_prefix="https://github.com/multisoul-ai/norma-os-releases/releases/download/$tag/"
printf '%s' "$sparkle_private_key" | "$generate_appcast" \
  --ed-key-file - \
  --download-url-prefix "$download_prefix" \
  --maximum-versions 1 \
  --maximum-deltas 0 \
  "$output_dir"
sparkle_private_key=""

[ -f "$appcast" ] || fail "generate_appcast did not create appcast.xml"
(
  cd "$output_dir"
  shasum -a 256 Norma-OS_aarch64.dmg >Norma-OS_aarch64.dmg.sha256
)

python3 "$repo_root/scripts/verify-sparkle-appcast.py" \
  --tag "$tag" \
  --assets "$output_dir" \
  --plist "$app/Contents/Info.plist"

echo "finalized Sparkle release assets in $output_dir"
