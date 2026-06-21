#!/usr/bin/env bash
# Mechanized constraint: storage is a lower persistence layer, while hot paths
# must not perform direct SQLite work.

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

for forbidden in app runtime ui; do
  if [ -n "$(tree_contains storage "$forbidden")" ]; then
    record_failure "storage must not depend on ${forbidden}"
  fi
done

for package in canvas terminal; do
  if [ -n "$(tree_contains "$package" storage)" ]; then
    record_failure "${package} must not depend on storage"
  fi
done

hot_path_pattern='rusqlite|Connection|AppStore|ProjectStore|RecentStore|CanvasStore|SessionStore|save_snapshot|save_checkpoint|record_opened'
hot_path_files=(
  crates/runtime/src/route.rs
  crates/terminal/src/pty.rs
  crates/terminal/src/session.rs
  crates/app/src/canvas_view.rs
)

for file in "${hot_path_files[@]}"; do
  matches=$(grep -nE "$hot_path_pattern" "$file" 2>/dev/null || true)
  if [ -n "$matches" ]; then
    record_failure "hot path file ${file} contains direct persistence reference:"
    record_failure "$matches"
  fi
done

send_input_body=$(sed -n '/pub fn send_node_input/,/pub fn drain_all/p' crates/runtime/src/runtime.rs)
if printf '%s\n' "$send_input_body" | grep -qE "$hot_path_pattern"; then
  record_failure "Runtime::send_node_input must not contain direct persistence work"
fi

drain_body=$(sed -n '/pub fn drain_all/,/pub fn remove_node/p' crates/runtime/src/runtime.rs)
if printf '%s\n' "$drain_body" | grep -qE "$hot_path_pattern"; then
  record_failure "Runtime::drain_all must not contain direct persistence work"
fi

frame_loop=$(sed -n '/let runtime_changed = app.runtime.drain_all()/,/if runtime_changed/p' crates/app/src/app.rs)
if printf '%s\n' "$frame_loop" | grep -qE "$hot_path_pattern"; then
  record_failure "app frame loop must not call direct SQLite store APIs"
fi

if [ -n "$failures" ]; then
  echo "ERROR: storage boundary violations found." >&2
  printf '%s\n' "$failures" >&2
  exit 1
fi

exit 0
