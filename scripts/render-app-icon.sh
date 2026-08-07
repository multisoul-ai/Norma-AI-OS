#!/usr/bin/env bash
# Render the tightly cropped Norma artwork into the transparent macOS icon
# safe area, then build the ICNS consumed by cargo-packager.

set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
artwork_source="$repo_root/assets/app-icon-source.png"
macos_icon="$repo_root/assets/app-icon-macos.png"
output_icns="$repo_root/assets/AppIcon.icns"

magick_bin=$(command -v magick || true)
if [ -z "$magick_bin" ] && [ -x /opt/homebrew/bin/magick ]; then
  magick_bin=/opt/homebrew/bin/magick
fi

if [ -z "$magick_bin" ]; then
  echo "ImageMagick 'magick' is required to render the macOS app icon" >&2
  exit 1
fi

if ! command -v iconutil >/dev/null 2>&1; then
  echo "macOS 'iconutil' is required to build assets/AppIcon.icns" >&2
  exit 1
fi

iconset_root=$(mktemp -d "${TMPDIR:-/tmp}/norma-app-icon.XXXXXX")
trap 'rm -rf "$iconset_root"' EXIT
iconset_dir="$iconset_root/AppIcon.iconset"
temporary_macos_icon="$iconset_root/app-icon-macos.png"
temporary_icns="$iconset_root/AppIcon.icns"
mkdir -p "$iconset_dir"

# Apple-style macOS icon canvas:
#   canvas width          = 1024px
#   target visible width = 824px
#   horizontal inset     = (1024 - 824) / 2 = 100px
# The source is first trimmed to its real alpha bounds so stale source padding
# cannot silently change the packaged Dock icon's visual scale.
"$magick_bin" \
  "$artwork_source" \
  -alpha on \
  -trim \
  +repage \
  -resize 824x824 \
  -gravity center \
  -background none \
  -extent 1024x1024 \
  -define png:exclude-chunks=date,time \
  PNG32:"$temporary_macos_icon"

render_icon_size() {
  size=$1
  filename=$2
  "$magick_bin" \
    "$temporary_macos_icon" \
    -filter Lanczos \
    -resize "${size}x${size}!" \
    -define png:exclude-chunks=date,time \
    PNG32:"$iconset_dir/$filename"
}

render_icon_size 16 icon_16x16.png
render_icon_size 32 icon_16x16@2x.png
render_icon_size 32 icon_32x32.png
render_icon_size 64 icon_32x32@2x.png
render_icon_size 128 icon_128x128.png
render_icon_size 256 icon_128x128@2x.png
render_icon_size 256 icon_256x256.png
render_icon_size 512 icon_256x256@2x.png
render_icon_size 512 icon_512x512.png
cp "$temporary_macos_icon" "$iconset_dir/icon_512x512@2x.png"

iconutil -c icns "$iconset_dir" -o "$temporary_icns"
mv "$temporary_macos_icon" "$macos_icon"
mv "$temporary_icns" "$output_icns"

echo "Rendered $macos_icon"
echo "Rendered $output_icns"
