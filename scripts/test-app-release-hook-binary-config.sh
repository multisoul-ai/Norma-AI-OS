#!/usr/bin/env bash
# Focused test for `cargo build --release -p app` helper binary wiring.
#
# Scenario:
#   A release app build must place both helper binaries next to `norma-os` and
#   carry the Chrome extension whose key matches the Native Messaging origin.
#
# Data construction:
#   app manifest  = crates/app/Cargo.toml
#   app bin       = norma-os
#   required bin  = multisoul-agent-hook
#   wrapper path  = src/bin/multisoul_agent_hook.rs
#   auth bin      = norma-browser-auth-host
#   auth wrapper  = src/bin/norma_browser_auth_host.rs
#   extension id  = SHA-256(manifest public key DER) first 16 bytes, nibble a-p
#
# Execution:
#   1. Read the app manifest from the repository root.
#   2. Verify the app package declares a secondary hook binary.
#   3. Verify `cargo run -p app` still defaults to the app binary.
#   4. Verify the wrapper source delegates to the shared norma hook CLI.
#   5. Verify Packager includes the auth helper and extension Resources.
#   6. Derive the extension ID and verify the host authorizes exactly that origin.
#
# Expected result:
#   - Positive: app manifest contains a `norma-os` main bin target.
#   - Positive: app manifest contains a `multisoul-agent-hook` bin target.
#   - Positive: app manifest sets `default-run = "norma-os"`.
#   - Positive: wrapper source exists and calls `norma::agent_hook::cli::main`.
#   - Positive: release package includes `norma-browser-auth-host` and extension resources.
#   - Positive: fixed manifest key derives the exact Native Messaging allowed origin.
#   - Negative: app manifest does not keep `default-run = "multisoul"`.
#   - Negative: app manifest does not keep `name = "multisoul"` for the main app.
#   - Negative: relying only on Packager.toml or the norma package bin fails.

set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
manifest="$repo_root/crates/app/Cargo.toml"
wrapper="$repo_root/crates/app/src/bin/multisoul_agent_hook.rs"
auth_wrapper="$repo_root/crates/app/src/bin/norma_browser_auth_host.rs"
packager="$repo_root/Packager.toml"
extension_manifest="$repo_root/crates/app/assets/browser-auth-extension/manifest.json"
host_source="$repo_root/crates/app/src/browser_auth_host.rs"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

if ! grep -Fq 'name = "norma-os"' "$manifest"; then
  fail "crates/app/Cargo.toml must declare a norma-os main bin target"
fi

if ! grep -Fq 'name = "multisoul-agent-hook"' "$manifest"; then
  fail "crates/app/Cargo.toml must declare a multisoul-agent-hook bin target"
fi

if ! grep -Fq 'default-run = "norma-os"' "$manifest"; then
  fail "crates/app/Cargo.toml must set default-run to norma-os so cargo run -p app creates target/debug/norma-os"
fi

if grep -Fq 'default-run = "multisoul"' "$manifest"; then
  fail "crates/app/Cargo.toml must not keep the old multisoul default-run"
fi

if grep -Fq 'name = "multisoul"' "$manifest"; then
  fail "crates/app/Cargo.toml must not keep the old multisoul main bin target"
fi

if ! grep -Fq 'path = "src/bin/multisoul_agent_hook.rs"' "$manifest"; then
  fail "app hook bin target must point at src/bin/multisoul_agent_hook.rs"
fi

if [ ! -f "$wrapper" ]; then
  fail "app hook wrapper source is missing"
fi

if ! grep -Fq 'norma::agent_hook::cli::main()' "$wrapper"; then
  fail "app hook wrapper must delegate to norma::agent_hook::cli::main()"
fi

if ! grep -Fq 'name = "norma-browser-auth-host"' "$manifest"; then
  fail "crates/app/Cargo.toml must declare the Chrome auth Native Messaging helper"
fi

if [ ! -f "$auth_wrapper" ] || ! grep -Fq 'run_browser_auth_native_host()' "$auth_wrapper"; then
  fail "Chrome auth helper wrapper must delegate to app::run_browser_auth_native_host()"
fi

if ! grep -Fq 'path = "target/release/norma-browser-auth-host"' "$packager"; then
  fail "Packager.toml must include norma-browser-auth-host as a secondary executable"
fi

if ! grep -Fq '"crates/app/assets/browser-auth-extension"' "$packager"; then
  fail "Packager.toml must include the Chrome auth extension in app Resources"
fi

if [ ! -f "$extension_manifest" ]; then
  fail "bundled Chrome auth extension manifest is missing"
fi

python3 - "$extension_manifest" "$host_source" <<'PY' || fail "Chrome extension key and Native Messaging allowed origin must stay aligned"
import base64
import hashlib
import json
import pathlib
import sys

manifest = json.loads(pathlib.Path(sys.argv[1]).read_text())
public_key = base64.b64decode(manifest["key"], validate=True)
digest = hashlib.sha256(public_key).digest()[:16]
extension_id = "".join(chr(ord("a") + nibble) for byte in digest for nibble in (byte >> 4, byte & 0x0F))
allowed_origin = f"chrome-extension://{extension_id}/"
host_source = pathlib.Path(sys.argv[2]).read_text()
if allowed_origin not in host_source:
    raise SystemExit(f"derived origin {allowed_origin} is absent from Native Messaging host")
PY

echo "app release helper binary config test passed"
