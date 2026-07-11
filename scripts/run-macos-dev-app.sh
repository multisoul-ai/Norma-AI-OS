#!/usr/bin/env bash
# Run a Cargo-built macOS binary from a lightweight development .app bundle.
#
# TCC needs both the app bundle metadata and a LaunchServices application
# launch. `cargo run` normally executes target/debug/norma-os directly, which
# bypasses both and cannot show a reliable microphone permission prompt.

set -euo pipefail

if [ "$#" -lt 1 ]; then
  echo "usage: run-macos-dev-app.sh <compiled-binary> [args...]" >&2
  exit 64
fi

binary=$1
shift

# Cargo applies a target runner to test binaries too. Unit and integration tests
# do not need TCC bundle metadata, and bundling each test would make test runs
# unexpectedly mutate target/debug. Keep those binaries on Cargo's normal path.
case "$binary" in
  */deps/*)
    exec "$binary" "$@"
    ;;
esac

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
binary_dir=$(cd "$(dirname "$binary")" && pwd)
binary_name=$(basename "$binary")
bundle="$binary_dir/Norma OS Dev.app"
bundle_binary="$bundle/Contents/MacOS/$binary_name"
plist="$bundle/Contents/Info.plist"
entitlements="$repo_root/packaging/macos/NormaOS.entitlements"

mkdir -p "$bundle/Contents/MacOS"
cp "$binary" "$bundle_binary"
chmod u+x "$bundle_binary"
cp "$repo_root/packaging/macos/DevInfo.plist" "$plist"
/usr/libexec/PlistBuddy -c 'Delete :CFBundleExecutable' "$plist" >/dev/null 2>&1 || true
/usr/libexec/PlistBuddy -c "Add :CFBundleExecutable string $binary_name" "$plist"
codesign --force --sign - --entitlements "$entitlements" "$bundle" >/dev/null

# `open -W -n` registers this exact development bundle with LaunchServices and
# waits until it exits, preserving Cargo's foreground lifecycle. `-n` ensures a
# fresh process loads the just-written Info.plist and entitlement signature.
open -W -n "$bundle" --args "$@"
