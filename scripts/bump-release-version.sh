#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
packager_config="$repo_root/Packager.toml"
cargo_manifest="$repo_root/Cargo.toml"

usage() {
  echo "usage: $0 [--config Packager.toml] [--cargo Cargo.toml]" >&2
}

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
    -h|--help)
      usage
      exit 0
      ;;
    *)
      usage
      fail "unknown argument: $1"
      ;;
  esac
done

[ -f "$packager_config" ] || fail "packager config not found: $packager_config"
[ -f "$cargo_manifest" ] || fail "cargo manifest not found: $cargo_manifest"

extract_packager_version() {
  awk '
    /^\[/ { in_top_level = 0 }
    BEGIN { in_top_level = 1 }
    in_top_level && $1 == "version" && $2 == "=" {
      value = $3
      gsub(/^"/, "", value)
      gsub(/"$/, "", value)
      print value
      found = 1
      exit
    }
    END { exit found ? 0 : 1 }
  ' "$1"
}

extract_workspace_version() {
  awk '
    /^\[workspace\.package\]$/ { in_workspace_package = 1; next }
    /^\[/ { in_workspace_package = 0 }
    in_workspace_package && $1 == "version" && $2 == "=" {
      value = $3
      gsub(/^"/, "", value)
      gsub(/"$/, "", value)
      print value
      found = 1
      exit
    }
    END { exit found ? 0 : 1 }
  ' "$1"
}

next_patch_version() {
  local version="$1"
  local major minor patch extra

  IFS=. read -r major minor patch extra <<<"$version"
  if [ -n "${extra:-}" ] || [ -z "${major:-}" ] || [ -z "${minor:-}" ] || [ -z "${patch:-}" ]; then
    fail "version must use MAJOR.MINOR.PATCH: $version"
  fi

  case "$major.$minor.$patch" in
    *[!0-9.]*)
      fail "version must use numeric MAJOR.MINOR.PATCH: $version"
      ;;
  esac

  echo "$major.$minor.$((10#$patch + 1))"
}

replace_packager_version() {
  local file="$1"
  local old_version="$2"
  local new_version="$3"
  local tmp="${file}.tmp.$$"

  awk -v old_version="$old_version" -v new_version="$new_version" '
    /^\[/ { in_top_level = 0 }
    BEGIN { in_top_level = 1 }
    in_top_level && $0 == "version = \"" old_version "\"" {
      print "version = \"" new_version "\""
      replaced = 1
      next
    }
    { print }
    END { exit replaced ? 0 : 1 }
  ' "$file" >"$tmp" || {
    rm -f "$tmp"
    fail "failed to replace Packager.toml version $old_version"
  }

  mv "$tmp" "$file"
}

replace_workspace_version() {
  local file="$1"
  local old_version="$2"
  local new_version="$3"
  local tmp="${file}.tmp.$$"

  awk -v old_version="$old_version" -v new_version="$new_version" '
    /^\[workspace\.package\]$/ { in_workspace_package = 1 }
    /^\[/ && $0 != "[workspace.package]" { in_workspace_package = 0 }
    in_workspace_package && $0 == "version = \"" old_version "\"" {
      print "version = \"" new_version "\""
      replaced = 1
      next
    }
    { print }
    END { exit replaced ? 0 : 1 }
  ' "$file" >"$tmp" || {
    rm -f "$tmp"
    fail "failed to replace Cargo.toml workspace version $old_version"
  }

  mv "$tmp" "$file"
}

packager_version=$(extract_packager_version "$packager_config") \
  || fail "Packager.toml top-level version not found"
cargo_version=$(extract_workspace_version "$cargo_manifest") \
  || fail "Cargo.toml [workspace.package] version not found"

if [ "$packager_version" != "$cargo_version" ]; then
  fail "version mismatch: Packager.toml has $packager_version but Cargo.toml has $cargo_version"
fi

next_version=$(next_patch_version "$packager_version")

replace_packager_version "$packager_config" "$packager_version" "$next_version"
replace_workspace_version "$cargo_manifest" "$cargo_version" "$next_version"

echo "bumped release version: $packager_version -> $next_version"
