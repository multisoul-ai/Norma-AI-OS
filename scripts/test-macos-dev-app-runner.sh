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
#
# Execution:
#   1. Check Cargo's native macOS target runner configuration.
#   2. Check the runner materializes an app bundle from the compiled binary.
#   3. Check the runner clears stale bundle contents before copying the binary.
#   4. Check the runner installs the development microphone usage description.
#   5. Check the LaunchServices process forwards stdout/stderr to the caller.
#   6. Check the runner executes the binary from inside that bundle.
#
# Expected result:
#   - Positive: aarch64 macOS cargo runs use the development app runner.
#   - Positive: the runner copies DevInfo.plist into Contents/Info.plist.
#   - Positive: the runner executes Contents/MacOS/<binary>.
#   - Positive: tracing output remains visible to terminal pipes such as `tee`.
#   - Negative: stale executables must not survive into a newly signed bundle.
#   - Negative: cargo run must not keep launching only target/debug/norma-os.
#   - Negative: LaunchServices must not leave app stdout/stderr on `/dev/null`.

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

echo "macOS dev app runner contract test passed"
