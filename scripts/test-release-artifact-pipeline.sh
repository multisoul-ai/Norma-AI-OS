#!/usr/bin/env bash
# Final artifact seam: official Sparkle tool input and notarized validation order.
#
# Scenario:
#   The final stapled DMG is renamed once, signed by the official appcast tool
#   through stdin, and subjected to platform and metadata checks before upload.
#
# Data construction (including key numeric derivation):
#   canonical assets = 3: DMG + SHA-256 + appcast.xml
#   maximum deltas   = 0 for the first release phase
#
# Execution:
#   1. Inspect finalization and validation entrypoints.
#   2. Verify private key transport, immutable URL prefix, and canonical names.
#   3. Verify codesign, Gatekeeper, stapler, DMG layout, checksum, and appcast checks.
#
# Expected result:
#   - Positive: generate_appcast reads --ed-key-file - and receives stdin.
#   - Positive: all three canonical assets and tag-specific URL are enforced.
#   - Positive: the canonical release verifier re-checks Finder's custom layout.
#   - Negative: private key does not appear in argv and latest DMG URL is forbidden.
#   - Negative: a structurally valid but generic Finder DMG cannot be published.

set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
finalize="$repo_root/scripts/finalize-sparkle-release.sh"
verify="$repo_root/scripts/verify-release-artifacts.sh"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

for name in Norma-OS_aarch64.dmg Norma-OS_aarch64.dmg.sha256 appcast.xml; do
  grep -Fq "$name" "$finalize" "$verify" || fail "pipeline must enforce canonical asset $name"
done
grep -Fq 'unset SPARKLE_EDDSA_PRIVATE_KEY' "$finalize" \
  || fail "finalizer must remove the exported private key before invoking child processes"
grep -Fq "printf '%s' \"\$sparkle_private_key\" | \"\$generate_appcast\"" "$finalize" \
  || fail "private Sparkle key must enter generate_appcast through stdin"
grep -Fq -- '--ed-key-file -' "$finalize" || fail "generate_appcast must read its key from stdin"
grep -Fq -- '--maximum-deltas 0' "$finalize" || fail "first phase must disable delta generation"
grep -Fq 'releases/download/$tag/' "$finalize" || fail "DMG URL prefix must contain the concrete tag"
if grep -Fq 'releases/latest/download/Norma-OS_aarch64.dmg' "$finalize"; then
  fail "canonical DMG URL must never use releases/latest"
fi
for command in 'codesign --verify' 'spctl --assess' 'xcrun stapler validate' 'hdiutil verify' 'verify-sparkle-appcast.py'; do
  grep -Fq "$command" "$verify" || fail "artifact verifier must run $command"
done
grep -Fq 'verify-dmg-layout.sh" "$dmg"' "$verify" \
  || fail "artifact verifier must reject a release DMG without the customized Finder layout"

echo "release artifact pipeline test passed"
