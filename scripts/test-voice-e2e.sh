#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

run_real_models=false
if [[ "${1:-}" == "--real-models" ]]; then
  run_real_models=true
elif [[ $# -gt 0 ]]; then
  echo "usage: $0 [--real-models]" >&2
  exit 2
fi

echo "[voice-e2e] headless GPUI and cross-crate scenarios"
cargo test -p e2e

echo "[voice-e2e] app shortcut, mode, dialogue, provider and playback scenarios"
cargo test -p app voice_
cargo test -p app dialogue_

echo "[voice-e2e] ASR catalog, installation integrity and deletion rules"
cargo test -p asr

echo "[voice-e2e] Sherpa capture, stream, recognizer factory and pipeline scenarios"
cargo test -p voice_sherpa

if [[ "$run_real_models" == true ]]; then
  echo "[voice-e2e] managed Paraformer/Zipformer UI selection and persistence"
  cargo test -p e2e \
    e2e_10_real_paraformer_and_zipformer_selection_is_shared_and_persistent \
    -- --ignored

  echo "[voice-e2e] real Paraformer WAV to final transcript"
  cargo test -p voice_sherpa \
    test_real_wav_reaches_final_transcript_and_thinking \
    -- --ignored

  echo "[voice-e2e] real Zipformer WAV to partial and final transcripts"
  cargo test -p voice_sherpa \
    test_zipformer_real_wav_produces_partial_and_final_transcripts \
    -- --ignored
fi

echo "[voice-e2e] all requested scenarios passed"
