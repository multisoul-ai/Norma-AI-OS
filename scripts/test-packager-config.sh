#!/usr/bin/env bash
# Focused test for Packager.toml release binary wiring.
#
# Scenario:
#   Packager.toml packages target/release/norma-os as the main macOS bundle
#   executable and target/release/multisoul-agent-hook as a secondary helper.
#
# Data construction:
#   main binary path     = target/release/norma-os
#   hook binary path     = target/release/multisoul-agent-hook
#   planner helper path  = sidecars/openai-card-planner/openai-card-planner.mjs
#   release build hook   = before-packaging-command in Packager.toml
#   required cargo target = -p app（app package now owns both release binaries）
#
# Execution:
#   1. Read Packager.toml from the repository root.
#   2. Verify the secondary hook binary is listed as a packaged binary.
#   3. Verify the one-shot pre-packaging command builds the app package.
#
# Expected result:
#   - Positive: product-name is Norma OS.
#   - Positive: bundle identifier is com.norma.os.
#   - Positive: the main app binary path is target/release/norma-os.
#   - Positive: the hook binary path is present in [[binaries]].
#   - Positive: the OpenAI card planner helper is packaged as a resource.
#   - Positive: before-packaging-command exists and bumps the persisted version.
#   - Positive: before-packaging-command invokes `cargo build --release -p app`.
#   - Negative: the old target/release/multisoul main binary path is not retained.
#   - Negative: old Multisoul product metadata is not retained.
#   - Negative: a config that builds without bumping the version fails.
#   - Negative: a config that bumps after building fails.

set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
config="$repo_root/Packager.toml"
dev_config="$repo_root/Packager.dev.toml"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

if ! grep -Fq 'path = "target/release/norma-os"' "$config"; then
  fail "Packager.toml must package target/release/norma-os as the main app binary"
fi

if grep -Fq 'path = "target/release/multisoul"' "$config"; then
  fail "Packager.toml must not package the old target/release/multisoul app binary"
fi

if ! grep -Fq 'path = "target/release/multisoul-agent-hook"' "$config"; then
  fail "Packager.toml must package target/release/multisoul-agent-hook"
fi

for resource_config in "$config" "$dev_config"; do
  if ! grep -Fq '"sidecars/openai-card-planner/openai-card-planner.mjs"' "$resource_config"; then
    fail "$(basename "$resource_config") must package the OpenAI card planner helper resource"
  fi
done

if ! grep -Fq 'product-name = "Norma OS"' "$config"; then
  fail "Packager.toml product-name must be Norma OS"
fi

if ! grep -Fq 'identifier = "com.norma.os"' "$config"; then
  fail "Packager.toml identifier must be com.norma.os"
fi

if grep -Fq 'product-name = "Multisoul"' "$config"; then
  fail "Packager.toml must not keep the old Multisoul product-name"
fi

if ! grep -Fq 'before-packaging-command' "$config"; then
  fail "Packager.toml must build release binaries before packaging"
fi

hook=$(grep -F 'before-packaging-command' "$config")

if ! printf '%s\n' "$hook" | grep -Fq -- 'scripts/bump-release-version.sh'; then
  fail "before-packaging-command must bump the release version before packaging"
fi

if ! printf '%s\n' "$hook" | grep -Fq -- 'cargo build --release -p app'; then
  fail "before-packaging-command must build the app package release binaries"
fi

case "$hook" in
  *scripts/bump-release-version.sh*'cargo build --release -p app'*)
    ;;
  *)
    fail "before-packaging-command must bump the release version before cargo build"
    ;;
esac

echo "packager config test passed"
