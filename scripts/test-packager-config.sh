#!/usr/bin/env bash
# Focused test for Packager.toml release binary wiring.
#
# Scenario:
#   Packager.toml packages target/release/norma-os as the main macOS bundle
#   executable and target/release/multisoul-agent-hook as a secondary helper.
#
# Data construction:
#   main binary path     = target/release/norma-os
#   hook binary path     = target/release/multisoul-agent-hook
#   planner helper path  = sidecars/openai-card-planner/openai-card-planner.mjs
#   release build hook   = before-packaging-command in Packager.toml
#   required cargo target = -p app（app package now owns both release binaries）
#
# Execution:
#   1. Read Packager.toml from the repository root.
#   2. Verify the secondary hook binary is listed as a packaged binary.
#   3. Verify the one-shot pre-packaging command builds the app package.
#
# Expected result:
#   - Positive: product-name is Norma OS.
#   - Positive: bundle identifier is com.norma.os.
#   - Positive: the main app binary path is target/release/norma-os.
#   - Positive: the hook binary path is present in [[binaries]].
#   - Positive: the OpenAI card planner helper is packaged as a resource.
#   - Positive: before-packaging-command exists and bumps the persisted version.
#   - Positive: before-packaging-command invokes `cargo build --release -p app`.
#   - Negative: the old target/release/multisoul main binary path is not retained.
#   - Negative: old Multisoul product metadata is not retained.
#   - Negative: a config that builds without bumping the version fails.
#   - Negative: a config that bumps after building fails.
#   - Positive: release and dev DMG configs use the Norma OS background and
#     matching window/icon positions.
#   - Positive: the 1024 app icon source has a tight alpha bbox so Finder and
#     DMG icons do not render visually undersized.
#   - Positive: the DMG background has a 2x Retina pixel buffer and 144 DPI
#     metadata, so Finder displays it sharply at 640x400 logical points.
#   - Negative: the app icon source must not contain near-white edge pixels
#     that show up as a strange border on dark backgrounds.
#   - Negative: the dev config must not fall back to cargo-packager's default
#     DMG layout.

set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
config="$repo_root/Packager.toml"
dev_config="$repo_root/Packager.dev.toml"
icon_source="$repo_root/assets/app-icon-source.png"
dmg_background="$repo_root/assets/dmg-background.png"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

if ! grep -Fq 'path = "target/release/norma-os"' "$config"; then
  fail "Packager.toml must package target/release/norma-os as the main app binary"
fi

if grep -Fq 'path = "target/release/multisoul"' "$config"; then
  fail "Packager.toml must not package the old target/release/multisoul app binary"
fi

if ! grep -Fq 'path = "target/release/multisoul-agent-hook"' "$config"; then
  fail "Packager.toml must package target/release/multisoul-agent-hook"
fi

for resource_config in "$config" "$dev_config"; do
  if ! grep -Fq '"sidecars/openai-card-planner/openai-card-planner.mjs"' "$resource_config"; then
    fail "$(basename "$resource_config") must package the OpenAI card planner helper resource"
  fi
done

if ! grep -Fq 'product-name = "Norma OS"' "$config"; then
  fail "Packager.toml product-name must be Norma OS"
fi

if ! grep -Fq 'identifier = "com.norma.os"' "$config"; then
  fail "Packager.toml identifier must be com.norma.os"
fi

if grep -Fq 'product-name = "Multisoul"' "$config"; then
  fail "Packager.toml must not keep the old Multisoul product-name"
fi

if ! grep -Fq 'before-packaging-command' "$config"; then
  fail "Packager.toml must build release binaries before packaging"
fi

hook=$(grep -F 'before-packaging-command' "$config")

if ! printf '%s\n' "$hook" | grep -Fq -- 'scripts/bump-release-version.sh'; then
  fail "before-packaging-command must bump the release version before packaging"
fi

if ! printf '%s\n' "$hook" | grep -Fq -- 'cargo build --release -p app'; then
  fail "before-packaging-command must build the app package release binaries"
fi

case "$hook" in
  *scripts/bump-release-version.sh*'cargo build --release -p app'*)
    ;;
  *)
    fail "before-packaging-command must bump the release version before cargo build"
    ;;
esac

# DMG visual/layout guard:
#   product name      = Norma OS
#   background path  = assets/dmg-background.png
#   background bitmap = 1280x800 @ 144 DPI (2x), which Finder displays as a
#                       sharp 640x400-point background instead of upscaling
#   app/drop targets = x=180 and x=460, centered on the left/right wells
#   dev config       = same [dmg] values, so local test packages match release
for cfg in "$config" "$dev_config"; do
  if ! grep -Fq '[dmg]' "$cfg"; then
    fail "$(basename "$cfg") must include a [dmg] section instead of using default DMG layout"
  fi

  if ! grep -Fq 'background = "assets/dmg-background.png"' "$cfg"; then
    fail "$(basename "$cfg") must use assets/dmg-background.png as the DMG background"
  fi

  if ! grep -Fq 'window-size = { width = 640, height = 400 }' "$cfg"; then
    fail "$(basename "$cfg") DMG window must stay 640x400 logical points"
  fi

  if ! grep -Fq 'app-position = { x = 180, y = 200 }' "$cfg"; then
    fail "$(basename "$cfg") must place the app icon at x=180 y=200"
  fi

  if ! grep -Fq 'application-folder-position = { x = 460, y = 200 }' "$cfg"; then
    fail "$(basename "$cfg") must place the Applications link at x=460 y=200"
  fi
done

background_width=$(sips -g pixelWidth "$dmg_background" | awk '/pixelWidth/ { print $2 }')
background_height=$(sips -g pixelHeight "$dmg_background" | awk '/pixelHeight/ { print $2 }')
background_geometry="${background_width}x${background_height}"
if [ "$background_geometry" != "1280x800" ]; then
  fail "assets/dmg-background.png must use a 1280x800 Retina pixel buffer, got $background_geometry"
fi

background_dpi_width=$(sips -g dpiWidth "$dmg_background" | awk '/dpiWidth/ { print $2 }')
background_dpi_height=$(sips -g dpiHeight "$dmg_background" | awk '/dpiHeight/ { print $2 }')
background_dpi="${background_dpi_width}x${background_dpi_height}"
if [ "$background_dpi" != "144.000x144.000" ]; then
  fail "assets/dmg-background.png must set 144 DPI Retina metadata, got $background_dpi"
fi

if ! command -v magick >/dev/null 2>&1; then
  fail "ImageMagick 'magick' is required to verify the app icon asset"
fi

# App icon whitespace guard:
#   image size        = 1024x1024
#   old alpha bbox    = 931x868+25+78, leaving visible transparent slack
#   required bbox     >= 950px wide and >= 900px tall after trimming/repadding
#   upper-left offset <= 45px on both axes, so the icon is optically centered
#   edge-white count  <= 16 pixels on each outer strip; larger counts indicate
#                       an antialiased white border or leftover drop shadow
icon_geometry=$(magick identify -format '%wx%h' "$icon_source")
if [ "$icon_geometry" != "1024x1024" ]; then
  fail "assets/app-icon-source.png must remain a 1024x1024 PNG, got $icon_geometry"
fi

icon_bbox=$(magick identify -format '%@' "$icon_source")
case "$icon_bbox" in
  *x*+*+*) ;;
  *) fail "could not read alpha bounding box for assets/app-icon-source.png: $icon_bbox" ;;
esac

bbox_w=${icon_bbox%%x*}
bbox_rest=${icon_bbox#*x}
bbox_h=${bbox_rest%%+*}
bbox_offsets=${bbox_rest#*+}
bbox_x=${bbox_offsets%%+*}
bbox_y=${bbox_offsets#*+}

if [ "$bbox_w" -lt 950 ]; then
  fail "assets/app-icon-source.png alpha bbox width must be at least 950px, got $bbox_w ($icon_bbox)"
fi

if [ "$bbox_h" -lt 900 ]; then
  fail "assets/app-icon-source.png alpha bbox height must be at least 900px, got $bbox_h ($icon_bbox)"
fi

if [ "$bbox_x" -gt 45 ] || [ "$bbox_y" -gt 45 ]; then
  fail "assets/app-icon-source.png alpha bbox must start within 45px of top-left, got +$bbox_x+$bbox_y ($icon_bbox)"
fi

for edge_spec in \
  "top 1024x36+0+0" \
  "bottom 1024x124+0+900" \
  "left 36x1024+0+0" \
  "right 36x1024+988+0"
do
  edge_name=${edge_spec%% *}
  edge_crop=${edge_spec#* }
  # Count visible near-white pixels only on the outer edge. This ignores the
  # interior white X and small square marks, but catches the previous white
  # fringe/drop-shadow around the rounded rectangle.
  edge_white=$(magick "$icon_source" -crop "$edge_crop" +repage -alpha on \
    -fx 'a>0.10 && r>0.82 && g>0.82 && b>0.82 ? 1 : 0' \
    -format '%[fx:mean*w*h]' info:)
  edge_white=${edge_white%.*}

  if [ "$edge_white" -gt 16 ]; then
    fail "assets/app-icon-source.png has $edge_white near-white pixels on the $edge_name edge; white border not removed"
  fi
done

echo "packager config test passed"
