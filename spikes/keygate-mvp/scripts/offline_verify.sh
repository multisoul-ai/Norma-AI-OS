#!/usr/bin/env bash
# Offline verify using saved token + pubkey (no Keygate network call).
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "$SCRIPT_DIR/lib.sh"
load_dotenv
load_state
ensure_venv

if [[ ! -f "$TOKEN_FILE" ]]; then
  echo "missing $TOKEN_FILE — run ./scripts/activate.sh first" >&2
  exit 1
fi

PUBKEY_HEX="${PUBKEY_HEX:-}"
if [[ -z "$PUBKEY_HEX" && -f "$PUBKEY_FILE" ]]; then
  PUBKEY_HEX="$(tr -d '[:space:]' <"$PUBKEY_FILE")"
fi
: "${PUBKEY_HEX:?PUBKEY_HEX missing — run activate.sh or bootstrap.sh first}"

# Prove we are not calling Keygate for this step (optional offline mode).
if [[ "${FORCE_OFFLINE:-1}" == "1" ]]; then
  export http_proxy=http://127.0.0.1:9
  export https_proxy=http://127.0.0.1:9
  export HTTP_PROXY=http://127.0.0.1:9
  export HTTPS_PROXY=http://127.0.0.1:9
  export ALL_PROXY=http://127.0.0.1:9
  export NO_PROXY=
fi

"$PYTHON" "$SCRIPT_DIR/offline_verify.py" \
  --token-file "$TOKEN_FILE" \
  --pubkey-hex "$PUBKEY_HEX" \
  --json
