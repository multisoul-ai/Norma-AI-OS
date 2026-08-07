#!/usr/bin/env bash
set -euo pipefail

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
    echo "Usage: $0 [--help]"
    echo "Downloads the official sherpa-onnx 16kHz Silero VAD model."
    exit 0
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="$ROOT/assets/models/silero_vad"
MODEL="$DEST/silero_vad.onnx"
URL="https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/silero_vad.onnx"

mkdir -p "$DEST"

if [[ ! -s "$MODEL" ]]; then
    curl --fail --location --continue-at - --silent --show-error "$URL" -o "$MODEL"
fi

echo "Downloaded Silero VAD model to: $MODEL"
