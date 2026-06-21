#!/usr/bin/env bash
# Mechanized constraint: single Rust source file must not exceed 500 lines.
#
# Scope: Rust source under crates/*/{src,tests} and optional root src.
#
# Excludes: fixtures and test fixture-style paths. Tests count because they
# contribute to the same review surface.
#
# On failure: print paths + line counts and ask contributors / LLM to split
# modules, extract helpers, and deduplicate instead of raising the cap.
#
# Background: docs/quality/mechanized-constraints.md

set -euo pipefail

limit=500

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
  line_count=$(wc -l <"$file" | tr -d ' ')

  if [ "$line_count" -gt "$limit" ]; then
    violations="${violations}${line_count}	${file}"$'\n'
  fi
done <<<"$candidates"

if [ -n "$violations" ]; then
  echo "ERROR: Rust source file(s) exceed ${limit} lines." >&2
  echo "" >&2
  echo "LLM / contributor hint: split by responsibility, extract shared helpers," >&2
  echo "and keep each source file under the cap. Do not raise the limit casually." >&2
  echo "" >&2
  printf '%s\n' "$violations" | sort -rn >&2
  echo "" >&2
  echo "Doc: docs/quality/mechanized-constraints.md" >&2
  exit 1
fi

exit 0
