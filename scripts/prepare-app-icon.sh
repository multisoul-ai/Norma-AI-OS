#!/usr/bin/env bash
# Compile the full-bleed macOS 26+ Icon Composer resource. AppIcon.icns remains
# the separately named fallback for earlier macOS; do not replace it with the
# flattened ICNS that actool generates for the modern artwork.

set -euo pipefail
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
output_dir=${1:-"$repo_root/build/icons"}
temporary_output=$(mktemp -d "${TMPDIR:-/tmp}/norma-native-icon.XXXXXX")
trap 'rm -rf "$temporary_output"' EXIT

xcrun actool "$repo_root/assets/NormaIcon.icon" \
  --compile "$temporary_output" \
  --platform macosx \
  --minimum-deployment-target 12.0 \
  --app-icon NormaIcon \
  --output-partial-info-plist "$temporary_output/icon-info.plist" \
  --output-format human-readable-text

if [ ! -s "$temporary_output/Assets.car" ]; then
  echo "actool did not produce the native macOS icon Assets.car" >&2
  exit 1
fi
mkdir -p "$output_dir"
cp "$temporary_output/Assets.car" "$output_dir/Assets.car"
echo "Prepared $output_dir/Assets.car"
