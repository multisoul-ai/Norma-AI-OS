#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODEL_ROOT="$ROOT/assets/models/tts"
MODEL_DIR="$MODEL_ROOT/matcha-icefall-zh-baker"
MODEL_ARCHIVE="$MODEL_ROOT/matcha-icefall-zh-baker.tar.bz2"
VOCODER="$MODEL_ROOT/vocos-22khz-univ.onnx"
MODEL_URL="https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/matcha-icefall-zh-baker.tar.bz2"
VOCODER_URL="https://github.com/k2-fsa/sherpa-onnx/releases/download/vocoder-models/vocos-22khz-univ.onnx"

mkdir -p "$MODEL_ROOT"

if [[ ! -f "$MODEL_DIR/model-steps-3.onnx" ]]; then
  curl --fail --location --continue-at - --silent --show-error "$MODEL_URL" -o "$MODEL_ARCHIVE"
  tar -xjf "$MODEL_ARCHIVE" -C "$MODEL_ROOT"
  rm "$MODEL_ARCHIVE"
fi

if [[ ! -f "$VOCODER" ]]; then
  curl --fail --location --continue-at - --silent --show-error "$VOCODER_URL" -o "$VOCODER"
fi

echo "Downloaded Matcha TTS model to: $MODEL_DIR"
echo "Downloaded Matcha vocoder to: $VOCODER"
