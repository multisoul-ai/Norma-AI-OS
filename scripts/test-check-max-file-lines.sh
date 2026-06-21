#!/usr/bin/env bash
# Focused tests for scripts/check-max-file-lines.sh.
#
# Scenarios covered:
#   1. A Rust source file with exactly 500 physical lines passes.
#   2. A Rust source file with 501 physical lines fails and reports the file.
#   3. Non-source and fixture files with 501 lines are excluded.
#   4. --staged checks only staged Rust source candidates.
#   5. Full-scan mode also catches untracked Rust source files.
#
# Data construction:
#   limit      = 500 lines
#   pass file  = 500 generated comment lines
#   fail file  = 501 generated comment lines, so fail_file - limit = 1
#
# Expected result:
#   - Positive: exact-limit files are accepted.
#   - Positive: over-limit source files are rejected with path and line count.
#   - Negative: docs and fixtures are not treated as production source files.

set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
lint_script="$repo_root/scripts/check-max-file-lines.sh"
tmp_root=$(mktemp -d)

trap 'rm -rf "$tmp_root"' EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

new_repo() {
  local name=$1
  local dir="$tmp_root/$name"
  mkdir -p "$dir"
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
    printf '// generated line %s\n' "$i" >>"$file"
    i=$((i + 1))
  done
}

assert_success() {
  local name=$1
  shift

  local out="$tmp_root/${name}.out"
  if ! "$@" >"$out" 2>&1; then
    cat "$out" >&2
    fail "$name should have passed"
  fi
}

assert_failure_contains() {
  local name=$1
  local expected=$2
  shift 2

  local out="$tmp_root/${name}.out"
  if "$@" >"$out" 2>&1; then
    cat "$out" >&2
    fail "$name should have failed"
  fi

  if ! grep -Fq "$expected" "$out"; then
    cat "$out" >&2
    fail "$name should mention: $expected"
  fi
}

# exact_limit_passes:
#   source file = crates/app/src/exact_limit.rs
#   lines       = 500
#   expectation = accepted because lines == limit, not greater than limit
exact_repo=$(new_repo exact-limit)
write_lines "$exact_repo/crates/app/src/exact_limit.rs" 500
git -C "$exact_repo" add crates/app/src/exact_limit.rs
assert_success "exact-limit" bash -c "cd '$exact_repo' && bash '$lint_script'"

# over_limit_fails:
#   source file = crates/app/src/too_long.rs
#   lines       = 501
#   overage     = 501 - 500 = 1
#   expectation = rejected with concrete path and count
fail_repo=$(new_repo over-limit)
write_lines "$fail_repo/crates/app/src/too_long.rs" 501
git -C "$fail_repo" add crates/app/src/too_long.rs
assert_failure_contains \
  "over-limit" \
  $'501\tcrates/app/src/too_long.rs' \
  bash -c "cd '$fail_repo' && bash '$lint_script'"

# exclusions_pass:
#   docs/oversized.rs                  = 501 lines, outside source scope
#   crates/app/src/fixtures/large.rs   = 501 lines, fixture data excluded
#   crates/app/src/production.rs       = 500 lines, production source at limit
#   expectation = accepted; excluded files do not mask production source behavior
exclude_repo=$(new_repo exclusions)
write_lines "$exclude_repo/docs/oversized.rs" 501
write_lines "$exclude_repo/crates/app/src/fixtures/large.rs" 501
write_lines "$exclude_repo/crates/app/src/production.rs" 500
git -C "$exclude_repo" add docs/oversized.rs crates/app/src/fixtures/large.rs crates/app/src/production.rs
assert_success "exclusions" bash -c "cd '$exclude_repo' && bash '$lint_script'"

# staged_mode_checks_staged_source:
#   unstaged source = 501 lines, but --staged sees no ACMR diff yet
#   staged source   = same 501-line file after git add
#   expectation     = first command passes, second command rejects exact file
staged_repo=$(new_repo staged-mode)
write_lines "$staged_repo/crates/app/src/staged_too_long.rs" 501
assert_success "staged-mode-empty" bash -c "cd '$staged_repo' && bash '$lint_script' --staged"
git -C "$staged_repo" add crates/app/src/staged_too_long.rs
assert_failure_contains \
  "staged-mode-fails" \
  $'501\tcrates/app/src/staged_too_long.rs' \
  bash -c "cd '$staged_repo' && bash '$lint_script' --staged"

# full_scan_checks_untracked_source:
#   untracked source = 501 lines
#   tracked files    = none
#   expectation      = full scan rejects current working tree source, even before staging
untracked_repo=$(new_repo untracked-source)
write_lines "$untracked_repo/crates/app/src/untracked_too_long.rs" 501
assert_failure_contains \
  "untracked-source" \
  $'501\tcrates/app/src/untracked_too_long.rs' \
  bash -c "cd '$untracked_repo' && bash '$lint_script'"

echo "check-max-file-lines tests passed"
