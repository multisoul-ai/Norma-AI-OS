#!/bin/bash
# Builds and opens an isolated real macOS app for Computer Use mobile pairing acceptance.
# Uses an isolated Launchpad preview solely to skip reconciling users' global CLI installs.
set -euo pipefail
echo "Disabled: this legacy runner depends on retired crates/e2e; use crate tests and isolated real-App acceptance." >&2
exit 2
root=$(cd "$(dirname "$0")/.." && pwd)
validation_preview=${NORMA_MOBILE_L3_PREVIEW:-launchpad}
case "$validation_preview" in
  launchpad|github-clone|voice-catalog) ;;
  *) echo "Unsupported isolated validation preview" >&2; exit 2 ;;
esac
cargo build --manifest-path "$root/Cargo.toml" -p app --features e2e-test-support --bin norma-os --bin multisoul-agent-hook --bin norma-browser-auth-host
cargo build --manifest-path "$root/Cargo.toml" -p norma --bin norma
validation_root=$(mktemp -d /tmp/norma-mobile-desktop-l3.XXXXXX)
validation_name="Norma Mobile Validation"
if [[ -n ${NORMA_REALTIME_L3_UPSTREAM:-} ]]; then validation_name="Norma Realtime Validation"; fi
validation_bundle="$validation_root/$validation_name.app"
mkdir -p "$validation_bundle/Contents/MacOS" "$validation_root/data"
cp "$root/target/debug/norma-os" "$validation_bundle/Contents/MacOS/norma-os"
cp "$root/target/debug/multisoul-agent-hook" "$validation_bundle/Contents/MacOS/multisoul-agent-hook"
cp "$root/target/debug/norma" "$validation_bundle/Contents/MacOS/norma"
cp "$root/target/debug/norma-browser-auth-host" "$validation_bundle/Contents/MacOS/norma-browser-auth-host"
python3 - "$validation_bundle" <<'PY'
import os, plistlib, pathlib, re, sys
bundle=pathlib.Path(sys.argv[1])
bundle_id=os.environ.get('NORMA_MOBILE_L3_BUNDLE_ID', 'com.norma.mobile-validation')
assert re.fullmatch(r'[A-Za-z0-9.-]+', bundle_id), 'invalid isolated bundle ID'
with (bundle/'Contents/Info.plist').open('wb') as output:
    plistlib.dump({'CFBundleIdentifier':bundle_id,
        'CFBundleName':bundle.stem,'CFBundleDisplayName':bundle.stem,'CFBundleExecutable':'norma-os',
        'CFBundlePackageType':'APPL','CFBundleVersion':'1',
        'NSHighResolutionCapable':True,
        'NSLocalNetworkUsageDescription':'验证隔离的 Norma 手机扫码配对。'}, output)
PY
codesign --force --sign - "$validation_bundle"
NORMA_MOBILE_APP_BUNDLE="$validation_bundle" cargo test --manifest-path "$root/Cargo.toml" \
  -p e2e --test mobile_desktop_bundle -- --ignored
validation_extra=(--env "NORMA_VISUAL_PREVIEW=$validation_preview")
if [[ -n ${NORMA_THREAD_REAL_CODEX_HOME:-} ]]; then
  test -d "$NORMA_THREAD_REAL_CODEX_HOME"
  test -n "${NORMA_CLOUD_LOCAL_PORT:?real thread validation needs a local relay port}"
  validation_extra+=(--env "CODEX_HOME=$NORMA_THREAD_REAL_CODEX_HOME"
    --env "ZDOTDIR=$NORMA_THREAD_REAL_CODEX_HOME"
    --env "NORMA_THREAD_CODEX_REAL_L3=1"
    --env "NORMA_CLOUD_LOCAL_PORT=$NORMA_CLOUD_LOCAL_PORT")
fi
if [[ -n ${NORMA_REALTIME_L3_UPSTREAM:-} ]]; then
  test -d "${NORMA_NATIVE_FIXTURE_ROOT:?realtime validation requires an isolated provider}"
  validation_extra+=(--env "NORMA_REALTIME_L3_UPSTREAM=$NORMA_REALTIME_L3_UPSTREAM"
    --env "ZDOTDIR=$NORMA_NATIVE_FIXTURE_ROOT"
    --env "NORMA_NATIVE_FIXTURE_ROOT=$NORMA_NATIVE_FIXTURE_ROOT")
fi
open -n "$validation_bundle" --env "NORMA_TEST_DATA_ROOT=$validation_root/data" \
  --env "NORMA_MOBILE_L3_LOOPBACK=1" \
  --env "NORMA_SOCKET=$validation_root/norma.sock" \
  "${validation_extra[@]}" \
  -o "$validation_root/stdout.log" --stderr "$validation_root/stderr.log"
printf '%s\n' "$validation_root"
