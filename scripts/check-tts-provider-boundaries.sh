#!/usr/bin/env bash
set -euo pipefail

crate_root="crates/tts/src"
provider_root="$crate_root/providers"

violations="$({
  rg -n '(^|[^[:alnum:]_])(tungstenite|ureq|std::net)([^[:alnum:]_]|$)' "$crate_root" \
    --glob '*.rs' --glob '!crates/tts/src/providers/**' || true
} | sed '/^[[:space:]]*$/d')"

if [[ -n "$violations" ]]; then
  echo "TTS network code must remain under crates/tts/src/providers/:" >&2
  echo "$violations" >&2
  exit 1
fi

if ! rg -q 'tungstenite|ureq' "$provider_root" --glob '*.rs'; then
  echo "Expected built-in remote Provider transport under crates/tts/src/providers/." >&2
  exit 1
fi

echo "TTS Provider network boundary check passed."
