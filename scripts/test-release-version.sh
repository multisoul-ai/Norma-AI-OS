#!/usr/bin/env bash
# Version lifecycle seam: read-only validation and explicit post-release patch bump.
#
# Scenario:
#   The two manifests describe the release being built; validation does not edit
#   them, while the separate bump command advances both only when explicitly run.
#
# Data construction (including key numeric derivation):
#   release version = 3.4.9
#   next patch       = 3.4.(9 + 1) = 3.4.10
#   mismatch fixture = Packager 3.4.9 versus Cargo 3.4.8
#
# Execution:
#   1. Validate matching fixture manifests twice and compare bytes before/after.
#   2. Reject the mismatched fixture without changing either manifest.
#   3. Require 3.4.9 to be newer than public version 3.4.8, then reject
#      equal 3.4.9 and newer 3.5.0 baselines.
#   4. Explicitly bump the matching fixture and validate 3.4.10.
#
# Expected result:
#   - Positive: validation prints exactly 3.4.9 and preserves both files.
#   - Positive: 3.4.9 is accepted as newer than 3.4.8.
#   - Positive: explicit bump produces and validates exactly 3.4.10.
#   - Negative: mismatch 3.4.9/3.4.8 is rejected and never auto-repaired.
#   - Negative: equal 3.4.9 and newer 3.5.0 public baselines are rejected.

set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
tmp_dir=$(mktemp -d)
trap 'rm -rf "$tmp_dir"' EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

packager="$tmp_dir/Packager.toml"
cargo="$tmp_dir/Cargo.toml"
cat >"$packager" <<'TOML'
name = "app"
version = "3.4.9"
[macos]
minimum-system-version = "12.0"
TOML
cat >"$cargo" <<'TOML'
[workspace.package]
edition = "2021"
version = "3.4.9"
license = "MIT"
TOML

before_packager=$(shasum -a 256 "$packager" | awk '{print $1}')
before_cargo=$(shasum -a 256 "$cargo" | awk '{print $1}')
version=$("$repo_root/scripts/release-version.sh" --config "$packager" --cargo "$cargo")
[ "$version" = "3.4.9" ] || fail "read-only validator should print 3.4.9"
[ "$before_packager" = "$(shasum -a 256 "$packager" | awk '{print $1}')" ] \
  || fail "read-only validator must not edit Packager.toml"
[ "$before_cargo" = "$(shasum -a 256 "$cargo" | awk '{print $1}')" ] \
  || fail "read-only validator must not edit Cargo.toml"

newer=$("$repo_root/scripts/release-version.sh" \
  --config "$packager" --cargo "$cargo" --require-newer-than v3.4.8)
[ "$newer" = "3.4.9" ] || fail "3.4.9 should be accepted as newer than public tag v3.4.8"
if "$repo_root/scripts/release-version.sh" \
  --config "$packager" --cargo "$cargo" --require-newer-than v3.4.9 >/dev/null 2>&1; then
  fail "release version 3.4.9 must not equal the public latest tag v3.4.9"
fi
if "$repo_root/scripts/release-version.sh" \
  --config "$packager" --cargo "$cargo" --require-newer-than v3.5.0 >/dev/null 2>&1; then
  fail "release version 3.4.9 must not be older than public latest tag v3.5.0"
fi

sed -i.bak 's/version = "3.4.9"/version = "3.4.8"/' "$cargo"
rm "$cargo.bak"
mismatch_cargo=$(shasum -a 256 "$cargo" | awk '{print $1}')
if "$repo_root/scripts/release-version.sh" --config "$packager" --cargo "$cargo" >/dev/null 2>&1; then
  fail "mismatched 3.4.9 and 3.4.8 manifests should be rejected"
fi
[ "$mismatch_cargo" = "$(shasum -a 256 "$cargo" | awk '{print $1}')" ] \
  || fail "mismatch validation must not auto-repair Cargo.toml"

sed -i.bak 's/version = "3.4.8"/version = "3.4.9"/' "$cargo"
rm "$cargo.bak"
"$repo_root/scripts/bump-release-version.sh" --config "$packager" --cargo "$cargo" >/dev/null
next=$("$repo_root/scripts/release-version.sh" --config "$packager" --cargo "$cargo")
[ "$next" = "3.4.10" ] || fail "explicit bump should advance 3.4.9 to 3.4.10"
if grep -Fq 'version = "3.4.9"' "$packager" "$cargo"; then
  fail "explicit bump should not retain stale 3.4.9 manifest versions"
fi

echo "release version lifecycle test passed"
