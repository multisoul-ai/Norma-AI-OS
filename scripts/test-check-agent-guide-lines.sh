#!/usr/bin/env bash
# Focused tests for scripts/check-agent-guide-lines.sh.
#
# Scenarios covered:
#   1. AGENTS.md and CLAUDE.md at exactly 300 lines pass.
#   2. A guide file with 301 lines fails and reports the file.
#   3. Missing AGENTS.md §4 heading fails even when under the line cap.
#   4. --staged checks only staged guide files.
#
# Expected result:
#   - Positive: exact-limit guides pass when §4 exists.
#   - Positive: over-limit guides are rejected with path and line count.
#   - Negative: guides without §4 are rejected.

set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
lint_script="$repo_root/scripts/check-agent-guide-lines.sh"
tmp_root=$(mktemp -d)

trap 'rm -rf "$tmp_root"' EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

new_repo() {
  local name=$1
  local dir="$tmp_root/$name"
  mkdir -p "$dir/scripts"
  cp "$lint_script" "$dir/scripts/check-agent-guide-lines.sh"
  git -C "$dir" init -q
  printf '%s\n' "$dir"
}

write_lines() {
  local file=$1
  local count=$2
  mkdir -p "$(dirname "$file")"
  : >"$file"

  local i=1
  while [ "$i" -le "$count" ]; do
    printf '# line %s\n' "$i" >>"$file"
    i=$((i + 1))
  done
}

write_agents_with_section4() {
  local file=$1
  local extra_lines=$2
  {
    printf '%s\n' '# AGENTS'
    printf '%s\n' '## 4. Where to find'
    printf '%s\n' '| topic | doc |'
  } >"$file"
  local i=1
  while [ "$i" -le "$extra_lines" ]; do
    printf '# filler %s\n' "$i" >>"$file"
    i=$((i + 1))
  done
}

assert_success() {
  local name=$1
  local repo=$2
  shift 2

  local out="$tmp_root/${name}.out"
  if ! (cd "$repo" && bash scripts/check-agent-guide-lines.sh "$@") >"$out" 2>&1; then
    cat "$out" >&2
    fail "$name should have passed"
  fi
}

assert_failure_contains() {
  local name=$1
  local expected=$2
  local repo=$3
  shift 3

  local out="$tmp_root/${name}.out"
  if (cd "$repo" && bash scripts/check-agent-guide-lines.sh "$@") >"$out" 2>&1; then
    cat "$out" >&2
    fail "$name should have failed"
  fi

  if ! grep -Fq "$expected" "$out"; then
    cat "$out" >&2
    fail "$name should mention: $expected"
  fi
}

repo=$(new_repo pass-at-limit)
write_agents_with_section4 "$repo/AGENTS.md" 297
write_lines "$repo/CLAUDE.md" 300
git -C "$repo" add AGENTS.md CLAUDE.md scripts/check-agent-guide-lines.sh
assert_success pass-at-limit "$repo"

repo=$(new_repo fail-over-limit)
write_agents_with_section4 "$repo/AGENTS.md" 297
write_lines "$repo/CLAUDE.md" 301
git -C "$repo" add AGENTS.md CLAUDE.md scripts/check-agent-guide-lines.sh
assert_failure_contains fail-over-limit "301	CLAUDE.md" "$repo"

repo=$(new_repo fail-missing-section4)
write_lines "$repo/AGENTS.md" 50
write_lines "$repo/CLAUDE.md" 50
git -C "$repo" add AGENTS.md CLAUDE.md scripts/check-agent-guide-lines.sh
assert_failure_contains fail-missing-section4 "missing the '## 4. Where to find'" "$repo"

repo=$(new_repo staged-only)
write_agents_with_section4 "$repo/AGENTS.md" 297
write_lines "$repo/CLAUDE.md" 301
git -C "$repo" add AGENTS.md scripts/check-agent-guide-lines.sh
assert_success staged-only-unstaged-over-limit "$repo" --staged
git -C "$repo" add CLAUDE.md
assert_failure_contains staged-over-limit "301	CLAUDE.md" "$repo" --staged

echo "check-agent-guide-lines tests passed"
