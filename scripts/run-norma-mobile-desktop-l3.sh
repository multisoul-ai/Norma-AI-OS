#!/bin/bash
# Builds and opens an isolated real macOS app for Computer Use mobile pairing acceptance.
# Uses the existing empty Launchpad preview solely to skip reconciling users' global CLI installs.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
validation_preview=${NORMA_MOBILE_L3_PREVIEW:-github-clone}
case "$validation_preview" in
  github-clone|voice-catalog) ;;
  *) echo "Unsupported isolated validation preview" >&2; exit 2 ;;
esac
cargo build --manifest-path "$root/Cargo.toml" -p app --features e2e-test-support --bin norma-os --bin multisoul-agent-hook
validation_root=$(mktemp -d /tmp/norma-mobile-desktop-l3.XXXXXX)
validation_bundle="$validation_root/Norma Mobile Validation.app"
mkdir -p "$validation_bundle/Contents/MacOS" "$validation_root/data"
cp "$root/target/debug/norma-os" "$validation_bundle/Contents/MacOS/norma-os"
cp "$root/target/debug/multisoul-agent-hook" "$validation_bundle/Contents/MacOS/multisoul-agent-hook"
python3 - "$validation_bundle" <<'PY'
import os, plistlib, pathlib, re, sys
bundle=pathlib.Path(sys.argv[1])
bundle_id=os.environ.get('NORMA_MOBILE_L3_BUNDLE_ID', 'com.norma.mobile-validation')
assert re.fullmatch(r'[A-Za-z0-9.-]+', bundle_id), 'invalid isolated bundle ID'
with (bundle/'Contents/Info.plist').open('wb') as output:
    plistlib.dump({'CFBundleIdentifier':bundle_id,
        'CFBundleName':'Norma Mobile Validation','CFBundleExecutable':'norma-os',
        'CFBundlePackageType':'APPL','CFBundleVersion':'1',
        'NSHighResolutionCapable':True,
        'NSLocalNetworkUsageDescription':'验证隔离的 Norma 手机扫码配对。'}, output)
PY
codesign --force --sign - "$validation_bundle"
NORMA_MOBILE_APP_BUNDLE="$validation_bundle" cargo test --manifest-path "$root/Cargo.toml" \
  -p e2e --test mobile_desktop_bundle -- --ignored
open -n "$validation_bundle" --env "NORMA_TEST_DATA_ROOT=$validation_root/data" \
  --env "NORMA_MOBILE_L3_LOOPBACK=1" \
  --env "NORMA_SOCKET=$validation_root/norma.sock" --env "NORMA_VISUAL_PREVIEW=$validation_preview" \
  -o "$validation_root/stdout.log" --stderr "$validation_root/stderr.log"
printf '%s\n' "$validation_root"
