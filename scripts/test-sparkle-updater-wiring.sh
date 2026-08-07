#!/usr/bin/env bash
# Client update seam: one App-global Sparkle 2 controller owns the standard action.
#
# Scenario:
#   The application initializes Sparkle once on the main thread, retains it for
#   the process lifetime, and routes the native menu action without custom UI.
#
# Data construction (including key numeric derivation):
#   controller instances per App = 1 (guarded by has_global)
#   custom polling loops          = 0
#   native menu actions           = 1 CheckForUpdates action
#
# Execution:
#   1. Inspect the updater owner, global registration, action, and menu mapping.
#   2. Verify dynamic framework absence maps to an unavailable state and warning.
#   3. Verify no dlclose occurs after successful controller construction.
#
# Expected result:
#   - Positive: SPUStandardUpdaterController starts on the main thread and is retained globally.
#   - Positive: Check for Updates… invokes checkForUpdates: using Sparkle standard UI.
#   - Negative: no SUUpdater legacy API, polling loop, custom update dialog, or successful-load dlclose exists.

set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
updater="$repo_root/crates/app/src/updater.rs"
menu="$repo_root/crates/app/src/menu_bar.rs"
lib="$repo_root/crates/app/src/lib.rs"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

grep -Fq 'SPUStandardUpdaterController' "$updater" || fail "client must use Sparkle 2 standard updater controller"
grep -Fq 'pthread_main_np()' "$updater" || fail "Sparkle initialization must enforce the macOS main thread"
grep -Fq 'initWithStartingUpdater: true' "$updater" || fail "standard updater must start through its official initializer"
grep -Fq 'cx.has_global::<SparkleUpdater>()' "$updater" || fail "App must install only one global updater owner"
grep -Fq 'checkForUpdates:' "$updater" || fail "menu action must invoke Sparkle checkForUpdates:"
grep -Fq 'Sparkle.framework is absent from this development build' "$updater" \
  || fail "framework absence must have an explicit safe development fallback"
grep -Fq 'Check for Updates…' "$menu" || fail "native application menu must use the familiar macOS label"
grep -Fq 'updater::install(cx);' "$lib" || fail "App startup must retain the updater before opening a window"
grep -Fq 'updater::check_for_updates(cx)' "$lib" || fail "global CheckForUpdates action must route to the updater owner"

if grep -Fq 'SUUpdater' "$updater"; then
  fail "legacy Sparkle 1 SUUpdater API must not be used"
fi
if grep -Eq '(sleep\(|Timer|poll)' "$updater"; then
  fail "client must let Sparkle schedule checks instead of adding a polling loop"
fi
if grep -A20 -F 'Ok(Self {' "$updater" | grep -Fq 'dlclose'; then
  fail "successfully loaded Objective-C framework must remain loaded until process termination"
fi

echo "Sparkle updater wiring test passed"
