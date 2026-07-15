#!/usr/bin/env bash
# Start local Keygate + Postgres for the spike.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "$SCRIPT_DIR/lib.sh"
load_dotenv

if [[ -z "${JWT_SECRET:-}" || -z "${LICENSE_SIGNING_KEY:-}" ]]; then
  echo "JWT_SECRET and LICENSE_SIGNING_KEY must be non-empty in .env" >&2
  echo "Generate with: openssl rand -hex 32" >&2
  exit 1
fi

# Build from public git source (GHCR image may require auth).
docker compose -f "$SPIKE_ROOT/docker-compose.yml" --env-file "$SPIKE_ROOT/.env" up -d --build
echo "Waiting for Keygate health..."
wait_healthy 120
echo "OK Keygate is up at $KEYGATE_BASE_URL"
echo "Admin UI: $KEYGATE_BASE_URL"
