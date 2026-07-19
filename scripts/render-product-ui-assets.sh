#!/usr/bin/env bash

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROTOTYPE="$ROOT/docs/product-ui.html"
OUTPUT_DIR="$ROOT/docs/design-docs/core-ui-v2"
THEME_DIR="$OUTPUT_DIR/themes"
CHROME="${CHROME:-$(find "$HOME/.cache/puppeteer/chrome-headless-shell" -type f -name chrome-headless-shell 2>/dev/null | sort -r | head -n 1)}"
WIDTH=1600
HEIGHT=1000

fail() {
  printf 'product-ui render failed: %s\n' "$1" >&2
  exit 1
}

render() {
  local route="$1"
  local theme="$2"
  local output="$3"
  local temporary="${output}.tmp.png"
  local url="file://$PROTOTYPE?clean=1&theme=$theme#$route"

  "$CHROME" \
    --headless \
    --disable-gpu \
    --hide-scrollbars \
    --no-first-run \
    --no-default-browser-check \
    --window-size="$WIDTH,$HEIGHT" \
    --screenshot="$temporary" \
    "$url" >/dev/null 2>&1

  mv "$temporary" "$output"
  printf 'rendered %s\n' "${output#$ROOT/}"
}

test -f "$PROTOTYPE" || fail "docs/product-ui.html is missing"
test -n "$CHROME" && test -x "$CHROME" || fail "Chrome Headless Shell is unavailable"

mkdir -p "$THEME_DIR"

render launchpad paper-blue "$OUTPUT_DIR/launchpad.png"
render canvas paper-blue "$OUTPUT_DIR/canvas.png"
render settings paper-blue "$OUTPUT_DIR/settings.png"

for theme in \
  paper-blue \
  linen-sage \
  silver-violet \
  sapphire-night \
  midnight-violet \
  forest-mint; do
  render canvas "$theme" "$THEME_DIR/$theme.png"
done

rm -f \
  "$THEME_DIR/ceramic-mist.png" \
  "$THEME_DIR/pearl-lilac.png" \
  "$THEME_DIR/sand-coral.png"

printf 'product-ui assets are up to date (%sx%s)\n' "$WIDTH" "$HEIGHT"
