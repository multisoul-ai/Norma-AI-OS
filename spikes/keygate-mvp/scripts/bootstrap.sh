#!/usr/bin/env bash
# Bootstrap: setup wizard API + OTP login + perpetual plan + manual license.
# Requires Keygate already running (./scripts/up.sh).
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "$SCRIPT_DIR/lib.sh"
load_dotenv
wait_healthy 30

ADMIN_NAME="${ADMIN_NAME:-Spike Admin}"
SITE_NAME="${SITE_NAME:-Norma Spike}"
PRODUCT_NAME="${PRODUCT_NAME:-Norma OS Desktop}"
PRODUCT_SLUG="${PRODUCT_SLUG:-norma-os-desktop}"
PRODUCT_TYPE="${PRODUCT_TYPE:-desktop}"
CUSTOMER_EMAIL="${CUSTOMER_EMAIL:-customer@example.com}"
IDENTIFIER="${IDENTIFIER:-spike-desktop-1}"

status_json="$(curl -sf "$API/setup/status")"
needed="$(json_get "$status_json" '.data.needed')"
PRODUCT_ID=""
DEFAULT_PLAN_ID=""

setup_via_api() {
  curl -sS -X POST "$API/setup/initialize" \
    -H 'Content-Type: application/json' \
    -d "$(jq -n \
      --arg email "$ADMIN_EMAIL" \
      --arg name "$ADMIN_NAME" \
      --arg site "$SITE_NAME" \
      --arg pname "$PRODUCT_NAME" \
      --arg slug "$PRODUCT_SLUG" \
      --arg ptype "$PRODUCT_TYPE" \
      '{admin_email:$email,admin_name:$name,site_name:$site,product_name:$pname,product_slug:$slug,product_type:$ptype}')"
}

# Keygate v0.1.1 setup/initialize can 500 because INSERT INTO plans omits
# checkout_id (column is NOT NULL). Fall back to SQL seed for the spike.
setup_via_sql() {
  echo "WARN: setup/initialize failed — applying SQL seed workaround (Keygate checkout_id bug)" >&2
  local user_id product_id plan_id checkout_id sql_file
  user_id="$(uuidgen | tr '[:upper:]' '[:lower:]')"
  product_id="$(uuidgen | tr '[:upper:]' '[:lower:]')"
  plan_id="$(uuidgen | tr '[:upper:]' '[:lower:]')"
  checkout_id="$(openssl rand -hex 4)"
  sql_file="$(mktemp)"

  # Generate SQL in Python to avoid bash heredoc quote parsing pitfalls.
  USER_ID="$user_id" PRODUCT_ID_SQL="$product_id" PLAN_ID_SQL="$plan_id" \
  CHECKOUT_ID="$checkout_id" ADMIN_EMAIL="$ADMIN_EMAIL" ADMIN_NAME="$ADMIN_NAME" \
  SITE_NAME="$SITE_NAME" PRODUCT_NAME="$PRODUCT_NAME" PRODUCT_SLUG="$PRODUCT_SLUG" \
  PRODUCT_TYPE="$PRODUCT_TYPE" python3 - <<'PY' >"$sql_file"
import os

def q(value: str) -> str:
    return "'" + value.replace("'", "''") + "'"

print(f"""INSERT INTO users (id, email, name, role, created_at, updated_at)
VALUES ({q(os.environ['USER_ID'])}, {q(os.environ['ADMIN_EMAIL'])}, {q(os.environ['ADMIN_NAME'])}, 'owner', now(), now())
ON CONFLICT (email) DO UPDATE SET role = 'owner', name = EXCLUDED.name, updated_at = now();

INSERT INTO settings (key, value) VALUES ('site_name', {q(os.environ['SITE_NAME'])})
ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value;

INSERT INTO products (id, name, slug, type, created_at)
VALUES ({q(os.environ['PRODUCT_ID_SQL'])}, {q(os.environ['PRODUCT_NAME'])}, {q(os.environ['PRODUCT_SLUG'])}, {q(os.environ['PRODUCT_TYPE'])}, now())
ON CONFLICT (slug) DO NOTHING;

INSERT INTO plans (
  id, product_id, name, slug, license_type, max_activations, max_seats,
  grace_days, active, created_at, checkout_id, license_model, floating_timeout
) VALUES (
  {q(os.environ['PLAN_ID_SQL'])}, {q(os.environ['PRODUCT_ID_SQL'])}, 'Pro', 'pro', 'subscription', 3, 0,
  7, true, now(), {q(os.environ['CHECKOUT_ID'])}, 'standard', 30
) ON CONFLICT (product_id, slug) DO NOTHING;

INSERT INTO settings (key, value) VALUES ('setup_complete', 'true')
ON CONFLICT (key) DO UPDATE SET value = 'true';
""")
PY

  docker compose -f "$SPIKE_ROOT/docker-compose.yml" --env-file "$SPIKE_ROOT/.env" exec -T postgres \
    psql -U keygate -d keygate -v ON_ERROR_STOP=1 <"$sql_file"
  rm -f "$sql_file"
  PRODUCT_ID="$product_id"
  DEFAULT_PLAN_ID="$plan_id"
  echo "SQL seed complete. product=$PRODUCT_ID default_plan=$DEFAULT_PLAN_ID"
}

if [[ "$needed" == "true" ]]; then
  echo "Running setup/initialize..."
  init_json="$(setup_via_api || true)"
  if echo "$init_json" | jq -e '.success == true' >/dev/null 2>&1; then
    PRODUCT_ID="$(json_get "$init_json" '.data.product.id')"
    DEFAULT_PLAN_ID="$(json_get "$init_json" '.data.plan.id')"
    echo "Setup complete via API. product=$PRODUCT_ID default_plan=$DEFAULT_PLAN_ID"
  else
    echo "setup/initialize response: ${init_json:-<empty>}" >&2
    setup_via_sql
  fi
else
  echo "Setup already complete; will resolve product after login..."
fi
# Development-only login (ENVIRONMENT=development + localhost BASE_URL).
echo "Dev-login as $ADMIN_EMAIL..."
rm -f "$COOKIE_JAR"
verify_json="$(curl -sf -c "$COOKIE_JAR" -b "$COOKIE_JAR" -X POST "$API/auth/dev-login" \
  -H 'Content-Type: application/json' \
  -d "$(jq -n --arg email "$ADMIN_EMAIL" --arg name "$ADMIN_NAME" '{email:$email,name:$name}')")"
logged_email="$(echo "$verify_json" | jq -r --arg fallback "$ADMIN_EMAIL" '.data.user.email // .data.email // $fallback')"
echo "Logged in as $logged_email"

if [[ -z "$PRODUCT_ID" ]]; then
  products_json="$(curl -sf -b "$COOKIE_JAR" "$API/admin/products")"
  PRODUCT_ID="$(json_get "$products_json" '.data.products[0].id')"
  DEFAULT_PLAN_ID="$(curl -sf -b "$COOKIE_JAR" "$API/admin/plans?product_id=$PRODUCT_ID" | jq -er '.data.plans[0].id')"
fi

# Create a perpetual plan with high activation cap (MVP: no practical device lock).
PLAN_SLUG="buyout-spike"
plans_json="$(curl -sf -b "$COOKIE_JAR" "$API/admin/plans?product_id=$PRODUCT_ID")"
PLAN_ID="$(echo "$plans_json" | jq -r --arg slug "$PLAN_SLUG" '.data.plans[]? | select(.slug==$slug) | .id' | head -n1)"
if [[ -z "$PLAN_ID" || "$PLAN_ID" == "null" ]]; then
  echo "Creating perpetual plan $PLAN_SLUG..."
  plan_json="$(curl -sf -b "$COOKIE_JAR" -X POST "$API/admin/plans" \
    -H 'Content-Type: application/json' \
    -d "$(jq -n \
      --arg pid "$PRODUCT_ID" \
      --arg slug "$PLAN_SLUG" \
      '{product_id:$pid,name:"Buyout Spike",slug:$slug,license_type:"perpetual",max_activations:10000,grace_days:7}')")"
  PLAN_ID="$(json_get "$plan_json" '.data.id')"
else
  echo "Reusing perpetual plan $PLAN_ID"
fi

# Reuse an existing spike license for this customer+plan when present so
# check.sh can re-run without minting a new key every time.
existing_lic="$(curl -sf -b "$COOKIE_JAR" "$API/admin/licenses?limit=50" | jq -c \
  --arg plan "$PLAN_ID" --arg email "$CUSTOMER_EMAIL" \
  '.data.licenses // [] | map(select(.plan_id==$plan and .email==$email and .status=="active")) | .[0] // empty')"
if [[ -n "$existing_lic" ]]; then
  LICENSE_ID="$(echo "$existing_lic" | jq -r '.id')"
  LICENSE_KEY="$(echo "$existing_lic" | jq -r '.license_key')"
  echo "Reusing license $LICENSE_ID for $CUSTOMER_EMAIL"
else
  echo "Creating license for $CUSTOMER_EMAIL..."
  lic_json="$(curl -sf -b "$COOKIE_JAR" -X POST "$API/admin/licenses" \
    -H 'Content-Type: application/json' \
    -d "$(jq -n \
      --arg pid "$PRODUCT_ID" \
      --arg plan "$PLAN_ID" \
      --arg email "$CUSTOMER_EMAIL" \
      '{product_id:$pid,plan_id:$plan,email:$email,notes:"keygate-mvp spike"}')")"
  LICENSE_ID="$(json_get "$lic_json" '.data.id')"
  LICENSE_KEY="$(json_get "$lic_json" '.data.license_key')"
fi
if [[ -z "$LICENSE_KEY" || "$LICENSE_KEY" == "null" ]]; then
  echo "LICENSE_KEY missing after bootstrap license step" >&2
  exit 1
fi

save_state PRODUCT_ID "$PRODUCT_ID"
save_state PLAN_ID "$PLAN_ID"
save_state DEFAULT_PLAN_ID "${DEFAULT_PLAN_ID:-}"
save_state LICENSE_ID "$LICENSE_ID"
save_state LICENSE_KEY "$LICENSE_KEY"
save_state CUSTOMER_EMAIL "$CUSTOMER_EMAIL"
save_state IDENTIFIER "$IDENTIFIER"
save_state ADMIN_EMAIL "$ADMIN_EMAIL"

# Fetch pubkey for offline verify
pubkey_json="$(curl -sf "$API/license/pubkey")"
PUBKEY="$(json_get "$pubkey_json" '.data.public_key')"
printf '%s\n' "$PUBKEY" >"$PUBKEY_FILE"
save_state PUBKEY_HEX "$PUBKEY"

# Stats peek (user/license visibility)
stats_json="$(curl -sf -b "$COOKIE_JAR" "$API/admin/stats" || echo '{}')"
echo "Admin stats snapshot: $(echo "$stats_json" | jq -c '.data // .')"

echo
echo "OK bootstrap"
echo "  LICENSE_KEY=$LICENSE_KEY"
echo "  IDENTIFIER=$IDENTIFIER"
echo "  state -> $STATE_FILE"
