#!/usr/bin/env bash
# Render the editable DMG SVG into the Retina PNG consumed by cargo-packager.

set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
source_svg="$repo_root/assets/dmg-background.svg"
icon_source="$repo_root/assets/app-icon-source.png"
output_png="$repo_root/assets/dmg-background.png"

magick_bin=$(command -v magick || true)
if [ -z "$magick_bin" ] && [ -x /opt/homebrew/bin/magick ]; then
  magick_bin=/opt/homebrew/bin/magick
fi

if [ -z "$magick_bin" ]; then
  echo "ImageMagick 'magick' is required to render the DMG background" >&2
  exit 1
fi

temporary_png=$(mktemp "${TMPDIR:-/tmp}/norma-dmg-background.XXXXXX.png")
trap 'rm -f "$temporary_png"' EXIT

(
  cd "$repo_root/assets"
  "$magick_bin" \
    -background none \
    -density 96 \
    "$(basename "$source_svg")" \
    -resize 1280x800! \
    \( "$(basename "$icon_source")" -resize 102x102 \) \
    -geometry +430+66 \
    -composite \
    -units PixelsPerInch \
    -density 144 \
    -define png:exclude-chunks=date,time \
    PNG32:"$temporary_png"
)

geometry=$("$magick_bin" identify -format '%wx%h' "$temporary_png")
if [ "$geometry" != "1280x800" ]; then
  echo "Rendered DMG background must be 1280x800, got $geometry" >&2
  exit 1
fi

mv "$temporary_png" "$output_png"
trap - EXIT
echo "Rendered $output_png"
