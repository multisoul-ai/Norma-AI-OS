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
cli_binary="$binary_dir/norma"
bundle_cli="$bundle/Contents/MacOS/norma"
hook_binary="$binary_dir/multisoul-agent-hook"
bundle_hook="$bundle/Contents/MacOS/multisoul-agent-hook"
auth_host_binary="$binary_dir/norma-browser-auth-host"
bundle_auth_host="$bundle/Contents/MacOS/norma-browser-auth-host"
media_resources="$repo_root/build/release-media"
wallpaper_resources="$media_resources/wallpapers"
feature_guide_resources="$media_resources/feature-guide"
browser_auth_extension="$repo_root/crates/app/assets/browser-auth-extension"
bundle_resources="$bundle/Contents/Resources"

# `open -n` is required to load the newly built signed bundle, but it would
# otherwise create another process when an older cargo-run App is still alive.
# Two Dev processes share the Norma CLI socket and make the active project
# nondeterministic, so require the previous foreground run to exit first.
existing_dev_pids=$(pgrep -f "$bundle_binary" || true)
if [ -n "$existing_dev_pids" ]; then
  echo "Norma OS Dev is already running. Close the existing cargo run before starting another." >&2
  while IFS= read -r existing_dev_pid; do
    [ -n "$existing_dev_pid" ] || continue
    echo "Existing Norma OS Dev PID: $existing_dev_pid" >&2
  done <<<"$existing_dev_pids"
  exit 75
fi

# TCC keys folder consent to the app's designated requirement. An ad-hoc
# signature uses the binary CDHash as that requirement, so every Cargo rebuild
# becomes a new app in TCC and repeatedly asks for Documents access. Prefer an
# explicit identity for reproducibility; otherwise enumerate Apple Development
# certificates whose display names occur only once in the Keychain, returning
# their unique SHA-1 identity hashes. Repeated display names can map to
# inaccessible keys and must be selected explicitly by hash.
resolve_dev_signing_identities() {
  if [ -n "${MULTISOUL_DEV_CODESIGN_IDENTITY:-}" ]; then
    printf '%s\n' "$MULTISOUL_DEV_CODESIGN_IDENTITY"
    return
  fi

  security find-identity -v -p codesigning 2>/dev/null \
    | awk '
        /"Apple Development: / {
          hash = $2
          label = $0
          sub(/^[^"]*"/, "", label)
          sub(/".*$/, "", label)
          hashes[++count] = hash
          labels[count] = label
          label_counts[label] += 1
        }
        END {
          for (position = 1; position <= count; position += 1) {
            if (label_counts[labels[position]] == 1) {
              print hashes[position]
            }
          }
        }
      '
}

sign_development_bundle() {
  local identity
  local attempted=false
  while IFS= read -r identity; do
    [ -n "$identity" ] || continue
    attempted=true
    if codesign --force --sign "$identity" --entitlements "$entitlements" "$bundle" >/dev/null 2>&1; then
      printf '%s\n' "$identity"
      return 0
    fi
    echo "Skipping unusable development signing identity: $identity" >&2
  done < <(resolve_dev_signing_identities)

  if [ "$attempted" = false ]; then
    echo "No Apple Development signing identity is available for Norma OS Dev." >&2
  else
    echo "No listed Apple Development signing identity could sign Norma OS Dev." >&2
  fi
  echo "Set MULTISOUL_DEV_CODESIGN_IDENTITY to a usable SHA-1 identity from:" >&2
  echo "  security find-identity -v -p codesigning" >&2
  echo "Refusing ad-hoc signing because it causes macOS to re-prompt for Documents access after rebuilds." >&2
  return 1
}

# `cargo run -p app` only builds the selected norma-os binary. Installers
# resolve their CLI and bridges as siblings of the running executable, so the
# development bundle must explicitly build and carry each companion binary.
if [ "$binary_name" = "norma-os" ] && [ ! -x "$cli_binary" ]; then
  cargo build --quiet --manifest-path "$repo_root/Cargo.toml" \
    --package norma --bin norma
fi

if [ "$binary_name" = "norma-os" ] && { [ ! -x "$hook_binary" ] || [ ! -x "$auth_host_binary" ]; }; then
  cargo build --quiet --manifest-path "$repo_root/crates/app/Cargo.toml" \
    --bin multisoul-agent-hook --bin norma-browser-auth-host
fi

# A previous Cargo target can leave a differently signed executable in the
# shared development bundle. Rebuild from an empty bundle so codesign never
# sees stale nested code whose sealed Info.plist belongs to another binary.
if [ "$binary_name" = "norma-os" ]; then
  (cd "$repo_root" && scripts/prepare-release-media.sh --local-test)
fi
rm -rf "$bundle"
mkdir -p "$bundle/Contents/MacOS"
cp "$binary" "$bundle_binary"
chmod u+x "$bundle_binary"
if [ "$binary_name" = "norma-os" ]; then
  if [ ! -x "$cli_binary" ]; then
    echo "norma build did not produce $cli_binary" >&2
    exit 1
  fi
  cp "$cli_binary" "$bundle_cli"
  chmod u+x "$bundle_cli"
  if [ ! -x "$hook_binary" ]; then
    echo "multisoul-agent-hook build did not produce $hook_binary" >&2
    exit 1
  fi
  cp "$hook_binary" "$bundle_hook"
  chmod u+x "$bundle_hook"
  if [ ! -x "$auth_host_binary" ]; then
    echo "norma-browser-auth-host build did not produce $auth_host_binary" >&2
    exit 1
  fi
  cp "$auth_host_binary" "$bundle_auth_host"
  chmod u+x "$bundle_auth_host"
  if [ ! -d "$wallpaper_resources" ]; then
    echo "built-in wallpaper resources are missing: $wallpaper_resources" >&2
    exit 1
  fi
  mkdir -p "$bundle_resources"
  cp -R "$repo_root/packaging/macos/en.lproj" "$bundle_resources/"
  cp -R "$repo_root/packaging/macos/zh-CN.lproj" "$bundle_resources/"
  cp -R "$repo_root/crates/app/assets/home" "$bundle_resources/"
  cp -R "$wallpaper_resources" "$bundle_resources/"
  if [ ! -d "$feature_guide_resources" ]; then
    echo "Feature Guide resources are missing: $feature_guide_resources" >&2
    exit 1
  fi
  cp -R "$feature_guide_resources" "$bundle_resources/"
  cp -R "$media_resources/wallpaper-previews" "$bundle_resources/"
  cp "$media_resources/media-catalog.json" "$bundle_resources/"
  if [ ! -f "$browser_auth_extension/manifest.json" ]; then
    echo "Chrome authorization extension is missing: $browser_auth_extension" >&2
    exit 1
  fi
  cp -R "$browser_auth_extension" "$bundle_resources/"
fi
cp "$repo_root/packaging/macos/DevInfo.plist" "$plist"
/usr/libexec/PlistBuddy -c 'Delete :CFBundleExecutable' "$plist" >/dev/null 2>&1 || true
/usr/libexec/PlistBuddy -c "Add :CFBundleExecutable string $binary_name" "$plist"
signing_identity=$(sign_development_bundle) || exit 69
echo "Norma OS Dev signed with identity: $signing_identity" >&2

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
