#!/usr/bin/env bash
# Focused test for the project cargo-packager wrapper.
#
# Scenario:
#   `cargo packager --release -c Packager.toml` should be safe for local daily
#   packaging even when the user's shell contains Apple notary credentials.
#
# Data construction:
#   fake cargo-packager = temp executable earlier in PATH
#   default command     = wrapper --release -c Packager.toml
#   notarize command    = wrapper --notarize --release -c Packager.toml
#   stale notary env    = APPLE_ID / APPLE_PASSWORD / APPLE_TEAM_ID / APPLE_API_*
#   release API env     = APP_STORE_CONNECT_API_KEY_ID / _ISSUER_ID / _KEY_PATH
#
# Execution:
#   1. Verify Cargo aliases `cargo packager` to the wrapper manifest.
#   2. Run `cargo packager` in default mode against a fake cargo-packager.
#   3. Verify default mode forwards original cargo-packager args.
#   4. Verify default mode removes every notary credential env var.
#   5. Verify the real cargo-packager process still starts in the repository root.
#   6. Run the wrapper with --notarize against the same fake cargo-packager.
#   7. Verify --notarize is stripped before forwarding to cargo-packager.
#   8. Verify --notarize maps APP_STORE_CONNECT_* into APPLE_API_*.
#
# Expected result:
#   - Positive: default mode forwards `--release -c Packager.toml`.
#   - Positive: default mode removes stale APPLE_* notary credentials.
#   - Positive: relative `Packager.toml` resolves from the repository root.
#   - Positive: --notarize mode injects APPLE_API_* from APP_STORE_CONNECT_*.
#   - Negative: cargo-packager never receives the wrapper-only --notarize flag.
#   - Negative: personal Apple ID credentials are not forwarded in --notarize mode.

set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
wrapper_manifest="$repo_root/packaging/cargo-packager-wrapper/Cargo.toml"
cargo_config="$repo_root/.cargo/config.toml"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

grep -Fq 'packager = "run --quiet --manifest-path packaging/cargo-packager-wrapper/Cargo.toml --"' "$cargo_config" \
  || fail "cargo packager must route through the project wrapper"

tmp_dir=$(mktemp -d)
trap 'rm -rf "$tmp_dir"' EXIT

fake_bin="$tmp_dir/bin"
mkdir -p "$fake_bin"
cat >"$fake_bin/cargo-packager" <<'FAKE'
#!/usr/bin/env bash
{
  printf 'args=%s\n' "$*"
  printf 'cwd=%s\n' "$(pwd -P)"
  printf 'APPLE_ID=%s\n' "${APPLE_ID-<unset>}"
  printf 'APPLE_PASSWORD=%s\n' "${APPLE_PASSWORD-<unset>}"
  printf 'APPLE_TEAM_ID=%s\n' "${APPLE_TEAM_ID-<unset>}"
  printf 'APPLE_KEYCHAIN_PROFILE=%s\n' "${APPLE_KEYCHAIN_PROFILE-<unset>}"
  printf 'APPLE_API_KEY=%s\n' "${APPLE_API_KEY-<unset>}"
  printf 'APPLE_API_ISSUER=%s\n' "${APPLE_API_ISSUER-<unset>}"
  printf 'APPLE_API_KEY_PATH=%s\n' "${APPLE_API_KEY_PATH-<unset>}"
} >"$CAPTURE_FILE"
FAKE
chmod +x "$fake_bin/cargo-packager"

default_capture="$tmp_dir/default.env"
CAPTURE_FILE="$default_capture" \
PATH="$fake_bin:$PATH" \
APPLE_ID="person@example.com" \
APPLE_PASSWORD="app-password" \
APPLE_TEAM_ID="PERSONALTEAM" \
APPLE_KEYCHAIN_PROFILE="personal-profile" \
APPLE_API_KEY="stale-key" \
APPLE_API_ISSUER="stale-issuer" \
APPLE_API_KEY_PATH="/tmp/stale.p8" \
  cargo packager --release -c Packager.toml

grep -Fq 'args=--release -c Packager.toml' "$default_capture" \
  || fail "default wrapper mode must forward cargo-packager args unchanged"

grep -Fq "cwd=$repo_root" "$default_capture" \
  || fail "relative Packager.toml must resolve from the repository root"

for name in APPLE_ID APPLE_PASSWORD APPLE_TEAM_ID APPLE_KEYCHAIN_PROFILE APPLE_API_KEY APPLE_API_ISSUER APPLE_API_KEY_PATH; do
  grep -Fq "$name=<unset>" "$default_capture" \
    || fail "default wrapper mode must unset $name"
done

notarize_capture="$tmp_dir/notarize.env"
CAPTURE_FILE="$notarize_capture" \
PATH="$fake_bin:$PATH" \
APPLE_ID="person@example.com" \
APPLE_PASSWORD="app-password" \
APPLE_TEAM_ID="PERSONALTEAM" \
APPLE_KEYCHAIN_PROFILE="personal-profile" \
APPLE_API_KEY="stale-key" \
APPLE_API_ISSUER="stale-issuer" \
APPLE_API_KEY_PATH="/tmp/stale.p8" \
APP_STORE_CONNECT_API_KEY_ID="company-key" \
APP_STORE_CONNECT_API_ISSUER_ID="company-issuer" \
APP_STORE_CONNECT_API_KEY_PATH="/secure/AuthKey_company-key.p8" \
  cargo packager --notarize --release -c Packager.toml

grep -Fq 'args=--release -c Packager.toml' "$notarize_capture" \
  || fail "--notarize must be a wrapper-only flag and not reach cargo-packager"

grep -Fq 'APPLE_API_KEY=company-key' "$notarize_capture" \
  || fail "--notarize must map APP_STORE_CONNECT_API_KEY_ID to APPLE_API_KEY"

grep -Fq 'APPLE_API_ISSUER=company-issuer' "$notarize_capture" \
  || fail "--notarize must map APP_STORE_CONNECT_API_ISSUER_ID to APPLE_API_ISSUER"

grep -Fq 'APPLE_API_KEY_PATH=/secure/AuthKey_company-key.p8' "$notarize_capture" \
  || fail "--notarize must map APP_STORE_CONNECT_API_KEY_PATH to APPLE_API_KEY_PATH"

grep -Fq 'APPLE_ID=<unset>' "$notarize_capture" \
  || fail "--notarize must not forward personal Apple ID credentials"

echo "cargo-packager wrapper test passed"
