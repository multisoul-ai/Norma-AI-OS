#!/usr/bin/env bash
# Focused test for the project-local release-dmg skill and deterministic DMG handoff.
#
# Scenario:
#   A future agent should turn requests such as “打包 DMG” or “正式发布 DMG” into
#   the repository-owned release workflow without bypassing signing/notarization guards.
#
# Data construction:
#   skill directory     = .agents/skills/release-dmg
#   local package mode  = ./scripts/release-local.sh
#   public release mode = ./scripts/release-local.sh --notarize
#   output directory    = build/
#   checksum algorithm  = SHA-256 via shasum -a 256
#
# Execution:
#   1. Verify the project-local skill and UI metadata exist.
#   2. Verify its trigger metadata covers build, package, DMG, and release requests.
#   3. Verify local and notarized requests route through scripts/release-local.sh.
#   4. Verify the release script validates a DMG and emits its SHA-256 checksum.
#   5. Verify AGENTS.md exposes the skill as the project packaging entrypoint.
#
# Expected result:
#   - Positive: local packaging uses the default release-local mode.
#   - Positive: public release uses the explicit --notarize mode.
#   - Positive: the handoff reports a concrete DMG path and SHA-256 checksum.
#   - Negative: the skill does not bypass the Cargo wrapper with cargo-packager.
#   - Negative: the skill does not upload or publish artifacts to an external service.

set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
skill_dir="$repo_root/.agents/skills/release-dmg"
skill_file="$skill_dir/SKILL.md"
skill_metadata="$skill_dir/agents/openai.yaml"
release_script="$repo_root/scripts/release-local.sh"
agent_guide="$repo_root/AGENTS.md"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

require_file() {
  local path="$1"
  local reason="$2"
  [ -f "$path" ] || fail "$reason: $path"
}

require_text() {
  local needle="$1"
  local path="$2"
  local reason="$3"
  grep -Fq -- "$needle" "$path" || fail "$reason"
}

reject_text() {
  local needle="$1"
  local path="$2"
  local reason="$3"
  if grep -Fq -- "$needle" "$path"; then
    fail "$reason"
  fi
}

require_file "$skill_file" "release-dmg skill must provide SKILL.md"
require_file "$skill_metadata" "release-dmg skill must provide agents/openai.yaml"

if git -C "$repo_root" check-ignore -q "$skill_file"; then
  fail "release-dmg skill must be version-controlled instead of hidden by .gitignore"
fi

require_text 'name: release-dmg' "$skill_file" \
  "release-dmg SKILL.md must declare the canonical skill name"
require_text '打包' "$skill_file" \
  "release-dmg metadata must trigger for Chinese packaging requests"
require_text 'DMG' "$skill_file" \
  "release-dmg metadata must trigger for DMG requests"
require_text '发布' "$skill_file" \
  "release-dmg metadata must trigger for release requests"
require_text './scripts/release-local.sh' "$skill_file" \
  "local DMG packaging must route through scripts/release-local.sh"
require_text './scripts/release-local.sh --notarize' "$skill_file" \
  "public DMG release must explicitly request notarization"
require_text 'build/' "$skill_file" \
  "the skill must tell agents where to find packaged artifacts"
require_text 'SHA-256' "$skill_file" \
  "the skill must report an integrity checksum during handoff"
require_text 'display_name: "Release DMG"' "$skill_metadata" \
  "release-dmg UI metadata must expose a recognizable display name"
require_text 'default_prompt: "使用 $release-dmg' "$skill_metadata" \
  "release-dmg UI metadata must provide a Chinese invocation prompt"

require_text 'DMG=' "$release_script" \
  "release-local.sh must resolve the concrete DMG artifact"
require_text 'hdiutil verify "$DMG"' "$release_script" \
  "release-local.sh must verify that the produced DMG is readable"
require_text 'shasum -a 256 "$DMG"' "$release_script" \
  "release-local.sh must emit the DMG SHA-256 checksum"
require_text 'stat -f '\''%z'\'' "$DMG"' "$release_script" \
  "release-local.sh must report the concrete DMG size in bytes"

require_text '`release-dmg`' "$agent_guide" \
  "AGENTS.md must route packaging and release requests to release-dmg"

reject_text 'cargo-packager ' "$skill_file" \
  "the skill must not bypass the repository Cargo wrapper with cargo-packager"
reject_text 'gh release upload' "$skill_file" \
  "the skill must not upload a DMG without explicit external-release authorization"
reject_text 'aws s3' "$skill_file" \
  "the skill must not upload a DMG to external storage"

echo "release-dmg skill test passed"
