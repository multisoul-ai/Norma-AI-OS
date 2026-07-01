#!/usr/bin/env bash
# Focused test for autonomous release version bumping.
#
# Scenario:
#   The release bump script advances a plain semantic patch version and keeps
#   Packager.toml synchronized with the workspace package version.
#
# Data construction:
#   Packager.toml version = 1.2.9
#   Cargo.toml workspace version = 1.2.9
#   Expected next patch version = 1.2.(9 + 1) = 1.2.10
#
# Execution:
#   1. Create temporary Packager.toml and Cargo.toml files.
#   2. Run scripts/bump-release-version.sh against those files.
#   3. Verify both files contain 1.2.10 and no longer contain 1.2.9.
#   4. Run the script against an invalid 1.2 version and verify it fails.
#
# Expected result:
#   - Positive: Packager.toml is updated to 1.2.10.
#   - Positive: Cargo.toml workspace version is updated to 1.2.10.
#   - Negative: stale 1.2.9 versions are absent after a successful bump.
#   - Negative: invalid non-MAJOR.MINOR.PATCH versions are rejected.

set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
tmp_dir=$(mktemp -d)
trap 'rm -rf "$tmp_dir"' EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

packager_config="$tmp_dir/Packager.toml"
cargo_manifest="$tmp_dir/Cargo.toml"

cat >"$packager_config" <<'EOF'
name = "app"
version = "1.2.9"

[macos]
minimum-system-version = "12.0"
EOF

cat >"$cargo_manifest" <<'EOF'
[workspace]
resolver = "2"

[workspace.package]
edition = "2021"
version = "1.2.9"
license = "MIT"

[workspace.dependencies]
anyhow = "1"
EOF

bash "$repo_root/scripts/bump-release-version.sh" \
  --config "$packager_config" \
  --cargo "$cargo_manifest"

if ! grep -Fq 'version = "1.2.10"' "$packager_config"; then
  fail "Packager.toml should be bumped to 1.2.10"
fi

if ! awk '
  /^\[workspace\.package\]$/ { in_workspace_package = 1; next }
  /^\[/ { in_workspace_package = 0 }
  in_workspace_package && $0 == "version = \"1.2.10\"" { found = 1 }
  END { exit found ? 0 : 1 }
' "$cargo_manifest"; then
  fail "Cargo.toml workspace package version should be bumped to 1.2.10"
fi

if grep -Fq 'version = "1.2.9"' "$packager_config"; then
  fail "Packager.toml should not keep the stale 1.2.9 version"
fi

if awk '
  /^\[workspace\.package\]$/ { in_workspace_package = 1; next }
  /^\[/ { in_workspace_package = 0 }
  in_workspace_package && $0 == "version = \"1.2.9\"" { found = 1 }
  END { exit found ? 0 : 1 }
' "$cargo_manifest"; then
  fail "Cargo.toml workspace package version should not keep the stale 1.2.9 version"
fi

invalid_config="$tmp_dir/InvalidPackager.toml"
invalid_cargo="$tmp_dir/InvalidCargo.toml"
cp "$packager_config" "$invalid_config"
cp "$cargo_manifest" "$invalid_cargo"
sed -i.bak 's/version = "1.2.10"/version = "1.2"/' "$invalid_config"
sed -i.bak 's/version = "1.2.10"/version = "1.2"/' "$invalid_cargo"
rm "$invalid_config.bak"
rm "$invalid_cargo.bak"

if bash "$repo_root/scripts/bump-release-version.sh" \
  --config "$invalid_config" \
  --cargo "$invalid_cargo" >/dev/null 2>&1; then
  fail "invalid 1.2 version should be rejected"
fi

if ! grep -Fq 'version = "1.2"' "$invalid_config"; then
  fail "invalid config version should remain unchanged after rejection"
fi

echo "release version bump test passed"
