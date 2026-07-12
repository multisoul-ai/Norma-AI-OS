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
hook_binary="$binary_dir/multisoul-agent-hook"
bundle_hook="$bundle/Contents/MacOS/multisoul-agent-hook"
wallpaper_resources="$repo_root/crates/app/assets/wallpapers"
bundle_resources="$bundle/Contents/Resources"

# `cargo run -p app` only builds the selected norma-os binary. The hook
# installer resolves its bridge as a sibling of the running executable, so the
# development bundle must explicitly build and carry the helper as well.
if [ "$binary_name" = "norma-os" ] && [ ! -x "$hook_binary" ]; then
  cargo build --quiet --manifest-path "$repo_root/crates/app/Cargo.toml" --bin multisoul-agent-hook
fi

# A previous Cargo target can leave a differently signed executable in the
# shared development bundle. Rebuild from an empty bundle so codesign never
# sees stale nested code whose sealed Info.plist belongs to another binary.
rm -rf "$bundle"
mkdir -p "$bundle/Contents/MacOS"
cp "$binary" "$bundle_binary"
chmod u+x "$bundle_binary"
if [ "$binary_name" = "norma-os" ]; then
  if [ ! -x "$hook_binary" ]; then
    echo "multisoul-agent-hook build did not produce $hook_binary" >&2
    exit 1
  fi
  cp "$hook_binary" "$bundle_hook"
  chmod u+x "$bundle_hook"
  if [ ! -d "$wallpaper_resources" ]; then
    echo "built-in wallpaper resources are missing: $wallpaper_resources" >&2
    exit 1
  fi
  mkdir -p "$bundle_resources"
  cp -R "$wallpaper_resources" "$bundle_resources/"
fi
cp "$repo_root/packaging/macos/DevInfo.plist" "$plist"
/usr/libexec/PlistBuddy -c 'Delete :CFBundleExecutable' "$plist" >/dev/null 2>&1 || true
/usr/libexec/PlistBuddy -c "Add :CFBundleExecutable string $binary_name" "$plist"
codesign --force --sign - --entitlements "$entitlements" "$bundle" >/dev/null

# `open -W -n` registers this exact development bundle with LaunchServices and
# waits until it exits, preserving Cargo's foreground lifecycle. `-n` ensures a
# fresh process loads the just-written Info.plist and entitlement signature.
# LaunchServices rejects /dev/stdout and defaults to /dev/null, so bridge both
# output streams through named pipes before forwarding them to Cargo/`tee`.
stdio_dir=$(mktemp -d "${TMPDIR:-/tmp}/norma-os-dev-stdio.XXXXXX")
stdout_pipe="$stdio_dir/stdout"
stderr_pipe="$stdio_dir/stderr"
mkfifo "$stdout_pipe" "$stderr_pipe"

stdout_reader=
stderr_reader=
open_pid=
launched_app_pid=
known_app_pids=$(pgrep -f "$bundle_binary" || true)

remember_launched_app() {
  local pid
  while IFS= read -r pid; do
    [ -n "$pid" ] || continue
    case " $known_app_pids " in
      *" $pid "*) ;;
      *)
        launched_app_pid=$pid
        return
        ;;
    esac
  done < <(pgrep -f "$bundle_binary" || true)
}

terminate_launched_app() {
  if [ -z "$launched_app_pid" ]; then
    # LaunchServices starts the .app asynchronously. Give a just-starting app
    # a short chance to appear before the terminal signal is forwarded.
    for _ in {1..10}; do
      remember_launched_app
      [ -n "$launched_app_pid" ] && break
      sleep 0.1
    done
  fi

  if [ -n "$launched_app_pid" ] && kill -0 "$launched_app_pid" >/dev/null 2>&1; then
    kill -TERM "$launched_app_pid" >/dev/null 2>&1 || true
  fi
}

cleanup_done=
cleanup() {
  if [ "$cleanup_done" = true ]; then
    return
  fi
  cleanup_done=true

  terminate_launched_app
  if [ -n "$open_pid" ] && kill -0 "$open_pid" >/dev/null 2>&1; then
    kill -TERM "$open_pid" >/dev/null 2>&1 || true
  fi
  if [ -n "$stdout_reader" ]; then kill "$stdout_reader" >/dev/null 2>&1 || true; fi
  if [ -n "$stderr_reader" ]; then kill "$stderr_reader" >/dev/null 2>&1 || true; fi
  rm -rf "$stdio_dir"
}
trap cleanup EXIT INT TERM

cat "$stdout_pipe" &
stdout_reader=$!
cat "$stderr_pipe" >&2 &
stderr_reader=$!

set +e
open -W -n -o "$stdout_pipe" --stderr "$stderr_pipe" "$bundle" --args "$@" &
open_pid=$!
remember_launched_app
wait "$open_pid"
open_status=$?
set -e

if [ "$open_status" -ne 0 ]; then
  kill "$stdout_reader" "$stderr_reader" >/dev/null 2>&1 || true
fi
wait "$stdout_reader" >/dev/null 2>&1 || true
wait "$stderr_reader" >/dev/null 2>&1 || true
exit "$open_status"
