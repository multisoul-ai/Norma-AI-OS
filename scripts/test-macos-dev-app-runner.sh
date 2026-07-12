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
#   signal fixture = fake `open -W` that starts an independently living app
#                    process, mirroring LaunchServices parent detachment.
#
# Execution:
#   1. Check Cargo's native macOS target runner configuration.
#   2. Check the runner materializes an app bundle from the compiled binary.
#   3. Check the runner clears stale bundle contents before copying the binary.
#   4. Check the runner installs the development microphone usage description.
#   5. Check the LaunchServices process forwards stdout/stderr to the caller.
#   6. Check the runner executes the binary from inside that bundle.
#   7. Check Ctrl+C / terminal termination also stops the detached app process.
#
# Expected result:
#   - Positive: aarch64 macOS cargo runs use the development app runner.
#   - Positive: the runner copies DevInfo.plist into Contents/Info.plist.
#   - Positive: the runner executes Contents/MacOS/<binary>.
#   - Positive: tracing output remains visible to terminal pipes such as `tee`.
#   - Positive: INT and TERM terminate the specific app process launched by this runner.
#   - Negative: stale executables must not survive into a newly signed bundle.
#   - Negative: cargo run must not keep launching only target/debug/norma-os.
#   - Negative: LaunchServices must not leave app stdout/stderr on `/dev/null`.
#   - Negative: stopping Cargo must not leave Norma OS Dev visible after its runner exits.

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
cleanup_fixture() {
  if [ -n "$runner_pid" ]; then kill -TERM "$runner_pid" >/dev/null 2>&1 || true; fi
  if [ -n "$app_pid" ]; then kill -TERM "$app_pid" >/dev/null 2>&1 || true; fi
  rm -rf "$tmp_dir"
}
trap cleanup_fixture EXIT

fixture_root="$tmp_dir/fixture"
fixture_bin="$fixture_root/bin"
fixture_runner="$fixture_root/scripts/run-macos-dev-app.sh"
fixture_input="$tmp_dir/test-input-binary"
app_pid_file="$tmp_dir/app.pid"
mkdir -p "$fixture_bin" "$fixture_root/scripts" "$fixture_root/packaging/macos"
cp "$runner" "$fixture_runner"
cp "$dev_plist" "$fixture_root/packaging/macos/DevInfo.plist"
cp "$entitlements" "$fixture_root/packaging/macos/NormaOS.entitlements"

cat >"$fixture_input" <<'APP'
#!/usr/bin/env bash
trap 'exit 0' TERM
while true; do sleep 1; done
APP
chmod +x "$fixture_input"

cat >"$fixture_bin/codesign" <<'CODESIGN'
#!/usr/bin/env bash
exit 0
CODESIGN
chmod +x "$fixture_bin/codesign"

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

"$bundle/Contents/MacOS/test-input-binary" &
app_pid=$!
printf '%s\n' "$app_pid" >"$OPEN_APP_PID_FILE"
trap 'exit 0' INT TERM
wait "$app_pid"
OPEN
chmod +x "$fixture_bin/open"

PATH="$fixture_bin:$PATH" OPEN_APP_PID_FILE="$app_pid_file" \
  "$fixture_runner" "$fixture_input" >"$tmp_dir/runner.log" 2>&1 &
runner_pid=$!

for _ in {1..50}; do
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

echo "macOS dev app runner contract test passed"
