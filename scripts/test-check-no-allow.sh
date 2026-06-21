#!/usr/bin/env bash
# Focused tests for scripts/check-no-allow.sh.
#
# Scenarios covered:
#   1. A production Rust source file containing #[allow(...)] is rejected.
#   2. A clean production Rust source file is accepted.
#   3. A crate-local test Rust file containing #[allow(...)] is rejected.
#   4. Fixture Rust files are excluded from the rule.
#   5. --staged checks the staged Rust source candidate set.
#
# Data construction:
#   bad file      = crates/app/src/bad.rs with one #[allow(dead_code)] line
#   clean file    = crates/app/src/clean.rs with no #[allow(...)] attributes
#   test file     = crates/app/tests/unit/bad_test.rs with one #[allow(dead_code)] line
#   fixture file  = crates/app/tests/fixtures/allowed_fixture.rs, outside rule scope
#
# Expected result:
#   - Positive: clean source and fixture-only repositories are accepted.
#   - Negative: checked source containing #[allow(...)] is rejected with path and line.

set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
lint_script="$repo_root/scripts/check-no-allow.sh"
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

write_file() {
  local file=$1
  local body=$2
  mkdir -p "$(dirname "$file")"
  printf '%s\n' "$body" >"$file"
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

# production_allow_fails:
#   source file = crates/app/src/bad.rs
#   line 1      = #[allow(dead_code)]
#   expectation = rejected with exact path and line number
bad_repo=$(new_repo production-allow)
write_file "$bad_repo/crates/app/src/bad.rs" '#[allow(dead_code)]
fn unused() {}'
git -C "$bad_repo" add crates/app/src/bad.rs
assert_failure_contains \
  "production-allow" \
  "crates/app/src/bad.rs:1:" \
  bash -c "cd '$bad_repo' && bash '$lint_script'"

# test_allow_fails:
#   test file   = crates/app/tests/unit/bad_test.rs
#   line 1      = #[allow(dead_code)]
#   expectation = rejected with exact path and line number
bad_test_repo=$(new_repo test-allow)
write_file "$bad_test_repo/crates/app/tests/unit/bad_test.rs" '#[allow(dead_code)]
fn unused_test_helper() {}'
git -C "$bad_test_repo" add crates/app/tests/unit/bad_test.rs
assert_failure_contains \
  "test-allow" \
  "crates/app/tests/unit/bad_test.rs:1:" \
  bash -c "cd '$bad_test_repo' && bash '$lint_script'"

# clean_source_passes:
#   source file = crates/app/src/clean.rs
#   content     = a small function without #[allow(...)]
#   expectation = accepted because no suppression attribute exists
clean_repo=$(new_repo clean-source)
write_file "$clean_repo/crates/app/src/clean.rs" 'fn used() -> usize { 1 }'
git -C "$clean_repo" add crates/app/src/clean.rs
assert_success "clean-source" bash -c "cd '$clean_repo' && bash '$lint_script'"

# fixtures_are_excluded:
#   fixture file = crates/app/tests/fixtures/allowed_fixture.rs
#   line 1       = #[allow(dead_code)]
#   expectation  = accepted because fixture paths are outside production scope
fixture_repo=$(new_repo fixture-excluded)
write_file "$fixture_repo/crates/app/tests/fixtures/allowed_fixture.rs" '#[allow(dead_code)]
fn fixture_helper() {}'
git -C "$fixture_repo" add crates/app/tests/fixtures/allowed_fixture.rs
assert_success "fixture-excluded" bash -c "cd '$fixture_repo' && bash '$lint_script'"

# staged_mode_checks_only_staged_source:
#   unstaged source = contains #[allow(dead_code)] but is not staged yet
#   staged source   = same file after git add
#   expectation     = first command passes, second command rejects exact file
staged_repo=$(new_repo staged-mode)
write_file "$staged_repo/crates/app/src/staged_bad.rs" '#[allow(dead_code)]
fn staged_bad() {}'
assert_success "staged-mode-empty" bash -c "cd '$staged_repo' && bash '$lint_script' --staged"
git -C "$staged_repo" add crates/app/src/staged_bad.rs
assert_failure_contains \
  "staged-mode-fails" \
  "crates/app/src/staged_bad.rs:1:" \
  bash -c "cd '$staged_repo' && bash '$lint_script' --staged"

echo "check-no-allow tests passed"
