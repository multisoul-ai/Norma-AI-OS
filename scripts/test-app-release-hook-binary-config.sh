#!/usr/bin/env bash
# Focused test for `cargo build --release -p app` hook binary wiring.
#
# Scenario:
#   A release app build must place `multisoul-agent-hook` next to the
#   `multisoul` executable so agent hook install can resolve the sibling binary.
#
# Data construction:
#   app manifest  = crates/app/Cargo.toml
#   required bin  = multisoul-agent-hook
#   wrapper path  = src/bin/multisoul_agent_hook.rs
#
# Execution:
#   1. Read the app manifest from the repository root.
#   2. Verify the app package declares a secondary hook binary.
#   3. Verify `cargo run -p app` still defaults to the Multisoul app binary.
#   4. Verify the wrapper source delegates to the shared norma hook CLI.
#
# Expected result:
#   - Positive: app manifest contains a `multisoul-agent-hook` bin target.
#   - Positive: app manifest sets `default-run = "multisoul"`.
#   - Positive: wrapper source exists and calls `norma::agent_hook::cli::main`.
#   - Negative: relying only on Packager.toml or the norma package bin fails.

set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
manifest="$repo_root/crates/app/Cargo.toml"
wrapper="$repo_root/crates/app/src/bin/multisoul_agent_hook.rs"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

if ! grep -Fq 'name = "multisoul-agent-hook"' "$manifest"; then
  fail "crates/app/Cargo.toml must declare a multisoul-agent-hook bin target"
fi

if ! grep -Fq 'default-run = "multisoul"' "$manifest"; then
  fail "crates/app/Cargo.toml must set default-run so cargo run -p app starts Multisoul"
fi

if ! grep -Fq 'path = "src/bin/multisoul_agent_hook.rs"' "$manifest"; then
  fail "app hook bin target must point at src/bin/multisoul_agent_hook.rs"
fi

if [ ! -f "$wrapper" ]; then
  fail "app hook wrapper source is missing"
fi

if ! grep -Fq 'norma::agent_hook::cli::main()' "$wrapper"; then
  fail "app hook wrapper must delegate to norma::agent_hook::cli::main()"
fi

echo "app release hook binary config test passed"
