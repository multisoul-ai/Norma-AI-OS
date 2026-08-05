#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
packager_config="$repo_root/Packager.toml"
cargo_manifest="$repo_root/Cargo.toml"
require_newer_than=""

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --config)
      [ "$#" -ge 2 ] || fail "--config requires a path"
      packager_config="$2"
      shift 2
      ;;
    --cargo)
      [ "$#" -ge 2 ] || fail "--cargo requires a path"
      cargo_manifest="$2"
      shift 2
      ;;
    --require-newer-than)
      [ "$#" -ge 2 ] || fail "--require-newer-than requires a version or v-prefixed tag"
      require_newer_than="$2"
      shift 2
      ;;
    *)
      fail "unknown argument: $1"
      ;;
  esac
done

[ -f "$packager_config" ] || fail "packager config not found: $packager_config"
[ -f "$cargo_manifest" ] || fail "cargo manifest not found: $cargo_manifest"

packager_version=$(awk '
  BEGIN { in_top_level = 1 }
  /^\[/ { in_top_level = 0 }
  in_top_level && $1 == "version" && $2 == "=" {
    value = $3
    gsub(/^"|"$/, "", value)
    print value
    found = 1
    exit
  }
  END { exit found ? 0 : 1 }
' "$packager_config") || fail "Packager.toml top-level version not found"

cargo_version=$(awk '
  /^\[workspace\.package\]$/ { in_workspace_package = 1; next }
  /^\[/ { in_workspace_package = 0 }
  in_workspace_package && $1 == "version" && $2 == "=" {
    value = $3
    gsub(/^"|"$/, "", value)
    print value
    found = 1
    exit
  }
  END { exit found ? 0 : 1 }
' "$cargo_manifest") || fail "Cargo.toml [workspace.package] version not found"

case "$packager_version" in
  ''|*[!0-9.]*|.*|*..*|*.)
    fail "version must use numeric MAJOR.MINOR.PATCH: $packager_version"
    ;;
esac

IFS=. read -r major minor patch extra <<<"$packager_version"
if [ -n "${extra:-}" ] || [ -z "${major:-}" ] || [ -z "${minor:-}" ] || [ -z "${patch:-}" ]; then
  fail "version must use numeric MAJOR.MINOR.PATCH: $packager_version"
fi

if [ "$packager_version" != "$cargo_version" ]; then
  fail "version mismatch: Packager.toml has $packager_version but Cargo.toml has $cargo_version"
fi

if [ -n "$require_newer_than" ]; then
  baseline_version=${require_newer_than#v}
  case "$baseline_version" in
    ''|*[!0-9.]*|.*|*..*|*.)
      fail "comparison version must use numeric MAJOR.MINOR.PATCH: $require_newer_than"
      ;;
  esac

  IFS=. read -r baseline_major baseline_minor baseline_patch baseline_extra <<<"$baseline_version"
  if [ -n "${baseline_extra:-}" ] || [ -z "${baseline_major:-}" ] \
    || [ -z "${baseline_minor:-}" ] || [ -z "${baseline_patch:-}" ]; then
    fail "comparison version must use numeric MAJOR.MINOR.PATCH: $require_newer_than"
  fi

  current_major=$((10#$major))
  current_minor=$((10#$minor))
  current_patch=$((10#$patch))
  public_major=$((10#$baseline_major))
  public_minor=$((10#$baseline_minor))
  public_patch=$((10#$baseline_patch))
  if [ "$current_major" -lt "$public_major" ] \
    || { [ "$current_major" -eq "$public_major" ] && [ "$current_minor" -lt "$public_minor" ]; } \
    || { [ "$current_major" -eq "$public_major" ] && [ "$current_minor" -eq "$public_minor" ] \
      && [ "$current_patch" -le "$public_patch" ]; }; then
    fail "release version $packager_version must be newer than public latest $require_newer_than"
  fi
fi

printf '%s\n' "$packager_version"
