#!/usr/bin/env bash
# Mechanized constraint: prohibit #[allow(...)] in Rust source files.
#
# Using #[allow] silences compiler and clippy diagnostics without fixing them.
# Fix the root cause instead: remove unused code, split responsibilities, group
# parameters into context structs, or wire the code into the real call path.
#
# Scope: Rust source under crates/*/{src,tests} and optional root src.
# Excludes: fixture paths, which may intentionally model external code.

set -euo pipefail

if [ "${1:-}" = "--staged" ]; then
  files=$(git diff --cached --name-only --diff-filter=ACMR)
else
  files=$(
    {
      git ls-files
      git ls-files --others --exclude-standard
    } | sort -u
  )
fi

if [ -z "$files" ]; then
  exit 0
fi

candidates=$(printf '%s\n' "$files" | awk '
  {
    is_source = ($0 ~ /^crates\/[^\/]+\/(src|tests)\/.*\.rs$/ || $0 ~ /^src\/.*\.rs$/)
    is_excluded = ($0 ~ /\/fixtures\// || $0 ~ /\/__fixtures__\//)
    if (is_source && !is_excluded) {
      print
    }
  }
')

if [ -z "$candidates" ]; then
  exit 0
fi

violations=""
while IFS= read -r file; do
  [ -f "$file" ] || continue
  matches=$(grep -nE '#\[[[:space:]]*allow[[:space:]]*\(' "$file" 2>/dev/null || true)
  if [ -n "$matches" ]; then
    while IFS= read -r match; do
      violations="${violations}  ${file}:${match}"$'\n'
    done <<<"$matches"
  fi
done <<<"$candidates"

if [ -n "$violations" ]; then
  echo "ERROR: #[allow(...)] found in Rust source. Fix the root cause; no exceptions are allowed." >&2
  echo "" >&2
  echo "Common fixes:" >&2
  echo "  dead_code                   -> remove unused code, or wire it into the call path" >&2
  echo "  unused_imports              -> delete the import" >&2
  echo "  unused_variables            -> prefix intentionally unused bindings with _" >&2
  echo "  clippy::too_many_arguments  -> group related params into a context struct" >&2
  echo "" >&2
  printf '%s' "$violations" >&2
  echo "" >&2
  echo "Doc: docs/quality/mechanized-constraints.md" >&2
  exit 1
fi

exit 0
