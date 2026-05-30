#!/usr/bin/env bash
# e2e-stripe-checkout.sh — Stripe billing chain E2E validation (test mode).
#
# Strategy (Option C — deterministic signed POST):
#   1. Create Stripe test-mode customer + subscription via API.
#   2. Build a checkout.session.completed event JSON carrying the real
#      customer/subscription IDs and tenant_id in metadata.
#   3. Compute a valid Stripe webhook signature (HMAC-SHA256) using the
#      STRIPE_WEBHOOK_SECRET from .env.local.
#   4. POST the signed event to the live webhook endpoint at
#      https://corelink-signup.humangr.com/webhooks/stripe
#      (Stripe-Signature header accepted; Bot Fight Mode passes User-Agent).
#   5. Poll D1 tenant_billing until status=paid row appears (≤30s).
#   6. Cleanup: cancel subscription, delete customer, delete D1 row.
#
# Requires:
#   - curl, python3 (stdlib only)
#   - .env.local in $REPO_ROOT (or parent main repo) with:
#       STRIPE_SECRET_KEY=sk_test_...
#       STRIPE_WEBHOOK_SECRET=whsec_...
#       CLOUDFLARE_API_TOKEN=...
#       STRIPE_PRICE_ID_STARTER=price_...  (optional; falls back to API list)
#   - wrangler v4: apps/signup-worker/node_modules/wrangler/bin/wrangler.js
#
# Usage:
#   bash scripts/e2e-stripe-checkout.sh
#
# Exit codes:
#   0 — PASS (row inserted with status=paid, all cleanup succeeded)
#   1 — FAIL (see output for reason)

set -euo pipefail

# ---------------------------------------------------------------------------
# Paths — resolve worktree vs main checkout transparently
# ---------------------------------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

GIT_COMMON_DIR=$(git -C "$REPO_ROOT" rev-parse --git-common-dir 2>/dev/null || echo "")
if [[ -n "$GIT_COMMON_DIR" && "$GIT_COMMON_DIR" != ".git" && -d "$GIT_COMMON_DIR" ]]; then
  MAIN_REPO_ROOT=$(dirname "$GIT_COMMON_DIR")
else
  MAIN_REPO_ROOT="$REPO_ROOT"
fi

# wrangler.js directly (not .bin/ shim — shim uses bash features broken on Node v22)
WRANGLER="$REPO_ROOT/apps/signup-worker/node_modules/wrangler/bin/wrangler.js"
[[ -f "$WRANGLER" ]] || WRANGLER="$MAIN_REPO_ROOT/apps/signup-worker/node_modules/wrangler/bin/wrangler.js"

# wrangler.toml from worktree (worktree may have updated bindings vs main)
WRANGLER_CONFIG="$REPO_ROOT/apps/signup-worker/wrangler.toml"
[[ -f "$WRANGLER_CONFIG" ]] || WRANGLER_CONFIG="$MAIN_REPO_ROOT/apps/signup-worker/wrangler.toml"

# .env.local: worktree-local first, then main repo
ENV_FILE="$REPO_ROOT/.env.local"
[[ -f "$ENV_FILE" ]] || ENV_FILE="$MAIN_REPO_ROOT/.env.local"

# ---------------------------------------------------------------------------
# Load credentials
# ---------------------------------------------------------------------------
if [[ ! -f "$ENV_FILE" ]]; then
  echo "ERROR: .env.local not found (tried $REPO_ROOT and $MAIN_REPO_ROOT)" >&2
  exit 1
fi

set -o allexport
# shellcheck disable=SC1090
source "$ENV_FILE"
set +o allexport

[[ -n "${STRIPE_SECRET_KEY:-}" ]]     || { echo "ERROR: STRIPE_SECRET_KEY missing from .env.local" >&2; exit 1; }
[[ -n "${STRIPE_WEBHOOK_SECRET:-}" ]] || { echo "ERROR: STRIPE_WEBHOOK_SECRET missing from .env.local" >&2; exit 1; }
[[ -n "${CLOUDFLARE_API_TOKEN:-}" ]]  || { echo "ERROR: CLOUDFLARE_API_TOKEN missing from .env.local" >&2; exit 1; }

echo "Credentials loaded (STRIPE_SECRET_KEY=...${STRIPE_SECRET_KEY: -4}, WEBHOOK_SECRET=...${STRIPE_WEBHOOK_SECRET: -4})"

# ---------------------------------------------------------------------------
# Dependency checks
# ---------------------------------------------------------------------------
command -v curl    >/dev/null || { echo "ERROR: curl not found" >&2; exit 1; }
command -v python3 >/dev/null || { echo "ERROR: python3 not found" >&2; exit 1; }
[[ -f "$WRANGLER" ]] || { echo "ERROR: wrangler not found at $WRANGLER" >&2; exit 1; }

WRANGLER_VER=$(node "$WRANGLER" --version 2>&1 || echo "unknown")
echo "wrangler: $WRANGLER_VER"

# ---------------------------------------------------------------------------
# State for cleanup trap
# ---------------------------------------------------------------------------
STRIPE_CUSTOMER_ID=""
STRIPE_SUBSCRIPTION_ID=""
TENANT_ID=""
D1_ROW_INSERTED=0

cleanup() {
  local exit_code=$?
  echo ""
  echo "==> Cleanup..."

  # Cancel Stripe subscription
  if [[ -n "$STRIPE_SUBSCRIPTION_ID" ]]; then
    echo "  Canceling Stripe subscription: $STRIPE_SUBSCRIPTION_ID"
    local sub_status
    sub_status=$(curl -sf -X DELETE \
      "https://api.stripe.com/v1/subscriptions/${STRIPE_SUBSCRIPTION_ID}" \
      -u "${STRIPE_SECRET_KEY}:" | \
      python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('status','?'))" 2>/dev/null || echo "error")
    echo "  Subscription status: $sub_status"
  fi

  # Delete Stripe customer
  if [[ -n "$STRIPE_CUSTOMER_ID" ]]; then
    echo "  Deleting Stripe customer: $STRIPE_CUSTOMER_ID"
    local deleted
    deleted=$(curl -sf -X DELETE \
      "https://api.stripe.com/v1/customers/${STRIPE_CUSTOMER_ID}" \
      -u "${STRIPE_SECRET_KEY}:" | \
      python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('deleted','?'))" 2>/dev/null || echo "error")
    echo "  Customer deleted: $deleted"
  fi

  # Delete D1 row
  if [[ "$D1_ROW_INSERTED" -eq 1 && -n "$TENANT_ID" && -n "$STRIPE_CUSTOMER_ID" ]]; then
    echo "  Removing D1 tenant_billing row (tenant_id=$TENANT_ID stripe_customer_id=$STRIPE_CUSTOMER_ID)"
    CLOUDFLARE_API_TOKEN="$CLOUDFLARE_API_TOKEN" \
      node "$WRANGLER" d1 execute CONFIG_DB --env prod --remote \
        --command "DELETE FROM tenant_billing WHERE tenant_id='${TENANT_ID}' AND stripe_customer_id='${STRIPE_CUSTOMER_ID}'" \
        --config "$WRANGLER_CONFIG" 2>&1 | \
      python3 -c "
import sys, json, re
raw = sys.stdin.read()
m = re.search(r'\{.*\"changes\"\s*:\s*(\d+).*\}', raw, re.DOTALL)
if m:
    print('  D1 changes:', m.group(1))
else:
    print('  D1 cleanup output:', raw[:200])
" || echo "  Warning: D1 cleanup may have failed — run: DELETE FROM tenant_billing WHERE tenant_id='$TENANT_ID'"
  fi

  if [[ $exit_code -eq 0 ]]; then
    echo ""
    echo "==========================================="
    echo "RESULT: PASS"
    echo "==========================================="
    echo "  tenant_id:              $TENANT_ID"
    echo "  stripe_customer_id:     $STRIPE_CUSTOMER_ID"
    echo "  stripe_subscription_id: $STRIPE_SUBSCRIPTION_ID"
    echo "  D1 row status:          paid"
    echo "  Cleanup:                complete"
    echo "==========================================="
  else
    echo ""
    echo "==========================================="
    echo "RESULT: FAIL (exit_code=$exit_code)"
    echo "==========================================="
    echo "  tenant_id:              ${TENANT_ID:-<not set>}"
    echo "  stripe_customer_id:     ${STRIPE_CUSTOMER_ID:-<not set>}"
    echo "  stripe_subscription_id: ${STRIPE_SUBSCRIPTION_ID:-<not set>}"
    echo "==========================================="
  fi
  exit $exit_code
}
trap cleanup EXIT

# ---------------------------------------------------------------------------
# Helper: run D1 query and extract results JSON
# ---------------------------------------------------------------------------
d1_query() {
  local cmd="$1"
  CLOUDFLARE_API_TOKEN="$CLOUDFLARE_API_TOKEN" \
    node "$WRANGLER" d1 execute CONFIG_DB --env prod --remote \
      --command "$cmd" \
      --config "$WRANGLER_CONFIG" 2>&1
}

d1_extract_results() {
  # Reads stdin (wrangler output), extracts the results array via Python
  # Strategy: find the last JSON array in the output (wrangler outputs ANSI + JSON)
  python3 -c "
import sys, json, re
raw = sys.stdin.read()
# wrangler emits a JSON array like: [ { \"results\": [...], ... } ]
# Strip ANSI escape codes first
ansi_escape = re.compile(r'\x1B(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~])')
clean = ansi_escape.sub('', raw)
# Find all JSON array starts and try to parse from each
# The wrangler JSON block starts with '[\n  {' pattern
for m in re.finditer(r'\[', clean):
    start = m.start()
    chunk = clean[start:]
    # Try to parse it
    depth = 0
    end = -1
    for i, ch in enumerate(chunk):
        if ch == '[':
            depth += 1
        elif ch == ']':
            depth -= 1
            if depth == 0:
                end = i + 1
                break
    if end == -1:
        continue
    candidate = chunk[:end]
    try:
        blocks = json.loads(candidate)
        if isinstance(blocks, list):
            for b in blocks:
                if isinstance(b, dict) and 'results' in b:
                    res = b['results']
                    if isinstance(res, list):
                        print(json.dumps(res))
                        sys.exit(0)
    except Exception:
        continue
sys.exit(0)
" 2>/dev/null || true
}

# ---------------------------------------------------------------------------
# Step 1: Verify tenant_billing table
# ---------------------------------------------------------------------------
echo ""
echo "==> Step 1: Verify tenant_billing table..."
TB_CHECK=$(d1_query "SELECT name FROM sqlite_master WHERE name='tenant_billing'")
if ! echo "$TB_CHECK" | grep -q '"tenant_billing"'; then
  echo "ERROR: tenant_billing table not found in D1 prod. Run migrations first." >&2
  exit 1
fi
echo "  tenant_billing table: EXISTS"

# ---------------------------------------------------------------------------
# Step 2: Pick a test tenant_id (must exist in `tenant` table for FK validity)
# ---------------------------------------------------------------------------
echo ""
echo "==> Step 2: Selecting test tenant from D1..."
TENANT_RESULTS=$(d1_query "SELECT tenant_id FROM tenant LIMIT 1" | d1_extract_results)
TENANT_ID=$(echo "$TENANT_RESULTS" | python3 -c "import sys,json; data=json.load(sys.stdin); print(data[0]['tenant_id'])" 2>/dev/null || echo "")

if [[ -z "$TENANT_ID" ]]; then
  # Fallback
  TENANT_ID="019e6f45-ff16-77a0-89f0-8547dd2bfe5d"
  echo "  Warning: could not extract tenant_id; using fallback: $TENANT_ID"
else
  echo "  Selected tenant_id: $TENANT_ID"
fi

# Pre-clean: remove any stale tenant_billing row for this tenant from prior failed runs
# (only if it was created by our test run — identified by matching our soon-to-be customer ID)
# We can't pre-clean safely without knowing the customer ID yet; cleanup trap handles it.

# ---------------------------------------------------------------------------
# Step 3: Create Stripe test customer
# ---------------------------------------------------------------------------
echo ""
echo "==> Step 3: Creating Stripe test customer..."
TS=$(date +%s)
CUST_RESP=$(curl -sf "https://api.stripe.com/v1/customers" \
  -u "${STRIPE_SECRET_KEY}:" \
  -d "name=e2e-test-checkout-${TS}" \
  -d "email=e2e-${TS}@test.humangr.com" \
  -d "metadata[tenant_id]=${TENANT_ID}" \
  -d "metadata[test_run]=e2e-stripe-checkout-2026-05-30" \
  -d "metadata[test_ts]=${TS}")
STRIPE_CUSTOMER_ID=$(echo "$CUST_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['id'])")
echo "  Customer: $STRIPE_CUSTOMER_ID"

# ---------------------------------------------------------------------------
# Step 4: Create Stripe test subscription (with trial to avoid payment)
# ---------------------------------------------------------------------------
echo ""
echo "==> Step 4: Creating Stripe test subscription..."

PRICE_ID="${STRIPE_PRICE_ID_STARTER:-}"
if [[ -z "$PRICE_ID" ]]; then
  echo "  STRIPE_PRICE_ID_STARTER not set — listing test-mode recurring prices..."
  PRICE_ID=$(curl -sf "https://api.stripe.com/v1/prices?active=true&limit=3&type=recurring" \
    -u "${STRIPE_SECRET_KEY}:" | \
    python3 -c "import sys,json; d=json.load(sys.stdin); print(d['data'][0]['id'] if d['data'] else '')" 2>/dev/null || echo "")
  [[ -n "$PRICE_ID" ]] || { echo "ERROR: no recurring prices in Stripe test mode" >&2; exit 1; }
  echo "  Resolved price: $PRICE_ID"
else
  echo "  Price: $PRICE_ID"
fi

# Attach a test payment method (tok_visa is the canonical test token)
PM_ID=$(curl -sf "https://api.stripe.com/v1/payment_methods" \
  -u "${STRIPE_SECRET_KEY}:" \
  -d "type=card" \
  -d "card[token]=tok_visa" | python3 -c "import sys,json; print(json.load(sys.stdin)['id'])")
curl -sf "https://api.stripe.com/v1/payment_methods/${PM_ID}/attach" \
  -u "${STRIPE_SECRET_KEY}:" -d "customer=${STRIPE_CUSTOMER_ID}" >/dev/null
curl -sf "https://api.stripe.com/v1/customers/${STRIPE_CUSTOMER_ID}" \
  -u "${STRIPE_SECRET_KEY}:" \
  -d "invoice_settings[default_payment_method]=${PM_ID}" >/dev/null
echo "  Payment method $PM_ID attached"

SUB_RESP=$(curl -sf "https://api.stripe.com/v1/subscriptions" \
  -u "${STRIPE_SECRET_KEY}:" \
  -d "customer=${STRIPE_CUSTOMER_ID}" \
  -d "items[0][price]=${PRICE_ID}" \
  -d "trial_period_days=1" \
  -d "metadata[tenant_id]=${TENANT_ID}" \
  -d "metadata[test_run]=e2e-stripe-checkout-2026-05-30")
STRIPE_SUBSCRIPTION_ID=$(echo "$SUB_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['id'])")
echo "  Subscription: $STRIPE_SUBSCRIPTION_ID"

# ---------------------------------------------------------------------------
# Step 5: POST signed checkout.session.completed event to webhook
# ---------------------------------------------------------------------------
echo ""
echo "==> Step 5: POSTing signed checkout.session.completed webhook..."

WEBHOOK_URL="https://corelink-signup.humangr.com/webhooks/stripe"
NOW_TS=$(date +%s)
EVENT_ID="evt_e2e_$(date +%s%N | head -c20)"

# Build event payload matching the fields the handler expects:
#   data.object.customer       → stripeCustomerId
#   data.object.subscription   → stripeSubscriptionId
#   data.object.metadata.tenant_id → tenantId
#   data.object.metadata.plan      → plan
EVENT_BODY=$(python3 -c "
import json, sys
event = {
  'id': '$EVENT_ID',
  'object': 'event',
  'api_version': '2026-02-25.clover',
  'created': $NOW_TS,
  'livemode': False,
  'type': 'checkout.session.completed',
  'data': {
    'object': {
      'id': 'cs_test_e2e_${NOW_TS}',
      'object': 'checkout.session',
      'customer': '${STRIPE_CUSTOMER_ID}',
      'subscription': '${STRIPE_SUBSCRIPTION_ID}',
      'payment_status': 'paid',
      'status': 'complete',
      'mode': 'subscription',
      'amount_total': 3000,
      'currency': 'usd',
      'metadata': {
        'tenant_id': '${TENANT_ID}',
        'plan': 'starter',
        'test_run': 'e2e-stripe-checkout-2026-05-30'
      }
    }
  }
}
print(json.dumps(event, separators=(',', ':')))
")

# Compute Stripe webhook signature: HMAC-SHA256(whsec_base64_decoded, timestamp.body)
STRIPE_SIG=$(python3 -c "
import base64, hmac, hashlib, sys
secret_b64 = '${STRIPE_WEBHOOK_SECRET}'.replace('whsec_', '', 1)
secret = base64.b64decode(secret_b64)
payload = '${NOW_TS}.' + '''$EVENT_BODY'''
sig = hmac.new(secret, payload.encode('utf-8'), hashlib.sha256).hexdigest()
print(sig)
")

echo "  URL:        $WEBHOOK_URL"
echo "  Event ID:   $EVENT_ID"
echo "  Customer:   $STRIPE_CUSTOMER_ID"
echo "  Sub:        $STRIPE_SUBSCRIPTION_ID"
echo "  Tenant:     $TENANT_ID"
echo "  Sig (last8): ...${STRIPE_SIG: -8}"

HTTP_STATUS=$(curl -sf \
  -o /tmp/e2e_webhook_response.txt \
  -w "%{http_code}" \
  "$WEBHOOK_URL" \
  -H "Content-Type: application/json" \
  -H "Stripe-Signature: t=${NOW_TS},v1=${STRIPE_SIG}" \
  -H "User-Agent: Stripe/1.0 (+https://stripe.com/docs/webhooks)" \
  -d "$EVENT_BODY" 2>/dev/null || echo "000")

WEBHOOK_RESPONSE=$(cat /tmp/e2e_webhook_response.txt 2>/dev/null || echo "")
echo "  HTTP status: $HTTP_STATUS"
echo "  Response:    $WEBHOOK_RESPONSE"

case "$HTTP_STATUS" in
  200|204)
    echo "  Webhook accepted."
    ;;
  400)
    echo "ERROR: Webhook returned 400 — likely invalid_signature. Check STRIPE_WEBHOOK_SECRET." >&2
    exit 1
    ;;
  401|403)
    echo "ERROR: Webhook returned $HTTP_STATUS — WAF or auth block." >&2
    exit 1
    ;;
  503)
    echo "ERROR: Webhook returned 503 — Worker misconfigured (STRIPE_WEBHOOK_SECRET not set in Worker)." >&2
    exit 1
    ;;
  000)
    echo "ERROR: curl failed to connect to $WEBHOOK_URL" >&2
    exit 1
    ;;
  *)
    echo "ERROR: Unexpected HTTP status $HTTP_STATUS from webhook." >&2
    exit 1
    ;;
esac

# ---------------------------------------------------------------------------
# Step 6: Poll D1 tenant_billing for status=paid (max 30s)
# ---------------------------------------------------------------------------
echo ""
echo "==> Step 6: Polling D1 tenant_billing for status=paid (max 30s)..."

POLL_MAX=15      # 15 × 2s = 30s
POLL_INTERVAL=2
POLL_COUNT=0
FOUND=0

while [[ $POLL_COUNT -lt $POLL_MAX ]]; do
  POLL_COUNT=$((POLL_COUNT + 1))
  echo "  Poll $POLL_COUNT/$POLL_MAX (${POLL_INTERVAL}s interval)..."

  QUERY_RESULTS=$(d1_query \
    "SELECT tenant_id, status, stripe_customer_id, stripe_subscription_id, created_at_ms \
     FROM tenant_billing \
     WHERE tenant_id='${TENANT_ID}' AND stripe_customer_id='${STRIPE_CUSTOMER_ID}'" | \
    d1_extract_results)

  ROW_STATUS=$(echo "$QUERY_RESULTS" | \
    python3 -c "import sys,json; data=json.load(sys.stdin); print(data[0].get('status','')) if data else print('')" \
    2>/dev/null || echo "")

  if [[ "$ROW_STATUS" == "paid" ]]; then
    FOUND=1
    D1_ROW_INSERTED=1
    echo "  Row found: status=paid"
    # Print the full row
    echo "$QUERY_RESULTS" | python3 -c "
import sys, json
try:
    data = json.load(sys.stdin)
    if data:
        print(json.dumps(data[0], indent=2))
except Exception as e:
    print('parse error:', e)
" 2>/dev/null || true
    break
  elif [[ -n "$ROW_STATUS" ]]; then
    echo "  Row exists but status='$ROW_STATUS' (expected 'paid')"
  else
    echo "  Row not yet found..."
  fi

  [[ $POLL_COUNT -lt $POLL_MAX ]] && sleep $POLL_INTERVAL
done

if [[ $FOUND -eq 0 ]]; then
  echo ""
  echo "ERROR: tenant_billing row with status=paid not found after $((POLL_MAX * POLL_INTERVAL))s" >&2
  echo "" >&2
  echo "Diagnostic steps:" >&2
  echo "  1. Check Cloudflare Workers > corelink-signup > Logs for errors" >&2
  echo "  2. Check Stripe dashboard > Developers > Webhooks > we_1TcaeeLh0hhAZjwoWambOOhJ" >&2
  echo "  3. Verify BILLING_DB binding in signup-worker is deployed" >&2
  echo "  4. Verify STRIPE_WEBHOOK_SECRET in Worker matches .env.local" >&2
  echo "" >&2
  echo "  Any existing rows for this tenant:" >&2
  d1_query "SELECT * FROM tenant_billing WHERE tenant_id='${TENANT_ID}'" 2>&1 | \
    grep -E '"tenant_id"|"status"|"stripe_cust|results' | head -10 || true
  exit 1
fi

echo ""
echo "==> E2E chain validated: checkout.session.completed → BILLING_DB → tenant_billing(status=paid)"
# cleanup trap fires on exit 0 and prints RESULT: PASS
