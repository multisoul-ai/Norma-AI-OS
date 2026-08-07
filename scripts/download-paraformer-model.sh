#!/usr/bin/env bash
# scripts/download-paraformer-model.sh
# Downloads sherpa-onnx-streaming-paraformer-bilingual-zh-en (int8) to assets/models/paraformer/
set -euo pipefail

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
    echo "Usage: $0 [--help]"
    echo "Downloads sherpa-onnx-streaming-paraformer-bilingual-zh-en (int8) to assets/models/paraformer/"
    exit 0
fi

DEST="$(cd "$(dirname "$0")/.." && pwd)/assets/models/paraformer"
MODEL_NAME="sherpa-onnx-streaming-paraformer-bilingual-zh-en"
BASE_URL="https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models"

mkdir -p "$DEST"
cd "$DEST"

echo "Downloading $MODEL_NAME..."
if ! curl -LO "$BASE_URL/${MODEL_NAME}.tar.bz2"; then
    echo "ERROR: Failed to download model. Check your internet connection and that GitHub is reachable." >&2
    exit 1
fi

tar -xjf "${MODEL_NAME}.tar.bz2" --strip-components=1
rm "${MODEL_NAME}.tar.bz2"

echo "Model files:"
ls -lh "$DEST"
