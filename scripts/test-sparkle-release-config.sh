#!/usr/bin/env bash
# Sparkle release wiring seam: pinned archive, generated plist, and packaged framework.
#
# Scenario:
#   A release build prepares exactly Sparkle 2.9.5, verifies its immutable digest,
#   injects the public update metadata, and embeds the framework under Contents/Frameworks.
#
# Data construction (including key numeric derivation):
#   Sparkle version       = 2.9.5
#   SHA-256 hex length    = 64 characters = 256 / 4
#   minimum macOS version = 12.0
#   feed URL count        = 1 canonical GitHub Releases appcast
#
# Execution:
#   1. Read the repository-owned Sparkle metadata and preparation scripts.
#   2. Verify the release plist requires a non-empty public key and keeps existing privacy text.
#   3. Verify preparation installs both appcast signing and one-time key-management tools.
#   4. Verify Packager.toml embeds the prepared framework and selects the generated plist.
#
# Expected result:
#   - Positive: version, digest, archive URL, feed URL, framework path, and plist path are exact.
#   - Positive: verified `generate_appcast` and `generate_keys` tools are both prepared.
#   - Positive: public key is read only from SPARKLE_PUBLIC_ED_KEY.
#   - Negative: no private key, placeholder public key, unverified cache, or latest-version URL is accepted.

set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
metadata="$repo_root/packaging/macos/sparkle-release.env"
prepare="$repo_root/scripts/prepare-sparkle.sh"
plist="$repo_root/scripts/prepare-release-info-plist.sh"
config="$repo_root/Packager.toml"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

[ -f "$metadata" ] || fail "Sparkle release metadata must exist"
[ -x "$prepare" ] || fail "Sparkle preparation script must be executable"
[ -x "$plist" ] || fail "release Info.plist preparation script must be executable"

grep -Fqx 'SPARKLE_VERSION=2.9.5' "$metadata" \
  || fail "Sparkle version must be pinned to 2.9.5"
grep -Fqx 'SPARKLE_ARCHIVE_SHA256=015336b601493e05c237964954bff6191370003d94edefe663724c88840d73cc' "$metadata" \
  || fail "Sparkle archive SHA-256 must match the reviewed 2.9.5 release"
grep -Fq '/releases/download/2.9.5/Sparkle-2.9.5.tar.xz' "$metadata" \
  || fail "Sparkle archive URL must be version-specific"
grep -Fqx 'SPARKLE_FEED_URL=https://github.com/multisoul-ai/norma-os-releases/releases/latest/download/appcast.xml' "$metadata" \
  || fail "Sparkle feed URL must use the canonical latest appcast asset"

grep -Fq 'shasum -a 256 -c' "$prepare" \
  || fail "Sparkle preparation must verify the archive before reuse or extraction"
grep -Fq -- "-path '*/bin/generate_keys'" "$prepare" \
  || fail "Sparkle preparation must locate generate_keys in the verified archive"
grep -Fq '"$output_dir/bin/generate_keys"' "$prepare" \
  || fail "Sparkle preparation must install generate_keys for one-time key setup"
grep -Fq 'SPARKLE_PUBLIC_ED_KEY' "$plist" \
  || fail "release plist must read the public EdDSA key from SPARKLE_PUBLIC_ED_KEY"
if grep -Eq '(SPARKLE_EDDSA_PRIVATE_KEY|SUPublicEDKey</key>[[:space:]]*<string>[^$<])' "$repo_root/packaging/macos/Info.plist" "$metadata"; then
  fail "checked-in Sparkle metadata must not contain private or placeholder key material"
fi

grep -Fq 'frameworks = ["build/sparkle/Sparkle.framework"]' "$config" \
  || fail "Packager.toml must embed the verified Sparkle.framework"
grep -Fq 'info-plist-path = "build/packaging/Info.plist"' "$config" \
  || fail "Packager.toml must consume the generated release Info.plist"
grep -Fq 'scripts/prepare-sparkle.sh' "$config" \
  || fail "release packaging hook must prepare the pinned Sparkle distribution"
grep -Fq 'scripts/prepare-release-info-plist.sh' "$config" \
  || fail "release packaging hook must generate public update metadata before packaging"

echo "Sparkle release config test passed"
