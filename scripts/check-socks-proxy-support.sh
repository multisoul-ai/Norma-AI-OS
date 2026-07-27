#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

sherpa_features="$(cargo tree -p sherpa-onnx-sys -e features 2>/dev/null)"
app_features="$(cargo tree -p app -e features -i ureq@3.3.0 2>/dev/null)"

if ! rg -q 'ureq feature "proxy-from-env"' <<<"$sherpa_features"; then
    echo "sherpa-onnx-sys must read proxy settings from the environment" >&2
    exit 1
fi

if ! rg -q 'ureq feature "socks-proxy"' <<<"$sherpa_features"; then
    echo "sherpa-onnx-sys must support SOCKS proxies used by developer environments" >&2
    exit 1
fi

if ! rg -q 'ureq feature "socks-proxy"' <<<"$app_features"; then
    echo "App runtime HTTP clients must support SOCKS proxies used by developer environments" >&2
    exit 1
fi

echo "Build-time and runtime SOCKS proxy support check passed."
