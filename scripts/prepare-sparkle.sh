#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
metadata="$repo_root/packaging/macos/sparkle-release.env"
output_dir="$repo_root/build/sparkle"
cache_dir="$repo_root/build/downloads"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

[ -f "$metadata" ] || fail "Sparkle metadata not found: $metadata"
# shellcheck source=/dev/null
source "$metadata"

: "${SPARKLE_VERSION:?missing SPARKLE_VERSION}"
: "${SPARKLE_ARCHIVE_URL:?missing SPARKLE_ARCHIVE_URL}"
: "${SPARKLE_ARCHIVE_SHA256:?missing SPARKLE_ARCHIVE_SHA256}"

archive="$cache_dir/Sparkle-$SPARKLE_VERSION.tar.xz"
mkdir -p "$cache_dir"

verify_archive() {
  printf '%s  %s\n' "$SPARKLE_ARCHIVE_SHA256" "$archive" | shasum -a 256 -c - >/dev/null
}

if [ -f "$archive" ] && ! verify_archive; then
  rm -f "$archive"
fi

if [ ! -f "$archive" ]; then
  archive_tmp=$(mktemp "$cache_dir/.Sparkle-$SPARKLE_VERSION.XXXXXX")
  trap 'rm -f "${archive_tmp:-}"' EXIT
  curl --fail --location --retry 3 --output "$archive_tmp" "$SPARKLE_ARCHIVE_URL"
  mv "$archive_tmp" "$archive"
  archive_tmp=""
fi

verify_archive || fail "Sparkle $SPARKLE_VERSION archive SHA-256 mismatch"

if tar -tf "$archive" | awk '/(^\/|(^|\/)\.\.($|\/))/ { unsafe = 1 } END { exit unsafe ? 0 : 1 }'; then
  fail "Sparkle archive contains an unsafe path"
fi

extract_dir=$(mktemp -d "${TMPDIR:-/tmp}/norma-sparkle.XXXXXX")
trap 'rm -rf "$extract_dir"; rm -f "${archive_tmp:-}"' EXIT
tar -xJf "$archive" -C "$extract_dir"

framework=$(find "$extract_dir" -type d -name Sparkle.framework -prune -print | head -n 1)
generate_appcast=$(find "$extract_dir" -type f -path '*/bin/generate_appcast' -print | head -n 1)
generate_keys=$(find "$extract_dir" -type f -path '*/bin/generate_keys' -print | head -n 1)
[ -n "$framework" ] || fail "Sparkle.framework not found in verified archive"
[ -n "$generate_appcast" ] || fail "generate_appcast not found in verified archive"
[ -n "$generate_keys" ] || fail "generate_keys not found in verified archive"

rm -rf "$output_dir"
mkdir -p "$output_dir/bin"
ditto "$framework" "$output_dir/Sparkle.framework"
ditto "$generate_appcast" "$output_dir/bin/generate_appcast"
ditto "$generate_keys" "$output_dir/bin/generate_keys"
chmod +x "$output_dir/bin/generate_appcast" "$output_dir/bin/generate_keys"
printf '%s\n' "$SPARKLE_ARCHIVE_SHA256" >"$output_dir/archive.sha256"

echo "prepared Sparkle $SPARKLE_VERSION in $output_dir"
