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
#   app default features = []（普通构建不启用 License 硬门控）
#   release feature mode = --no-default-features + production-data
#   release profile      = thin LTO + 1 codegen unit + symbol stripping
#   wallpaper budget     = 140 MiB = 140 * 1024 * 1024 bytes
#
# Execution:
#   1. Read Packager.toml from the repository root.
#   2. Verify the secondary hook binary is listed as a packaged binary.
#   3. Verify the one-shot pre-packaging command builds the app package without License defaults.
#
# Expected result:
#   - Positive: product-name is Norma OS.
#   - Positive: bundle identifier is com.norma.os.
#   - Positive: the main app binary path is target/release/norma-os.
#   - Positive: the hook binary path is present in [[binaries]].
#   - Negative: the retired OpenAI card planner helper is not packaged.
#   - Positive: before-packaging-command validates the persisted release version without mutating it.
#   - Positive: before-packaging-command invokes `cargo build --release -p app`.
#   - Positive: normal and packaged builds both leave the License feature disabled.
#   - Negative: the packaging hook must not explicitly enable `licenses`.
#   - Positive: release binaries use size-conscious link/strip settings.
#   - Positive: packaged wallpaper sources stay within the 140 MiB budget.
#   - Negative: the old target/release/multisoul main binary path is not retained.
#   - Negative: old Multisoul product metadata is not retained.
#   - Negative: the packager hook must not bump manifests after cargo-packager parsed them.
#   - Positive: release and dev DMG configs use the Norma OS background and
#     matching window/icon positions, with the labels clear of the well border.
#   - Negative: the old y=200 icon positions that overlap the well border are
#     not retained.
#   - Positive: the 1024 brand artwork source stays tightly cropped for reuse
#     in product surfaces and the DMG header.
#   - Positive: the macOS app icon uses an approximately 824px visual safe area
#     inside its 1024px canvas so it matches neighboring Dock icons.
#   - Negative: the old nearly edge-to-edge macOS icon is not retained.
#   - Positive: the checked-in PNG and ICNS are reproducible from one renderer.
#   - Positive: the DMG background has a 2x Retina pixel buffer and 144 DPI
#     metadata, so Finder displays it sharply at 640x400 logical points.
#   - Positive: both Finder label landing zones use a light backing plate with
#     mean luminance >= 0.76, keeping Finder-owned dark labels readable.
#   - Positive: the checked-in SVG source and renderer make the visual asset
#     deterministic instead of leaving an untraceable hand-edited bitmap.
#   - Positive: the colorful 102x102 Retina header mark is present instead of
#     being silently dropped by an SVG renderer that blocks linked images.
#   - Negative: the app icon source must not contain near-white edge pixels
#     that show up as a strange border on dark backgrounds.
#   - Negative: the dev config must not fall back to cargo-packager's default
#     DMG layout.

set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
config="$repo_root/Packager.toml"
dev_config="$repo_root/Packager.dev.toml"
cargo_config="$repo_root/Cargo.toml"
app_manifest="$repo_root/crates/app/Cargo.toml"
icon_source="$repo_root/assets/app-icon-source.png"
macos_icon="$repo_root/assets/app-icon-macos.png"
app_icon="$repo_root/assets/AppIcon.icns"
app_icon_renderer="$repo_root/scripts/render-app-icon.sh"
dmg_background="$repo_root/assets/dmg-background.png"
dmg_background_source="$repo_root/assets/dmg-background.svg"
dmg_background_renderer="$repo_root/scripts/render-dmg-background.sh"
wallpaper_resources="$repo_root/crates/app/assets/wallpapers"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

if ! grep -Fq '[profile.release]' "$cargo_config"; then
  fail "Cargo.toml must define an explicit size-conscious release profile"
fi

for release_setting in 'lto = "thin"' 'codegen-units = 1' 'strip = "symbols"'; do
  if ! grep -Fq "$release_setting" "$cargo_config"; then
    fail "Cargo.toml release profile must include $release_setting"
  fi
done

# Current optimized sources total about 135.72 MiB. The 140 MiB ceiling leaves
# roughly 4.28 MiB for deliberate additions while protecting the package's
# 50%-reduction margin and rejecting the previous ~171 MiB intermediate payload.
wallpaper_budget_bytes=$((140 * 1024 * 1024))
wallpaper_bytes=$(find "$wallpaper_resources" -type f -exec stat -f '%z' {} + \
  | awk '{ total += $1 } END { print total + 0 }')
if (( wallpaper_bytes > wallpaper_budget_bytes )); then
  fail "packaged wallpapers use $wallpaper_bytes bytes; budget is $wallpaper_budget_bytes bytes"
fi

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
  if grep -Fq '"sidecars/openai-card-planner/openai-card-planner.mjs"' "$resource_config"; then
    fail "$(basename "$resource_config") must not package the retired OpenAI card planner helper"
  fi
  if ! grep -Fq '{ src = "build/release-media/feature-guide", target = "feature-guide" }' "$resource_config"; then
    fail "$(basename "$resource_config") must package verified Feature Guide staging media"
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

if ! printf '%s\n' "$hook" | grep -Fq -- 'scripts/release-version.sh'; then
  fail "before-packaging-command must verify the synchronized release version before packaging"
fi

if ! printf '%s\n' "$hook" | grep -Fq -- 'cargo build --release -p app'; then
  fail "before-packaging-command must build the app package release binaries"
fi

if ! grep -Fq 'default = []' "$app_manifest"; then
  fail "crates/app/Cargo.toml must keep default features empty so normal builds bypass License authorization"
fi

if ! printf '%s\n' "$hook" | grep -Fq -- '--no-default-features --features production-data'; then
  fail "before-packaging-command must explicitly disable default features while enabling production-data"
fi

if printf '%s\n' "$hook" | grep -Eq -- '--features(=|[[:space:]])[^&]*licenses'; then
  fail "before-packaging-command must not enable the licenses feature"
fi

if printf '%s\n' "$hook" | grep -Fq -- 'scripts/bump-release-version.sh'; then
  fail "before-packaging-command must not bump manifests after cargo-packager has parsed Packager.toml"
fi

# DMG visual/layout guard:
#   product name      = Norma OS
#   background path  = assets/dmg-background.png
#   background bitmap = 1280x800 @ 144 DPI (2x), which Finder displays as a
#                       sharp 640x400-point background instead of upscaling
#   app/drop targets = x=180 and x=460, centered on the left/right wells
#   vertical layout  = y=175; the previous y=200 placed Finder's 16pt labels
#                      across the well's bottom border. Moving both icons up by
#                      200 - 175 = 25pt preserves their horizontal alignment
#                      and leaves a clear label-to-border gap.
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

  if ! grep -Fq 'app-position = { x = 180, y = 175 }' "$cfg"; then
    fail "$(basename "$cfg") must place the app icon at x=180 y=175 so its label clears the well border"
  fi

  if ! grep -Fq 'application-folder-position = { x = 460, y = 175 }' "$cfg"; then
    fail "$(basename "$cfg") must place the Applications link at x=460 y=175 so its label clears the well border"
  fi

  if grep -Eq '(app-position|application-folder-position) = \{ x = (180|460), y = 200 \}' "$cfg"; then
    fail "$(basename "$cfg") must not retain y=200 because Finder labels overlap the well border there"
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

if [ ! -f "$dmg_background_source" ]; then
  fail "assets/dmg-background.svg must preserve the editable DMG design source"
fi

if [ ! -x "$dmg_background_renderer" ]; then
  fail "scripts/render-dmg-background.sh must be executable so the PNG can be reproduced"
fi

magick_bin=$(command -v magick || true)
if [ -z "$magick_bin" ] && [ -x /opt/homebrew/bin/magick ]; then
  magick_bin=/opt/homebrew/bin/magick
fi

if [ -z "$magick_bin" ]; then
  fail "ImageMagick 'magick' is required to verify the app icon asset"
fi

# Finder label contrast guard:
#   Retina background                    = 1280x800 px
#   left label plate                     = 248x56 px at +236+486
#   right label plate                    = 272x56 px at +784+486
#   required mean relative luminance     >= 0.76
#
# Execution:
#   1. Crop the two exact label landing zones used by Finder.
#   2. Convert each crop to grayscale and calculate its normalized mean.
#   3. Reject either zone if the backing plate is too dark for Finder's
#      system-owned black item labels.
#
# Expected result:
#   - Positive: both label zones remain visibly light.
#   - Negative: the previous charcoal label wells fail this threshold.
for label_spec in \
  "Norma OS 248x56+236+486" \
  "Applications 272x56+784+486"
do
  label_name=${label_spec% *}
  label_crop=${label_spec##* }
  label_luminance=$("$magick_bin" "$dmg_background" -crop "$label_crop" +repage \
    -colorspace Gray -format '%[fx:mean]' info:)

  if ! awk -v luminance="$label_luminance" 'BEGIN { exit !(luminance >= 0.76) }'; then
    fail "$label_name Finder label zone must have mean luminance >= 0.76, got $label_luminance"
  fi
done

# Header mark guard:
#   expected crop             = 102x102 px at +430+66
#   required mean saturation >= 0.28
# The empty backdrop in this crop is nearly monochrome (about 0.09 saturation),
# while the Norma icon is intentionally colorful. This gives the test a
# concrete positive threshold and catches the negative missing-image case.
header_mark_saturation=$("$magick_bin" "$dmg_background" -crop 102x102+430+66 +repage \
  -colorspace HSL -channel G -separate -format '%[fx:mean]' info:)
if ! awk -v saturation="$header_mark_saturation" 'BEGIN { exit !(saturation >= 0.28) }'; then
  fail "DMG header mark must be present with mean saturation >= 0.28, got $header_mark_saturation"
fi

# App icon artwork guard:
#   source image size  = 1024x1024
#   source alpha bbox  = 1010x942+7+41, intentionally tight for reuse in the
#                        DMG header and product surfaces
#   required bbox      >= 950px wide and >= 900px tall
#   upper-left offset  <= 45px on both axes
#   edge-white count   <= 16 pixels on each outer strip; larger counts indicate
#                        an antialiased white border or leftover drop shadow
icon_geometry=$("$magick_bin" identify -format '%wx%h' "$icon_source")
if [ "$icon_geometry" != "1024x1024" ]; then
  fail "assets/app-icon-source.png must remain a 1024x1024 PNG, got $icon_geometry"
fi

icon_bbox=$("$magick_bin" identify -format '%@' "$icon_source")
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
  edge_white=$("$magick_bin" "$icon_source" -crop "$edge_crop" +repage -alpha on \
    -fx 'a>0.10 && r>0.82 && g>0.82 && b>0.82 ? 1 : 0' \
    -format '%[fx:mean*w*h]' info:)
  edge_white=${edge_white%.*}

  if [ "$edge_white" -gt 16 ]; then
    fail "assets/app-icon-source.png has $edge_white near-white pixels on the $edge_name edge; white border not removed"
  fi
done

# macOS Dock icon safe-area guard:
#
# Scenario:
#   The tightly cropped 1010px-wide brand artwork previously went straight into
#   AppIcon.icns. Dock therefore rendered Norma OS about 1010 / 824 = 1.23x the
#   visual width of neighboring icons.
#
# Data construction (including the key numeric derivation):
#   macOS canvas             = 1024x1024 px
#   target visible width     = 824 px
#   source visible bbox      = 1010x942 px
#   scale                    = 824 / 1010 ≈ 0.816
#   expected visible height  = round(942 * 0.816) ≈ 769 px
#   expected horizontal inset = (1024 - 824) / 2 = 100 px
#   expected vertical inset   = (1024 - 769) / 2 ≈ 127.5 px
#
# Execution:
#   1. Verify the deterministic renderer and its PNG/ICNS outputs exist.
#   2. Measure the macOS PNG's alpha bounding box.
#   3. Verify the visual bounds are centered and stay near 824x769.
#   4. Extract the 1024px representation from ICNS and compare it pixel-for-pixel
#      with the checked-in macOS PNG (iconutil may rewrite PNG metadata).
#
# Expected result:
#   - Positive: visible width is 820..828px and height is 765..773px.
#   - Positive: opposite transparent insets differ by no more than 2px.
#   - Positive: ICNS contains the exact checked-in 1024px macOS PNG.
#   - Negative: a nearly edge-to-edge icon wider than 828px must fail.
if [ ! -x "$app_icon_renderer" ]; then
  fail "scripts/render-app-icon.sh must be executable so the macOS icon can be reproduced"
fi

if [ ! -f "$macos_icon" ]; then
  fail "assets/app-icon-macos.png must exist as the reviewable 1024px macOS icon"
fi

if [ ! -f "$app_icon" ]; then
  fail "assets/AppIcon.icns must exist for cargo-packager"
fi

macos_icon_geometry=$("$magick_bin" identify -format '%wx%h' "$macos_icon")
if [ "$macos_icon_geometry" != "1024x1024" ]; then
  fail "assets/app-icon-macos.png must remain a 1024x1024 PNG, got $macos_icon_geometry"
fi

macos_icon_bbox=$("$magick_bin" identify -format '%@' "$macos_icon")
case "$macos_icon_bbox" in
  *x*+*+*) ;;
  *) fail "could not read alpha bounding box for assets/app-icon-macos.png: $macos_icon_bbox" ;;
esac

macos_bbox_w=${macos_icon_bbox%%x*}
macos_bbox_rest=${macos_icon_bbox#*x}
macos_bbox_h=${macos_bbox_rest%%+*}
macos_bbox_offsets=${macos_bbox_rest#*+}
macos_bbox_x=${macos_bbox_offsets%%+*}
macos_bbox_y=${macos_bbox_offsets#*+}
macos_bbox_right=$((1024 - macos_bbox_x - macos_bbox_w))
macos_bbox_bottom=$((1024 - macos_bbox_y - macos_bbox_h))

if [ "$macos_bbox_w" -lt 820 ] || [ "$macos_bbox_w" -gt 828 ]; then
  fail "assets/app-icon-macos.png visible width must be 820..828px, got $macos_bbox_w ($macos_icon_bbox)"
fi

if [ "$macos_bbox_h" -lt 765 ] || [ "$macos_bbox_h" -gt 773 ]; then
  fail "assets/app-icon-macos.png visible height must be 765..773px, got $macos_bbox_h ($macos_icon_bbox)"
fi

if [ $((macos_bbox_x - macos_bbox_right)) -lt -2 ] || [ $((macos_bbox_x - macos_bbox_right)) -gt 2 ]; then
  fail "assets/app-icon-macos.png must be horizontally centered, got left=$macos_bbox_x right=$macos_bbox_right ($macos_icon_bbox)"
fi

if [ $((macos_bbox_y - macos_bbox_bottom)) -lt -2 ] || [ $((macos_bbox_y - macos_bbox_bottom)) -gt 2 ]; then
  fail "assets/app-icon-macos.png must be vertically centered, got top=$macos_bbox_y bottom=$macos_bbox_bottom ($macos_icon_bbox)"
fi

icon_extract_dir=$(mktemp -d "${TMPDIR:-/tmp}/norma-app-icon-test.XXXXXX")
trap 'rm -rf "$icon_extract_dir"' EXIT
iconutil -c iconset "$app_icon" -o "$icon_extract_dir/AppIcon.iconset"
extracted_icon="$icon_extract_dir/AppIcon.iconset/icon_512x512@2x.png"

if [ ! -f "$extracted_icon" ]; then
  fail "assets/AppIcon.icns must contain a 1024px icon_512x512@2x.png representation"
fi

icon_pixel_difference=$("$magick_bin" compare -metric AE \
  "$macos_icon" "$extracted_icon" null: 2>&1 || true)
if [ "$icon_pixel_difference" != "0 (0)" ]; then
  fail "assets/AppIcon.icns 1024px representation must match assets/app-icon-macos.png pixel-for-pixel, got $icon_pixel_difference differing pixels"
fi

rm -rf "$icon_extract_dir"
trap - EXIT

echo "packager config test passed"
