#!/usr/bin/env bash
# Mechanized constraint: `account` is a UI-free, storage-free domain crate.
# It must not depend on app/runtime/storage/ui/gpui, and storage must not depend on it.

set -euo pipefail

failures=""

tree_contains() {
  local package="$1"
  local forbidden="$2"
  cargo tree -p "$package" --prefix none 2>/dev/null | grep -E "^${forbidden} v" || true
}

record_failure() {
  failures="${failures}$1"$'\n'
}

for forbidden in app runtime storage ui gpui gpui-component rusqlite; do
  if [ -n "$(tree_contains account "$forbidden")" ]; then
    record_failure "account must not depend on ${forbidden}"
  fi
done

if [ -n "$(tree_contains storage account)" ]; then
  record_failure "storage must not depend on account"
fi

if [ -n "$failures" ]; then
  printf 'Account boundary violations:\n%s' "$failures" >&2
  exit 1
fi

echo "account boundaries OK"
