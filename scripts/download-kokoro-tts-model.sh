#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODEL_ROOT="$ROOT/assets/models/tts"
MODEL_DIR="$MODEL_ROOT/kokoro-int8-multi-lang-v1_1"
MODEL_ARCHIVE="$MODEL_ROOT/kokoro-int8-multi-lang-v1_1.tar.bz2"
MODEL_URL="https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/kokoro-int8-multi-lang-v1_1.tar.bz2"

mkdir -p "$MODEL_ROOT"

if [[ ! -f "$MODEL_DIR/model.int8.onnx" ]]; then
  curl --fail --location --continue-at - --silent --show-error "$MODEL_URL" -o "$MODEL_ARCHIVE"
  tar -xjf "$MODEL_ARCHIVE" -C "$MODEL_ROOT"
  rm "$MODEL_ARCHIVE"
fi

echo "Downloaded Kokoro Chinese-English TTS model to: $MODEL_DIR"
