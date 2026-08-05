#!/usr/bin/env bash
# Focused test for release-local.sh notarization mode selection.
#
# Scenario:
#   Local packaging should avoid Apple notarization by default. The script should
#   submit to notarytool only when the caller explicitly passes --notarize.
#
# Data construction:
#   default command       = ./scripts/release-local.sh
#   notarize command     = ./scripts/release-local.sh --notarize
#   notarize credentials = APP_STORE_CONNECT_* mapped to APPLE_API_*
#   skip credentials     = unset APPLE_API_* / APPLE_ID_* before cargo-packager
#
# Execution:
#   1. Read scripts/release-local.sh from the repository root.
#   2. Verify NOTARIZE starts false.
#   3. Verify --notarize flips NOTARIZE true.
#   4. Verify default cargo-packager execution unsets all notary credential env.
#   5. Verify notarize execution maps APP_STORE_CONNECT_* to APPLE_API_*.
#   6. Verify release execution invokes the external cargo-packager directly,
#      bypassing the workspace target runner used only for the development app.
#
# Expected result:
#   - Positive: --notarize is an explicit accepted flag.
#   - Positive: default mode removes APPLE_API_* and APPLE_ID_* before packaging.
#   - Positive: notarize mode injects APPLE_API_* from APP_STORE_CONNECT_*.
#   - Positive: notarize mode calls cargo-packager after sanitizing notary env.
#   - Negative: default mode must not require stapler validation.
#   - Negative: release-local must not route its tooling binary through `cargo run`.
#   - Negative: unknown options still fail fast.

set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
script="$repo_root/scripts/release-local.sh"
packaging_doc="$repo_root/docs/references/packaging.md"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

grep -Fq 'NOTARIZE=false' "$script" \
  || fail "release-local.sh must default to NOTARIZE=false"

grep -Fq -- '--notarize) NOTARIZE=true ;;' "$script" \
  || fail "release-local.sh must accept --notarize and enable notarization explicitly"

grep -Fq -- '-u APPLE_API_KEY -u APPLE_API_ISSUER -u APPLE_API_KEY_PATH' "$script" \
  || fail "default packaging must unset APPLE_API_* so cargo-packager skips notarization"

grep -Fq -- '-u APPLE_KEYCHAIN_PROFILE -u APPLE_ID -u APPLE_PASSWORD -u APPLE_TEAM_ID' "$script" \
  || fail "default packaging must unset Apple ID notary credentials"

grep -Fq 'export APPLE_API_KEY="$APP_STORE_CONNECT_API_KEY_ID"' "$script" \
  || fail "notarize mode must map APP_STORE_CONNECT_API_KEY_ID to APPLE_API_KEY"

grep -Fq 'export APPLE_API_ISSUER="$APP_STORE_CONNECT_API_ISSUER_ID"' "$script" \
  || fail "notarize mode must map APP_STORE_CONNECT_API_ISSUER_ID to APPLE_API_ISSUER"

grep -Fq 'export APPLE_API_KEY_PATH="$APP_STORE_CONNECT_API_KEY_PATH"' "$script" \
  || fail "notarize mode must map APP_STORE_CONNECT_API_KEY_PATH to APPLE_API_KEY_PATH"

grep -Fq 'cargo-packager --release -c Packager.toml' "$script" \
  || fail "release-local must invoke external cargo-packager after preparing notarization credentials"

if grep -Eq '^[[:space:]]+cargo packager ' "$script"; then
  fail "release-local must not run its tooling binary through the development target runner"
fi

grep -Fq 'xcrun stapler validate "$APP" || die "stapler 无公证票据（--notarize 必须有）"' "$script" \
  || fail "only --notarize mode should require a stapled notarization ticket"

grep -Fq '*) die "未知参数: $arg" ;;' "$script" \
  || fail "unknown release-local.sh options must fail fast"

grep -Fq './scripts/release-local.sh --notarize' "$packaging_doc" \
  || fail "packaging docs must document --notarize as the explicit notarization entrypoint"

grep -Fq 'cargo packager --notarize --release -c Packager.toml' "$packaging_doc" \
  || fail "packaging docs must document cargo packager --notarize"

grep -Fq '只有 `release-local.sh` 的直接调用会先完成环境清理' "$packaging_doc" \
  || fail "packaging docs must distinguish the sanitized release entrypoint from a bare cargo-packager call"

echo "release-local notarize mode test passed"
