#!/usr/bin/env bash
# Mechanized constraint: AGENTS.md and CLAUDE.md must stay at or below 300 lines.
#
# When content grows beyond the cap, archive detail under docs/ and add a pointer
# row to AGENTS.md § "4. Where to find". Do not raise the limit casually.
#
# Background: docs/quality/mechanized-constraints.md

set -euo pipefail

limit=300
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

guides=(AGENTS.md CLAUDE.md)

if [ "${1:-}" = "--staged" ]; then
  staged=$(git diff --cached --name-only --diff-filter=ACMR || true)
  selected=()
  for guide in "${guides[@]}"; do
    if printf '%s\n' "$staged" | grep -Fxq "$guide"; then
      selected+=("$guide")
    fi
  done
  if [ "${#selected[@]}" -eq 0 ]; then
    exit 0
  fi
  guides=("${selected[@]}")
fi

violations=""
for guide in "${guides[@]}"; do
  [ -f "$guide" ] || continue
  line_count=$(wc -l <"$guide" | tr -d ' ')
  if [ "$line_count" -gt "$limit" ]; then
    violations="${violations}${line_count}	${guide}"$'\n'
  fi
done

if [ -n "$violations" ]; then
  echo "ERROR: Agent guide file(s) exceed ${limit} lines." >&2
  echo "" >&2
  echo "LLM / contributor hint: keep AGENTS.md and CLAUDE.md as navigation maps." >&2
  echo "Move expanded prose to docs/ (for example docs/references/ or docs/quality/)" >&2
  echo "and add a pointer row under AGENTS.md ## 4. Where to find." >&2
  echo "Do not raise the line cap casually." >&2
  echo "" >&2
  printf '%s\n' "$violations" | sort -rn >&2
  echo "" >&2
  echo "Doc: docs/quality/mechanized-constraints.md" >&2
  exit 1
fi

if ! grep -Fq '## 4. Where to find' AGENTS.md; then
  echo "ERROR: AGENTS.md is missing the '## 4. Where to find' navigation section." >&2
  echo "Archived docs must be linked from that table." >&2
  exit 1
fi

exit 0
