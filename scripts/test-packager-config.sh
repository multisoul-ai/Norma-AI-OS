#!/usr/bin/env bash
# Focused test for Packager.toml release binary wiring.
#
# Scenario:
#   Packager.toml packages target/release/multisoul-agent-hook as a secondary
#   macOS bundle executable.
#
# Data construction:
#   required binary path = target/release/multisoul-agent-hook
#   release build hook   = before-packaging-command in Packager.toml
#   required cargo target = -p app（app package now owns both release binaries）
#
# Execution:
#   1. Read Packager.toml from the repository root.
#   2. Verify the secondary hook binary is listed as a packaged binary.
#   3. Verify the one-shot pre-packaging command builds the app package.
#
# Expected result:
#   - Positive: the hook binary path is present in [[binaries]].
#   - Positive: before-packaging-command exists and invokes `cargo build --release -p app`.
#   - Negative: a config that only packages the hook, but never builds it, fails.

set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
config="$repo_root/Packager.toml"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

if ! grep -Fq 'path = "target/release/multisoul-agent-hook"' "$config"; then
  fail "Packager.toml must package target/release/multisoul-agent-hook"
fi

if ! grep -Fq 'before-packaging-command' "$config"; then
  fail "Packager.toml must build release binaries before packaging"
fi

if ! grep -F 'before-packaging-command' "$config" | grep -Fq -- 'cargo build --release -p app'; then
  fail "before-packaging-command must build the app package release binaries"
fi

echo "packager config test passed"
