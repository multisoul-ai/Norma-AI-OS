#!/usr/bin/env bash
# Shared helpers for Keygate MVP spike scripts.
set -euo pipefail

SPIKE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$SPIKE_ROOT"

DATA_DIR="$SPIKE_ROOT/data"
COOKIE_JAR="$DATA_DIR/session.cookies"
STATE_FILE="$DATA_DIR/state.env"
TOKEN_FILE="$DATA_DIR/offline.token"
PUBKEY_FILE="$DATA_DIR/pubkey.hex"
VENV_DIR="$SPIKE_ROOT/.venv"
PYTHON="${VENV_DIR}/bin/python3"

mkdir -p "$DATA_DIR"

load_dotenv() {
  if [[ ! -f "$SPIKE_ROOT/.env" ]]; then
    echo "missing .env — run: cp .env.example .env && edit secrets" >&2
    exit 1
  fi
  set -a
  # shellcheck disable=SC1091
  source "$SPIKE_ROOT/.env"
  set +a
  KEYGATE_BASE_URL="${KEYGATE_BASE_URL:-http://localhost:9000}"
  API="${KEYGATE_BASE_URL%/}/api/v1"
  ADMIN_EMAIL="${ADMIN_EMAILS%%,*}"
  ADMIN_EMAIL="$(echo "$ADMIN_EMAIL" | tr -d '[:space:]')"
  if [[ -z "$ADMIN_EMAIL" ]]; then
    echo "ADMIN_EMAILS must be set in .env" >&2
    exit 1
  fi
}

ensure_venv() {
  if [[ ! -x "$PYTHON" ]]; then
    python3 -m venv "$VENV_DIR"
    "$PYTHON" -m pip install -q -r "$SPIKE_ROOT/requirements.txt"
  fi
}

json_get() {
  # json_get <json> <jq-filter>
  # Use -r only: jq -e treats JSON false/null as exit 1, which breaks
  # set -e callers reading booleans like setup.status.needed.
  echo "$1" | jq -r "$2"
}

wait_healthy() {
  local tries="${1:-60}"
  local i
  for ((i = 1; i <= tries; i++)); do
    if curl -sf "$KEYGATE_BASE_URL/health" >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done
  echo "Keygate health check failed at $KEYGATE_BASE_URL/health" >&2
  return 1
}

save_state() {
  # append KEY=VALUE into state.env (replace if exists)
  local key="$1"
  local value="$2"
  touch "$STATE_FILE"
  if grep -q "^${key}=" "$STATE_FILE" 2>/dev/null; then
    grep -v "^${key}=" "$STATE_FILE" >"${STATE_FILE}.tmp" || true
    mv "${STATE_FILE}.tmp" "$STATE_FILE"
  fi
  printf '%s=%s\n' "$key" "$value" >>"$STATE_FILE"
}

load_state() {
  if [[ -f "$STATE_FILE" ]]; then
    set -a
    # shellcheck disable=SC1090
    source "$STATE_FILE"
    set +a
  fi
}
