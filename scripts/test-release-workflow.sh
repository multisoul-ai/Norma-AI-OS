#!/usr/bin/env bash
# GitHub publication seam: manual main-only draft release with late installation token.
#
# Scenario:
#   A release job may run only by explicit dispatch from main, keeps an in-flight
#   release alive, finishes all local notarization work before requesting the
#   short-lived GitHub App token, and publishes only after remote verification.
#
# Data construction (including key numeric derivation):
#   GitHub installation token lifetime ≈ 1 hour
#   potentially long pre-token work = Rust build + Apple notarization + appcast
#   canonical remote assets = 3 (DMG + SHA-256 + appcast)
#
# Execution:
#   1. Read the release workflow and repository publication script.
#   2. Verify dispatch/branch/concurrency/runner/tool pins and full action SHAs.
#   3. Verify the manifest version must be newer than the public latest release.
#   4. Compare step order: local artifact verification precedes token creation.
#   5. Verify draft recovery, conflict refusal, remote readback, publish, provenance, and bump PR guards.
#
# Expected result:
#   - Positive: main/manual/macos-15/Rust 1.91.1/cargo-packager 0.11.8 are exact.
#   - Positive: GitHub App token is created after notarized local assets pass.
#   - Positive: draft is published only after three downloaded assets match.
#   - Negative: an equal or older public latest version cannot enter the build.
#   - Negative: no push trigger, force push, clobber upload, early token, or non-draft overwrite exists.

set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
workflow="$repo_root/.github/workflows/release.yml"
publisher="$repo_root/scripts/publish-github-release.sh"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

[ -f "$workflow" ] || fail "release workflow must exist"
[ -x "$publisher" ] || fail "GitHub release publisher must be executable"

grep -Fq 'workflow_dispatch:' "$workflow" || fail "release workflow must be manually dispatched"
if grep -Eq '(inputs\.version|REQUESTED_VERSION|\$\{\{ inputs\.version \}\})' "$workflow"; then
  fail "release version must be derived from synchronized manifests without manual version input"
fi
if grep -Eq '^[[:space:]]+(push|pull_request|schedule):' "$workflow"; then
  fail "release workflow must not have an automatic trigger"
fi
grep -Fq 'cancel-in-progress: false' "$workflow" || fail "release concurrency must preserve in-flight jobs"
grep -Fq 'DISPATCH_REF: ${{ github.ref }}' "$workflow" || fail "workflow must inspect the dispatched ref at runtime"
grep -Fq '[ "$DISPATCH_REF" = "refs/heads/main" ]' "$workflow" || fail "workflow must fail non-main dispatches"
grep -Fq 'runs-on: macos-15' "$workflow" || fail "release runner must be macos-15"
grep -Fq 'environment: release' "$workflow" || fail "release job must use the protected release Environment"
grep -Fq 'RUST_TOOLCHAIN: 1.91.1' "$workflow" || fail "Rust must be pinned to 1.91.1"
grep -Fq 'CARGO_PACKAGER_VERSION: 0.11.8' "$workflow" || fail "cargo-packager must be pinned to 0.11.8"
grep -Fq 'actions/checkout@11d5960a326750d5838078e36cf38b85af677262' "$workflow" \
  || fail "checkout must use the reviewed full SHA"
grep -Fq 'actions/create-github-app-token@fee1f7d63c2ff003460e3d139729b119787bc349' "$workflow" \
  || fail "GitHub App token action must use the reviewed full SHA"

verify_line=$(grep -n 'Verify local release assets' "$workflow" | cut -d: -f1)
token_line=$(grep -n 'Create short-lived GitHub App token' "$workflow" | cut -d: -f1)
[ -n "$verify_line" ] && [ -n "$token_line" ] && [ "$verify_line" -lt "$token_line" ] \
  || fail "GitHub App token must be created only after local build/notarization/appcast verification"

build_start=$(grep -n 'Build, sign, notarize, and staple' "$workflow" | cut -d: -f1)
appcast_start=$(grep -n 'Generate signed Sparkle appcast from notarized DMG' "$workflow" | cut -d: -f1)
[ -n "$build_start" ] && [ -n "$appcast_start" ] && [ "$build_start" -lt "$appcast_start" ] \
  || fail "Sparkle signing must run after the build/notarization step"
if sed -n "${build_start},$((appcast_start - 1))p" "$workflow" | grep -Fq 'SPARKLE_EDDSA_PRIVATE_KEY'; then
  fail "permanent Sparkle private key must not be exposed to cargo build or dependency build scripts"
fi
grep -Fq 'apple-tool:,apple:,codesign:' "$workflow" \
  || fail "temporary keychain partition list must authorize codesign explicitly"

grep -Fq 'draft' "$publisher" || fail "publisher must create or resume a Draft Release"
grep -Fq 'remote asset digest mismatch' "$publisher" || fail "publisher must reject conflicting draft assets"
grep -Fq 'make_latest' "$publisher" || fail "publisher must explicitly publish as latest"
grep -Fq 'gh release download' "$publisher" || fail "publisher must download remote assets for readback verification"
grep -Fq 'releases/latest/download/$name' "$publisher" \
  || fail "publisher must read back each asset through the unauthenticated public latest URL"
grep -Fq 'public latest URL readback did not match' "$publisher" \
  || fail "bounded public propagation retries must fail closed"
grep -Fq 'for attempt in 1 2 3 4 5 6' "$publisher" \
  || fail "public latest propagation retry count must remain explicitly bounded"
if grep -Eq '(--clobber|push --force|push -f)' "$workflow" "$publisher"; then
  fail "release automation must not overwrite assets or force-push refs"
fi
grep -Fq 'scripts/bump-release-version.sh' "$workflow" || fail "successful release must prepare the next patch PR"
grep -Fq 'VERSION=$(scripts/release-version.sh)' "$workflow" \
  || fail "workflow must derive the release version from the read-only manifest seam"
grep -Fq 'api.github.com/repos/multisoul-ai/norma-os-releases/releases/latest' "$workflow" \
  || fail "workflow must read the current public latest release before building"
grep -Fq -- '--require-newer-than "$LATEST_TAG"' "$workflow" \
  || fail "workflow must reject a manifest version that is not newer than public latest"
grep -Fq 'git tag -a' "$workflow" || fail "successful release must create a source provenance tag"

echo "release workflow test passed"
