#!/usr/bin/env bash
# Mechanized constraint: keep known UI hot paths and shutdown persistence off
# expensive storage/discovery work.

set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

failures=""

record_failure() {
  failures="${failures}$1"$'\n'
}

line_number() {
  local pattern="$1"
  local file="$2"
  grep -nF "$pattern" "$file" | head -n 1 | cut -d: -f1
}

forbid_pattern_in_file() {
  local file="$1"
  local pattern="$2"
  local message="$3"
  local matches
  matches=$(grep -nE "$pattern" "$file" 2>/dev/null || true)
  if [ -n "$matches" ]; then
    record_failure "$message"
    record_failure "$matches"
  fi
}

collect_body=$(
  awk '
    /^pub fn collect_node_sessions\(/ { inside=1 }
    inside { print }
    inside && /^}$/ { exit }
  ' crates/runtime/src/session_snapshot.rs
)

if printf '%s\n' "$collect_body" | grep -qE 'find_(agent|codex)_session_id|read_dir|read_to_string|visit_codex_session_files'; then
  record_failure "collect_node_sessions must use cached agent session IDs only; no filesystem discovery on save/close."
fi

forbid_pattern_in_file \
  crates/app/src/canvas/events.rs \
  'rusqlite|Connection::open|ProjectStore|NodeSessionStore|SessionStore|CanvasStore|save_node_sessions|save_session_checkpoints|flush_persistence_now|find_(agent|codex)_session_id' \
  "canvas mouse/pointer event code must not call persistence or session discovery APIs."

forbid_pattern_in_file \
  crates/app/src/canvas/view.rs \
  'rusqlite|Connection::open|ProjectStore|NodeSessionStore|SessionStore|CanvasStore|save_node_sessions|save_session_checkpoints|flush_persistence_now|find_(agent|codex)_session_id' \
  "canvas render code must not call persistence or session discovery APIs."

forbid_pattern_in_file \
  crates/app/src/app/video_wallpaper.rs \
  'surface\(|VideoDecoder::open\(&path\)|decoder\.next_frame\(\)' \
  "video wallpaper UI lifecycle/render code must not use surface(CVPixelBuffer) or foreground AVFoundation open/decode."

codex_body=$(
  awk '
    /^fn codex_session_id_for_workspace\(/ { inside=1 }
    inside { print }
    inside && /^}$/ { exit }
  ' crates/runtime/src/session_snapshot.rs
)

if printf '%s\n' "$codex_body" | grep -q 'read_to_string'; then
  record_failure "codex_session_id_for_workspace must use bounded line reads, not read_to_string on full JSONL files."
fi

persistence_file=crates/app/src/workspace/persistence.rs
guard_line=$(line_number "if !had_pending_work" "$persistence_file")
node_sessions_line=$(line_number "save_node_sessions" "$persistence_file")

if [ -z "$guard_line" ]; then
  record_failure "PersistenceScheduler::flush_now must keep an explicit no-pending-work fast return."
elif [ -n "$node_sessions_line" ] && [ "$node_sessions_line" -lt "$guard_line" ]; then
  record_failure "save_node_sessions must appear after the no-pending-work fast return in flush_now."
fi

if [ -n "$failures" ]; then
  echo "ERROR: performance guardrail violations found." >&2
  printf '%s\n' "$failures" >&2
  exit 1
fi

exit 0
