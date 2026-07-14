#!/usr/bin/env bash
# First-time online activation: POST /license/activate → save offline token.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "$SCRIPT_DIR/lib.sh"
load_dotenv
load_state
wait_healthy 30

: "${LICENSE_KEY:?LICENSE_KEY missing — run ./scripts/bootstrap.sh first}"
: "${IDENTIFIER:?IDENTIFIER missing — run ./scripts/bootstrap.sh first}"

echo "Activating LICENSE_KEY against identifier=$IDENTIFIER ..."
# Include license fingerprint so re-bootstrap with a new key is not blocked by
# a prior Idempotency-Key cache for the same device identifier.
key_fp="$(printf '%s' "$LICENSE_KEY" | shasum -a 256 | cut -c1-16)"
idem="spike-activate-${IDENTIFIER}-${key_fp}"
http_body="$(mktemp)"
http_code="$(curl -sS -o "$http_body" -w '%{http_code}' -X POST "$API/license/activate" \
  -H 'Content-Type: application/json' \
  -H "Idempotency-Key: ${idem}" \
  -d "$(jq -n \
    --arg key "$LICENSE_KEY" \
    --arg id "$IDENTIFIER" \
    '{license_key:$key,identifier:$id,identifier_type:"device",label:"spike-desktop"}')")"
resp="$(cat "$http_body")"
rm -f "$http_body"
if [[ "$http_code" != "200" ]] || ! echo "$resp" | jq -e '.success == true' >/dev/null 2>&1; then
  echo "activate failed HTTP=$http_code body=$resp" >&2
  exit 1
fi

status="$(json_get "$resp" '.data.status')"
token="$(json_get "$resp" '.data.token')"
license_id="$(json_get "$resp" '.data.license_id')"
if [[ -z "$token" || "$token" == "null" ]]; then
  echo "activate response missing token: $resp" >&2
  exit 1
fi

printf '%s\n' "$token" >"$TOKEN_FILE"
save_state LAST_ACTIVATE_STATUS "$status"
save_state LICENSE_ID "$license_id"

# Refresh pubkey (in case signing key rotated)
pubkey_json="$(curl -sf "$API/license/pubkey")"
PUBKEY="$(json_get "$pubkey_json" '.data.public_key')"
printf '%s\n' "$PUBKEY" >"$PUBKEY_FILE"
save_state PUBKEY_HEX "$PUBKEY"

echo "OK activate status=$status license_id=$license_id"
echo "  token -> $TOKEN_FILE"
echo "  pubkey -> $PUBKEY_FILE"
