#!/usr/bin/env bash
# Focused contract test for the macOS Cargo development app runner.
#
# Scenario:
#   `cargo run -p app` must execute norma-os from a temporary .app bundle so
#   macOS can read NSMicrophoneUsageDescription and present the TCC prompt.
#
# Data construction:
#   cargo config   = .cargo/config.toml
#   runner script  = scripts/run-macos-dev-app.sh
#   dev plist      = packaging/macos/DevInfo.plist
#   bundle layout  = <target-dir>/Norma OS Dev.app/Contents/{MacOS,Info.plist}
#   orchestration CLI = <target-dir>/norma, built by the norma crate
#   hook helper    = <target-dir>/multisoul-agent-hook, built by the app crate
#   auth helper    = <target-dir>/norma-browser-auth-host, built by the app crate
#   auth extension = crates/app/assets/browser-auth-extension → bundle Resources
#   wallpaper      = crates/app/assets/wallpapers/1407174.mp4 → bundle Resources
#   feature guide  = crates/app/assets/feature-guide/demo-live-node-shortcuts.mp4
#                    → bundle Resources
#   signal fixture = fake `open -W` that starts an independently living app
#                    process, mirroring LaunchServices parent detachment.
#
# Execution:
#   1. Check Cargo's native macOS target runner configuration.
#   2. Check the runner materializes an app bundle from the compiled binary.
#   3. Check the runner clears stale bundle contents before copying the binary.
#   4. Check the runner installs the development microphone usage description.
#   5. Check the runner copies the sibling orchestration CLI into the same app bundle.
#   6. Check the runner copies the sibling hook helper into the same app bundle.
#   7. Check the runner copies the Chrome authorization helper and extension.
#   8. Check the runner copies built-in wallpaper resources into the app bundle.
#   9. Check the LaunchServices process forwards stdout/stderr to the caller.
#   10. Check the runner executes the binary from inside that bundle.
#   11. Check Ctrl+C / terminal termination also stops the detached app process.
#   12. Check a second cargo run refuses to start while the same Dev App is alive.
#
# Expected result:
#   - Positive: aarch64 macOS cargo runs use the development app runner.
#   - Positive: the runner copies DevInfo.plist into Contents/Info.plist.
#   - Positive: the runner executes Contents/MacOS/<binary>.
#   - Positive: the bundle contains executable Contents/MacOS/norma.
#   - Positive: the bundle contains Contents/MacOS/multisoul-agent-hook.
#   - Positive: the bundle contains Contents/MacOS/norma-browser-auth-host.
#   - Positive: the bundle contains Contents/Resources/browser-auth-extension/manifest.json.
#   - Positive: the bundle contains Contents/Resources/wallpapers/1407174.mp4.
#   - Positive: the bundle contains Contents/Resources/feature-guide/demo-live-node-shortcuts.mp4.
#   - Positive: tracing output remains visible to terminal pipes such as `tee`.
#   - Positive: INT and TERM terminate the specific app process launched by this runner.
#   - Positive: the runner selects a unique-label SHA-1 identity when Apple Development labels duplicate.
#   - Positive: a repeated cargo run reports the existing Dev App PID and exits immediately.
#   - Negative: stale executables must not survive into a newly signed bundle.
#   - Negative: orchestration installation must not fail because cargo run omitted the Norma CLI.
#   - Negative: the hook installer must not fail because cargo run omitted the helper binary.
#   - Negative: Chrome authorization must not fail because the dev app omitted its helper or extension.
#   - Negative: a development .app must not read built-in videos from the source checkout.
#   - Negative: cargo run must not keep launching only target/debug/norma-os.
#   - Negative: LaunchServices must not leave app stdout/stderr on `/dev/null`.
#   - Negative: stopping Cargo must not leave Norma OS Dev visible after its runner exits.
#   - Negative: the runner must not fall back to an ad-hoc signature, because its CDHash changes
#               after every rebuild and invalidates macOS TCC folder authorization.
#   - Negative: the runner must not pass an ambiguous certificate display name to codesign.
#   - Negative: an ambiguous duplicate-label identity must not be passed to codesign automatically.
#   - Negative: cargo run must not create two Dev App processes that compete for the Norma socket.

set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
config="$repo_root/.cargo/config.toml"
runner="$repo_root/scripts/run-macos-dev-app.sh"
dev_plist="$repo_root/packaging/macos/DevInfo.plist"
entitlements="$repo_root/packaging/macos/NormaOS.entitlements"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

if ! grep -Fq '[target.aarch64-apple-darwin]' "$config"; then
  fail "Cargo config must register an aarch64 macOS target section for the dev app runner"
fi

if ! grep -Fq 'runner = "scripts/run-macos-dev-app.sh"' "$config"; then
  fail "cargo run on macOS must invoke scripts/run-macos-dev-app.sh"
fi

if [ ! -f "$runner" ]; then
  fail "macOS development app runner is missing: $runner"
fi

if [ ! -x "$runner" ]; then
  fail "macOS development app runner must be executable by Cargo"
fi

if ! /usr/libexec/PlistBuddy -c 'Print :CFBundleDisplayName' "$dev_plist" 2>/dev/null | grep -Fxq 'Norma OS Dev'; then
  fail "development bundle must display as Norma OS Dev so its microphone permission entry is distinguishable from /Applications/Norma OS.app"
fi

if ! /usr/libexec/PlistBuddy -c 'Print :com.apple.security.device.audio-input' "$entitlements" 2>/dev/null | grep -Fxq 'true'; then
  fail "development app entitlements must grant com.apple.security.device.audio-input for macOS microphone access"
fi

if ! grep -Fq -- '--entitlements "$entitlements"' "$runner"; then
  fail "development app runner must sign its main bundle with microphone entitlements"
fi

if ! grep -Fq 'MULTISOUL_DEV_CODESIGN_IDENTITY' "$runner"; then
  fail "development app runner must accept a stable development signing identity override"
fi

if grep -Fq 'codesign --force --sign - ' "$runner"; then
  fail "development app runner must not use an ad-hoc signature that changes TCC identity on rebuild"
fi

if ! grep -Fq '*/deps/*' "$runner"; then
  fail "dev app runner must leave Cargo test executables outside the development app bundle"
fi

if ! grep -Fq 'packaging/macos/DevInfo.plist' "$runner"; then
  fail "dev app runner must install DevInfo.plist with microphone usage text"
fi

if ! grep -Fq 'Norma OS Dev.app' "$runner"; then
  fail "dev app runner must materialize a dedicated Norma OS Dev.app bundle"
fi

if ! grep -Fq 'rm -rf "$bundle"' "$runner"; then
  fail "dev app runner must clear stale bundle contents before signing a new executable"
fi

if ! grep -Fq 'Contents/MacOS' "$runner"; then
  fail "dev app runner must place the executable under Contents/MacOS"
fi

if ! grep -Fq 'Delete :CFBundleExecutable' "$runner"; then
  fail "dev app runner must replace CFBundleExecutable on repeated cargo runs"
fi

if ! grep -Fq 'open -W -n ' "$runner"; then
  fail "dev app runner must use LaunchServices to start the bundled app so macOS can present TCC authorization"
fi

if ! grep -Fq 'mkfifo "$stdout_pipe" "$stderr_pipe"' "$runner"; then
  fail "dev app runner must create FIFO bridges for LaunchServices output"
fi

if ! grep -Fq -- '-o "$stdout_pipe" --stderr "$stderr_pipe"' "$runner"; then
  fail "dev app runner must connect LaunchServices stdout/stderr to FIFO bridges"
fi

if ! grep -Fq 'terminate_launched_app' "$runner"; then
  fail "dev app runner must define a dedicated cleanup path for the LaunchServices app process"
fi

if ! grep -Fq 'trap cleanup EXIT INT TERM' "$runner"; then
  fail "dev app runner must invoke app cleanup for normal exit, Ctrl+C, and terminal termination"
fi

if ! grep -Fq 'kill -TERM "$launched_app_pid"' "$runner"; then
  fail "dev app runner must send TERM to the app process it launched instead of only stopping open -W"
fi

# Runner signal propagation:
#   fake `open -W` = starts test-input-binary, then waits without forwarding TERM;
#   launched app   = long-lived bash process recorded in app.pid;
#   test signal    = TERM sent to the runner, the non-interactive equivalent of
#                    terminal shutdown;
#   expectation    = the runner identifies the new app PID and terminates it.
tmp_dir=$(mktemp -d)
runner_pid=
app_pid=
collision_runner_pid=
cleanup_fixture() {
  if [ -n "$runner_pid" ]; then kill -TERM "$runner_pid" >/dev/null 2>&1 || true; fi
  if [ -n "$collision_runner_pid" ]; then
    kill -TERM "$collision_runner_pid" >/dev/null 2>&1 || true
  fi
  if [ -n "$app_pid" ]; then kill -TERM "$app_pid" >/dev/null 2>&1 || true; fi
  rm -rf "$tmp_dir"
}
trap cleanup_fixture EXIT

fixture_root="$tmp_dir/fixture"
fixture_bin="$fixture_root/bin"
fixture_runner="$fixture_root/scripts/run-macos-dev-app.sh"
fixture_input="$tmp_dir/norma-os"
fixture_cli_source="$tmp_dir/norma-source"
fixture_cli="$tmp_dir/norma"
fixture_hook_source="$tmp_dir/multisoul-agent-hook-source"
fixture_hook="$tmp_dir/multisoul-agent-hook"
fixture_auth_host_source="$tmp_dir/norma-browser-auth-host-source"
fixture_auth_host="$tmp_dir/norma-browser-auth-host"
app_pid_file="$tmp_dir/app.pid"
cargo_args_file="$tmp_dir/cargo-args"
codesign_args_file="$tmp_dir/codesign-args"
fixture_wallpaper_source="$fixture_root/crates/app/assets/wallpapers/1407174.mp4"
fixture_feature_guide_source="$fixture_root/crates/app/assets/feature-guide/demo-live-node-shortcuts.mp4"
fixture_auth_extension_manifest="$fixture_root/crates/app/assets/browser-auth-extension/manifest.json"
mkdir -p "$fixture_bin" "$fixture_root/scripts" "$fixture_root/packaging/macos" \
  "$(dirname "$fixture_wallpaper_source")" "$(dirname "$fixture_feature_guide_source")" \
  "$(dirname "$fixture_auth_extension_manifest")"
cp "$runner" "$fixture_runner"
cp "$dev_plist" "$fixture_root/packaging/macos/DevInfo.plist"
cp -R "$repo_root/packaging/macos/en.lproj" "$fixture_root/packaging/macos/"
cp -R "$repo_root/packaging/macos/zh-CN.lproj" "$fixture_root/packaging/macos/"
cp "$entitlements" "$fixture_root/packaging/macos/NormaOS.entitlements"
printf 'fixture bundle wallpaper\n' >"$fixture_wallpaper_source"
printf 'fixture feature guide video\n' >"$fixture_feature_guide_source"
printf '{"name":"fixture-browser-auth-extension"}\n' >"$fixture_auth_extension_manifest"

# 场景：runner 使用签名前的同一 staging 契约。此 fixture 只模拟准备器输出，
# 真正的 hash、profile 与事务行为由 test-prepare-release-media.py 覆盖。
cat >"$fixture_root/scripts/prepare-release-media.sh" <<'MEDIA'
#!/usr/bin/env bash
set -euo pipefail
[ "${1:-}" = --local-test ]
mkdir -p build/release-media/wallpaper-previews
cp -R crates/app/assets/wallpapers build/release-media/
cp -R crates/app/assets/feature-guide build/release-media/
printf 'trusted fixture catalog\n' >build/release-media/media-catalog.json
printf 'static fixture preview\n' >build/release-media/wallpaper-previews/optional.webp
MEDIA
chmod +x "$fixture_root/scripts/prepare-release-media.sh"

cat >"$fixture_input" <<'APP'
#!/usr/bin/env bash
trap 'exit 0' TERM
while true; do sleep 1; done
APP
chmod +x "$fixture_input"

cat >"$fixture_cli_source" <<'CLI'
#!/usr/bin/env bash
exit 0
CLI
chmod +x "$fixture_cli_source"

cat >"$fixture_hook_source" <<'HOOK'
#!/usr/bin/env bash
exit 0
HOOK
chmod +x "$fixture_hook_source"

cat >"$fixture_auth_host_source" <<'AUTH_HOST'
#!/usr/bin/env bash
exit 0
AUTH_HOST
chmod +x "$fixture_auth_host_source"

cat >"$fixture_bin/cargo" <<'CARGO'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"$FAKE_CARGO_ARGS_FILE"
case "$*" in
  *"--package norma --bin norma"*)
    cp "$FAKE_CLI_SOURCE" "$FAKE_CLI_DEST"
    chmod +x "$FAKE_CLI_DEST"
    ;;
esac
case "$*" in
  *"--bin multisoul-agent-hook --bin norma-browser-auth-host"*)
    cp "$FAKE_HOOK_SOURCE" "$FAKE_HOOK_DEST"
    chmod +x "$FAKE_HOOK_DEST"
    cp "$FAKE_AUTH_HOST_SOURCE" "$FAKE_AUTH_HOST_DEST"
    chmod +x "$FAKE_AUTH_HOST_DEST"
    ;;
esac
CARGO
chmod +x "$fixture_bin/cargo"

cat >"$fixture_bin/codesign" <<'CODESIGN'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$FAKE_CODESIGN_ARGS_FILE"
exit 0
CODESIGN
chmod +x "$fixture_bin/codesign"

cat >"$fixture_bin/security" <<'SECURITY'
#!/usr/bin/env bash
cat <<'IDENTITIES'
  1) 1111111111111111111111111111111111111111 "Apple Development: Fixture (TEAMID)"
  2) 2222222222222222222222222222222222222222 "Apple Development: Fixture (TEAMID)"
  3) 3333333333333333333333333333333333333333 "Apple Development: Fixture Alternate (OTHERTEAM)"
IDENTITIES
SECURITY
chmod +x "$fixture_bin/security"

cat >"$fixture_bin/open" <<'OPEN'
#!/usr/bin/env bash
set -euo pipefail

bundle=
for argument in "$@"; do
  case "$argument" in
    *.app) bundle=$argument ;;
  esac
done

if [ -z "$bundle" ]; then
  echo "fake open did not receive an app bundle" >&2
  exit 64
fi

"$bundle/Contents/MacOS/norma-os" &
app_pid=$!
printf '%s\n' "$app_pid" >"$OPEN_APP_PID_FILE"
trap 'exit 0' INT TERM
wait "$app_pid"
OPEN
chmod +x "$fixture_bin/open"

PATH="$fixture_bin:$PATH" OPEN_APP_PID_FILE="$app_pid_file" \
  FAKE_CLI_SOURCE="$fixture_cli_source" FAKE_CLI_DEST="$fixture_cli" \
  FAKE_HOOK_SOURCE="$fixture_hook_source" FAKE_HOOK_DEST="$fixture_hook" \
  FAKE_AUTH_HOST_SOURCE="$fixture_auth_host_source" FAKE_AUTH_HOST_DEST="$fixture_auth_host" \
  FAKE_CARGO_ARGS_FILE="$cargo_args_file" \
  FAKE_CODESIGN_ARGS_FILE="$codesign_args_file" \
  "$fixture_runner" "$fixture_input" >"$tmp_dir/runner.log" 2>&1 &
runner_pid=$!

# startup budget = 200 polls × 0.1s = 20s, including bundle preparation and signing.
# The previous 50 × 0.1s = 5s budget can expire while a busy host is still signing.
for _ in {1..200}; do
  if [ -s "$app_pid_file" ]; then
    app_pid=$(cat "$app_pid_file")
    if kill -0 "$app_pid" >/dev/null 2>&1; then
      break
    fi
  fi
  sleep 0.1
done

if [ -z "$app_pid" ] || ! kill -0 "$app_pid" >/dev/null 2>&1; then
  cat "$tmp_dir/runner.log" >&2
  fail "fixture app must be alive before the runner receives TERM"
fi

bundle_cli="$tmp_dir/Norma OS Dev.app/Contents/MacOS/norma"
if [ ! -x "$fixture_cli" ]; then
  cat "$tmp_dir/runner.log" >&2
  fail "runner must build norma when cargo run selected only norma-os"
fi

if ! grep -Fq -- '--package norma --bin norma' "$cargo_args_file"; then
  cat "$tmp_dir/runner.log" >&2
  fail "runner must request the norma package CLI instead of relying on unrelated app helper builds"
fi

if [ ! -x "$bundle_cli" ]; then
  cat "$tmp_dir/runner.log" >&2
  fail "cargo run development bundle must include executable norma beside norma-os"
fi

if ! cmp -s "$fixture_cli_source" "$bundle_cli"; then
  fail "development bundle orchestration CLI must match the sibling Cargo-built norma"
fi

bundle_hook="$tmp_dir/Norma OS Dev.app/Contents/MacOS/multisoul-agent-hook"
if [ ! -x "$fixture_hook" ]; then
  cat "$tmp_dir/runner.log" >&2
  fail "runner must build multisoul-agent-hook when cargo run selected only norma-os"
fi

if [ ! -x "$bundle_hook" ]; then
  cat "$tmp_dir/runner.log" >&2
  fail "cargo run development bundle must include executable multisoul-agent-hook beside norma-os"
fi

if ! cmp -s "$fixture_hook_source" "$bundle_hook"; then
  fail "development bundle hook helper must match the sibling Cargo-built multisoul-agent-hook"
fi

bundle_auth_host="$tmp_dir/Norma OS Dev.app/Contents/MacOS/norma-browser-auth-host"
if [ ! -x "$fixture_auth_host" ]; then
  cat "$tmp_dir/runner.log" >&2
  fail "runner must build norma-browser-auth-host when cargo run selected only norma-os"
fi

if [ ! -x "$bundle_auth_host" ]; then
  cat "$tmp_dir/runner.log" >&2
  fail "cargo run development bundle must include executable norma-browser-auth-host beside norma-os"
fi

if ! cmp -s "$fixture_auth_host_source" "$bundle_auth_host"; then
  fail "development bundle auth helper must match the sibling Cargo-built norma-browser-auth-host"
fi

bundle_auth_extension_manifest="$tmp_dir/Norma OS Dev.app/Contents/Resources/browser-auth-extension/manifest.json"
if [ ! -f "$bundle_auth_extension_manifest" ]; then
  cat "$tmp_dir/runner.log" >&2
  fail "cargo run development bundle must include the Chrome authorization extension"
fi

if ! cmp -s "$fixture_auth_extension_manifest" "$bundle_auth_extension_manifest"; then
  fail "development bundle Chrome authorization extension must match the source fixture"
fi

bundle_wallpaper="$tmp_dir/Norma OS Dev.app/Contents/Resources/wallpapers/1407174.mp4"
if ! cmp -s "$fixture_root/build/release-media/media-catalog.json" "$tmp_dir/Norma OS Dev.app/Contents/Resources/media-catalog.json"; then
  fail "development bundle must carry the prepared trusted catalog before signing"
fi
if ! cmp -s "$fixture_root/build/release-media/wallpaper-previews/optional.webp" "$tmp_dir/Norma OS Dev.app/Contents/Resources/wallpaper-previews/optional.webp"; then
  fail "development bundle must carry the prepared static preview"
fi
if [ ! -f "$bundle_wallpaper" ]; then
  cat "$tmp_dir/runner.log" >&2
  fail "cargo run development bundle must include built-in wallpaper resources outside the Documents source checkout"
fi

if ! cmp -s "$fixture_wallpaper_source" "$bundle_wallpaper"; then
  fail "development bundle wallpaper must match the built-in source fixture"
fi

bundle_feature_guide="$tmp_dir/Norma OS Dev.app/Contents/Resources/feature-guide/demo-live-node-shortcuts.mp4"
if [ ! -f "$bundle_feature_guide" ]; then
  cat "$tmp_dir/runner.log" >&2
  fail "cargo run development bundle must include Feature Guide media outside the Documents source checkout"
fi

# 语言资源：每种语言有一个 InfoPlist.strings；构建时必须按原目录名复制。
# 执行：读取两份资源 → 与源码逐一比较；预期不能缺失任一种语言。
for locale in en zh-CN; do
  if ! cmp -s "$repo_root/packaging/macos/$locale.lproj/InfoPlist.strings" "$tmp_dir/Norma OS Dev.app/Contents/Resources/$locale.lproj/InfoPlist.strings"; then
    fail "development bundle must preserve $locale permission descriptions"
  fi
done

if ! cmp -s "$fixture_feature_guide_source" "$bundle_feature_guide"; then
  fail "development bundle Feature Guide media must match the website-derived source fixture"
fi

if ! grep -Fq -- '--sign 3333333333333333333333333333333333333333' "$codesign_args_file"; then
  cat "$tmp_dir/runner.log" >&2
  fail "development bundle must skip duplicate-label identities and sign with a unique-label SHA-1 identity"
fi

kill -TERM "$runner_pid"
wait "$runner_pid" >/dev/null 2>&1 || true
runner_pid=

for _ in {1..50}; do
  if ! kill -0 "$app_pid" >/dev/null 2>&1; then
    app_pid=
    break
  fi
  sleep 0.1
done

if [ -n "$app_pid" ]; then
  cat "$tmp_dir/runner.log" >&2
  fail "TERM sent to the runner must terminate the independently launched app process"
fi

bundle_main="$tmp_dir/Norma OS Dev.app/Contents/MacOS/norma-os"
"$bundle_main" &
app_pid=$!

PATH="$fixture_bin:$PATH" OPEN_APP_PID_FILE="$app_pid_file" \
  FAKE_CLI_SOURCE="$fixture_cli_source" FAKE_CLI_DEST="$fixture_cli" \
  FAKE_HOOK_SOURCE="$fixture_hook_source" FAKE_HOOK_DEST="$fixture_hook" \
  FAKE_AUTH_HOST_SOURCE="$fixture_auth_host_source" FAKE_AUTH_HOST_DEST="$fixture_auth_host" \
  FAKE_CARGO_ARGS_FILE="$cargo_args_file" \
  FAKE_CODESIGN_ARGS_FILE="$codesign_args_file" \
  "$fixture_runner" "$fixture_input" >"$tmp_dir/collision-runner.log" 2>&1 &
collision_runner_pid=$!

for _ in {1..20}; do
  if ! kill -0 "$collision_runner_pid" >/dev/null 2>&1; then
    break
  fi
  sleep 0.1
done

if kill -0 "$collision_runner_pid" >/dev/null 2>&1; then
  kill -TERM "$collision_runner_pid" >/dev/null 2>&1 || true
  wait "$collision_runner_pid" >/dev/null 2>&1 || true
  collision_runner_pid=
  fail "a second cargo run must exit immediately instead of starting another Dev App"
fi

set +e
wait "$collision_runner_pid"
collision_status=$?
set -e
collision_runner_pid=

if [ "$collision_status" -eq 0 ]; then
  cat "$tmp_dir/collision-runner.log" >&2
  fail "a second cargo run must return a non-zero status while the Dev App is alive"
fi

if ! grep -Fq 'Norma OS Dev is already running' "$tmp_dir/collision-runner.log"; then
  cat "$tmp_dir/collision-runner.log" >&2
  fail "a repeated cargo run must explain that the existing Dev App must be closed first"
fi

if ! kill -0 "$app_pid" >/dev/null 2>&1; then
  cat "$tmp_dir/collision-runner.log" >&2
  fail "refusing a repeated cargo run must not terminate the already running Dev App"
fi

kill -TERM "$app_pid"
wait "$app_pid" >/dev/null 2>&1 || true
app_pid=

echo "macOS dev app runner contract test passed"
