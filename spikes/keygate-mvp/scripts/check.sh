#!/usr/bin/env bash
# End-to-end go/no-go harness for Keygate MVP spike.
# Exit 0 = GO, non-zero = NO-GO.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "$SCRIPT_DIR/lib.sh"

echo "==> [1/5] unit: offline token verify"
ensure_venv
"$PYTHON" -m pip install -q -r "$SPIKE_ROOT/requirements.txt"
"$PYTHON" -m unittest discover -s "$SPIKE_ROOT/tests" -v

echo "==> [2/5] up: docker Keygate"
"$SCRIPT_DIR/up.sh"

echo "==> [3/5] bootstrap: setup + perpetual license"
"$SCRIPT_DIR/bootstrap.sh"

echo "==> [4/5] activate: first-time online"
"$SCRIPT_DIR/activate.sh"

echo "==> [5/5] offline verify (proxies poisoned to block network)"
FORCE_OFFLINE=1 "$SCRIPT_DIR/offline_verify.sh" >/dev/null

# Hard check: admin can list licenses (dashboard / stats path)
load_dotenv
load_state
# Refresh session cookie — bootstrap already logged in, but check.sh may have a stale jar.
rm -f "$COOKIE_JAR"
curl -sf -c "$COOKIE_JAR" -b "$COOKIE_JAR" -X POST "$API/auth/dev-login" \
  -H 'Content-Type: application/json' \
  -d "$(jq -n --arg email "$ADMIN_EMAIL" --arg name "Spike Admin" '{email:$email,name:$name}')" >/dev/null
list_json="$(curl -sf -b "$COOKIE_JAR" "$API/admin/licenses?limit=5")"
if ! echo "$list_json" | jq -e '.data.licenses | length >= 1' >/dev/null 2>&1; then
  echo "NO-GO: admin licenses list empty or unreachable: $list_json" >&2
  exit 1
fi
echo "OK admin licenses list visible ($(echo "$list_json" | jq '.data.licenses | length') rows)"

stats_json="$(curl -sf -b "$COOKIE_JAR" "$API/admin/stats")"
if ! echo "$stats_json" | jq -e '.data.total_licenses >= 1' >/dev/null 2>&1; then
  echo "NO-GO: admin stats missing total_licenses: $stats_json" >&2
  exit 1
fi
echo "OK admin stats total_licenses=$(echo "$stats_json" | jq -r '.data.total_licenses')"

echo
echo "=============================="
echo " GO: Keygate spike criteria met"
echo "=============================="
echo "Next: thin App integration (paste key + local token). See ADR 0014."
echo "Artifacts: $DATA_DIR"
