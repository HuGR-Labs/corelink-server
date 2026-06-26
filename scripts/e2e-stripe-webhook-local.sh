#!/usr/bin/env bash
# e2e-stripe-webhook-local.sh — LOCAL Stripe webhook → tier integration harness.
#
# WHY THIS EXISTS (the gap it closes):
#   apps/signup-worker/tests/stripe.test.ts unit-tests handleStripeWebhook
#   against an IN-MEMORY D1 fake. That fake does NOT enforce SQLite CHECK
#   constraints, UNIQUE partial indexes, or ON CONFLICT semantics — so a
#   migration/SQL bug (a CHECK that rejects a real activation, a 6-tier value
#   the schema forbids, an upsert that violates subscription_started_when_active)
#   passes the unit tests but breaks in production. This harness runs the REAL
#   worker bundle (real index.ts routing + real binding wiring) via
#   `wrangler dev --local` against a REAL local SQLite D1 created from the REAL
#   migration DDL, then drives signed webhooks and asserts the persisted rows.
#
# SAFE BY CONSTRUCTION: everything is local (miniflare). It NEVER touches prod
#   D1, NEVER calls Stripe, and uses a self-generated test whsec. The prod
#   signed-webhook→tier path needs the owner's LIVE whsec + a prod-billing write
#   and is therefore validated by the deployed worker, not here.
#
# Scenarios (each FAILS the script on a wrong assertion → real gate):
#   A  checkout.session.completed (paid, tier=starter) → 200; tier_selections
#      flips pending_checkout→active (real subscription_started_when_active CHECK)
#      + tenant_billing status=paid, plan=starter.
#   B  checkout.session.completed (paid, tier=max) → 200; the 6-tier widen
#      (migration 0062) is REAL: a `max` activation that 0039's CHECK would
#      reject must succeed against the live DDL.
#   C  checkout.session.completed (payment_status=unpaid) → 200, ZERO writes
#      (negative teeth: an unproven payment must not entitle).
#   D  bad signature → 400 (no side effect).
#   E  customer.subscription.deleted → 200; tenant_billing canceled +
#      tier_selections inactive (revocation reaches the canonical gate).
#
# Usage:  bash scripts/e2e-stripe-webhook-local.sh
# Exit:   0 = all scenarios PASS;  1 = a scenario failed (see output).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
WORKER_DIR="$REPO_ROOT/apps/signup-worker"
WRANGLER="$WORKER_DIR/node_modules/wrangler/bin/wrangler.js"
PORT="${PORT:-8799}"
PERSIST="$(mktemp -d "${TMPDIR:-/tmp}/sw-stripe-harness.XXXXXX")"
DEVVARS="$WORKER_DIR/.dev.vars"
DEVVARS_BAK=""
DEV_PID=""

command -v node >/dev/null || { echo "ERROR: node not found" >&2; exit 1; }
[[ -f "$WRANGLER" ]] || { echo "ERROR: wrangler not found at $WRANGLER" >&2; exit 1; }

# A self-generated whsec. decodeWebhookSecret() base64-decodes everything after
# the whsec_ prefix, so the suffix MUST be valid base64.
WHSEC="whsec_$(printf 'corelink-local-harness-secret-do-not-deploy' | base64 | tr -d '\n')"

cleanup() {
  set +e
  [[ -n "$DEV_PID" ]] && kill "$DEV_PID" 2>/dev/null
  pkill -f "wrangler.*dev.*--port $PORT" 2>/dev/null
  pkill -f "workerd.*$PORT" 2>/dev/null
  if [[ -n "$DEVVARS_BAK" ]]; then
    mv -f "$DEVVARS_BAK" "$DEVVARS" 2>/dev/null
  else
    rm -f "$DEVVARS" 2>/dev/null
  fi
  rm -rf "$PERSIST" 2>/dev/null
}
trap cleanup EXIT

# ---------------------------------------------------------------------------
# 1. Real-DDL local schema (the 3 tables the webhook write-path touches).
#    Copied verbatim from migrations 0062 (tier_selections, 6-tier),
#    0055 (tenant_billing), 0044 (stripe_webhook_events_processed) — so the
#    CHECK/UNIQUE constraints are the SAME ones prod enforces.
# ---------------------------------------------------------------------------
SCHEMA="$PERSIST/schema.sql"
cat > "$SCHEMA" <<'SQL'
CREATE TABLE IF NOT EXISTS tier_selections (
    tenant_id                  TEXT    NOT NULL PRIMARY KEY,
    tier                       TEXT    NOT NULL
        CHECK (tier IN ('free','solo','starter','team','pro','max','enterprise')),
    subscription_state         TEXT    NOT NULL
        CHECK (subscription_state IN ('inactive','pending_checkout','active')),
    stripe_customer_id         TEXT,
    subscription_started_at_ms BIGINT,
    schema_version             INTEGER NOT NULL DEFAULT 1,
    correlation_id             TEXT    NOT NULL,
    CONSTRAINT subscription_started_when_active CHECK (
        (subscription_state = 'active' AND subscription_started_at_ms IS NOT NULL)
        OR (subscription_state <> 'active')
    )
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_tenant_active_subscription
    ON tier_selections (tenant_id) WHERE subscription_state = 'active';

CREATE TABLE IF NOT EXISTS tenant_billing (
    tenant_id              TEXT    NOT NULL PRIMARY KEY,
    stripe_customer_id     TEXT,
    stripe_subscription_id TEXT,
    status                 TEXT    NOT NULL DEFAULT 'inactive'
        CHECK (status IN ('inactive','paid','canceled','past_due','incomplete')),
    plan                   TEXT,
    current_period_end_ms  BIGINT,
    schema_version         INTEGER NOT NULL DEFAULT 1,
    created_at_ms          BIGINT  NOT NULL,
    updated_at_ms          BIGINT  NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_tenant_billing_customer_id
    ON tenant_billing (stripe_customer_id) WHERE stripe_customer_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_tenant_billing_subscription_id
    ON tenant_billing (stripe_subscription_id) WHERE stripe_subscription_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS stripe_webhook_events_processed (
    event_id        TEXT    NOT NULL PRIMARY KEY,
    event_type      TEXT    NOT NULL,
    processed_at_ms BIGINT  NOT NULL,
    outcome         TEXT    NOT NULL
        CHECK (outcome IN ('dispatched','acknowledged_unknown')),
    correlation_id  TEXT    NOT NULL
);
SQL

# Clear any stale process holding the port (a prior aborted run can leave a
# workerd bound to it, which makes the fresh dev server fail to listen).
lsof -ti:"$PORT" 2>/dev/null | xargs -r kill -9 2>/dev/null || true

echo "▶ harness persist dir: $PERSIST"
echo "▶ applying real-DDL schema to local D1 (CONFIG_DB)…"
node "$WRANGLER" d1 execute CONFIG_DB --local --persist-to "$PERSIST" \
  --config "$WORKER_DIR/wrangler.toml" --file "$SCHEMA" --yes >/dev/null

# Pre-seed a pending_checkout row so scenario A exercises the real
# pending_checkout→active transition (not just an insert).
node "$WRANGLER" d1 execute CONFIG_DB --local --persist-to "$PERSIST" \
  --config "$WORKER_DIR/wrangler.toml" --yes --command \
  "INSERT INTO tier_selections (tenant_id,tier,subscription_state,correlation_id)
   VALUES ('harness-tenant-A','starter','pending_checkout','seed:A');" >/dev/null

# ---------------------------------------------------------------------------
# 2. .dev.vars with the test whsec (backup + restore any real one).
# ---------------------------------------------------------------------------
[[ -f "$DEVVARS" ]] && { DEVVARS_BAK="$DEVVARS.harness-bak"; cp -f "$DEVVARS" "$DEVVARS_BAK"; }
cat > "$DEVVARS" <<EOF
STRIPE_WEBHOOK_SECRET=$WHSEC
STRIPE_PRICE_ID_SOLO=price_test_solo
STRIPE_PRICE_ID_STARTER=price_test_starter
STRIPE_PRICE_ID_TEAM=price_test_team
STRIPE_PRICE_ID_PRO=price_test_pro
STRIPE_PRICE_ID_MAX=price_test_max
EOF

# ---------------------------------------------------------------------------
# 3. Boot wrangler dev --local.
# ---------------------------------------------------------------------------
echo "▶ booting wrangler dev --local on port ${PORT}"
( cd "$WORKER_DIR" && exec node "$WRANGLER" dev --local --port "$PORT" \
    --persist-to "$PERSIST" >"$PERSIST/dev.log" 2>&1 ) &
DEV_PID=$!

ready=0
for _ in $(seq 1 60); do
  code=$(curl -s -o /dev/null -w '%{http_code}' --max-time 2 "http://127.0.0.1:$PORT/health" 2>/dev/null || true)
  [[ "$code" == "200" ]] && { ready=1; break; }
  # connection-refused returns instantly, so we MUST pace the poll or all 60
  # attempts burn before wrangler finishes binding (~2-3s cold start).
  sleep 1
done
[[ "$ready" == "1" ]] || { echo "ERROR: dev server did not become ready"; tail -30 "$PERSIST/dev.log"; exit 1; }
echo "  ready."

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------
# query_d1 <SQL> → prints the JSON results array (retries on transient lock).
query_d1() {
  local sql="$1" out
  for _ in $(seq 1 8); do
    out=$(node "$WRANGLER" d1 execute CONFIG_DB --local --persist-to "$PERSIST" \
      --config "$WORKER_DIR/wrangler.toml" --yes --json --command "$sql" 2>/dev/null) || true
    if [[ -n "$out" ]]; then printf '%s' "$out"; return 0; fi
  done
  printf '%s' "$out"
}

# field <json> <col>  → first-row column value (or empty)
field() {
  printf '%s' "$1" | python3 -c "
import sys,json
try:
    d=json.load(sys.stdin)
    rows=d[0]['results'] if isinstance(d,list) else d['results']
    print(rows[0].get('$2','') if rows else '')
except Exception:
    print('')
"
}

# post_event <signed?> <json-body>  → prints HTTP status. signed=1 uses WHSEC.
PASS=0; FAIL=0
post_event() {
  WHSEC="$WHSEC" PORT="$PORT" python3 - "$1" "$2" <<'PY'
import os,sys,time,hmac,hashlib,base64,json,urllib.request,urllib.error
signed=sys.argv[1]=="1"; body=sys.argv[2]
whsec=os.environ["WHSEC"]; port=os.environ["PORT"]
ts=str(int(time.time()))
if signed:
    key=base64.b64decode(whsec[len("whsec_"):])
    sig=hmac.new(key,f"{ts}.{body}".encode(),hashlib.sha256).hexdigest()
else:
    sig="deadbeef"
hdr=f"t={ts},v1={sig}"
req=urllib.request.Request(f"http://127.0.0.1:{port}/webhooks/stripe",
    data=body.encode(),method="POST",
    headers={"content-type":"application/json","stripe-signature":hdr})
try:
    r=urllib.request.urlopen(req,timeout=15); print(r.status)
except urllib.error.HTTPError as e: print(e.code)
except Exception as e: print("ERR:"+type(e).__name__)
PY
}

assert() { # <label> <expected> <actual>
  if [[ "$2" == "$3" ]]; then echo "  ✓ $1"; PASS=$((PASS+1));
  else echo "  ✗ $1 — expected [$2] got [$3]"; FAIL=$((FAIL+1)); fi
}

NOW=$(node -e 'console.log(Date.now())')

# ---------------------------------------------------------------------------
# Scenario A — paid checkout.session.completed → activate (starter)
# ---------------------------------------------------------------------------
echo "▶ A: checkout.session.completed (paid, starter) → activate"
A_BODY=$(cat <<EOF
{"id":"evt_A_$NOW","type":"checkout.session.completed","data":{"object":{
  "id":"cs_A","object":"checkout.session","payment_status":"paid",
  "customer":"cus_A","subscription":"sub_A","amount_total":3500,
  "metadata":{"tenant_id":"harness-tenant-A","tier":"starter"}}}}
EOF
)
assert "HTTP 200" 200 "$(post_event 1 "$A_BODY")"
A_TS=$(query_d1 "SELECT subscription_state st, tier, subscription_started_at_ms sa FROM tier_selections WHERE tenant_id='harness-tenant-A';")
assert "tier_selections.subscription_state=active" active "$(field "$A_TS" st)"
assert "tier_selections.tier=starter" starter "$(field "$A_TS" tier)"
A_SA=$(field "$A_TS" sa); [[ -n "$A_SA" && "$A_SA" != "None" ]] && assert "subscription_started_at_ms set (real CHECK satisfied)" ok ok || assert "subscription_started_at_ms set (real CHECK satisfied)" ok "missing"
A_TB=$(query_d1 "SELECT status, plan FROM tenant_billing WHERE tenant_id='harness-tenant-A';")
assert "tenant_billing.status=paid" paid "$(field "$A_TB" status)"
assert "tenant_billing.plan=starter" starter "$(field "$A_TB" plan)"

# ---------------------------------------------------------------------------
# Scenario B — 6-tier widen is REAL: tier=max must activate (0039 CHECK forbade it)
# ---------------------------------------------------------------------------
echo "▶ B: checkout.session.completed (paid, max) → 6-tier widen is live"
B_BODY=$(cat <<EOF
{"id":"evt_B_$NOW","type":"checkout.session.completed","data":{"object":{
  "id":"cs_B","object":"checkout.session","payment_status":"paid",
  "customer":"cus_B","subscription":"sub_B","amount_total":14900,
  "metadata":{"tenant_id":"harness-tenant-B","tier":"max"}}}}
EOF
)
assert "HTTP 200" 200 "$(post_event 1 "$B_BODY")"
B_TS=$(query_d1 "SELECT subscription_state st, tier FROM tier_selections WHERE tenant_id='harness-tenant-B';")
assert "tier_selections.tier=max activates (no CHECK violation)" max "$(field "$B_TS" tier)"
assert "tier_selections.subscription_state=active" active "$(field "$B_TS" st)"

# ---------------------------------------------------------------------------
# Scenario C — payment_status=unpaid → 200 but ZERO writes (negative teeth)
# ---------------------------------------------------------------------------
echo "▶ C: checkout.session.completed (unpaid) → no entitlement"
C_BODY=$(cat <<EOF
{"id":"evt_C_$NOW","type":"checkout.session.completed","data":{"object":{
  "id":"cs_C","object":"checkout.session","payment_status":"unpaid",
  "customer":"cus_C","subscription":"sub_C","amount_total":3500,
  "metadata":{"tenant_id":"harness-tenant-C","tier":"starter"}}}}
EOF
)
assert "HTTP 200" 200 "$(post_event 1 "$C_BODY")"
C_CNT=$(query_d1 "SELECT COUNT(*) c FROM tier_selections WHERE tenant_id='harness-tenant-C';")
assert "tier_selections NOT written for unpaid session" 0 "$(field "$C_CNT" c)"

# ---------------------------------------------------------------------------
# Scenario D — bad signature → 400, no side effect
# ---------------------------------------------------------------------------
echo "▶ D: bad signature → 400"
assert "HTTP 400 (invalid_signature)" 400 "$(post_event 0 "$A_BODY")"

# ---------------------------------------------------------------------------
# Scenario E — customer.subscription.deleted → revoke (cancel + inactive)
# ---------------------------------------------------------------------------
echo "▶ E: customer.subscription.deleted (sub_A) → revoke gate"
E_BODY=$(cat <<EOF
{"id":"evt_E_$NOW","type":"customer.subscription.deleted","data":{"object":{
  "id":"sub_A","object":"subscription","customer":"cus_A","status":"canceled"}}}
EOF
)
assert "HTTP 200" 200 "$(post_event 1 "$E_BODY")"
E_TB=$(query_d1 "SELECT status FROM tenant_billing WHERE tenant_id='harness-tenant-A';")
assert "tenant_billing.status=canceled" canceled "$(field "$E_TB" status)"
E_TS=$(query_d1 "SELECT subscription_state st FROM tier_selections WHERE tenant_id='harness-tenant-A';")
assert "tier_selections.subscription_state=inactive (gate revoked)" inactive "$(field "$E_TS" st)"

# ---------------------------------------------------------------------------
# Verdict
# ---------------------------------------------------------------------------
echo
echo "════════════════════════════════════════════"
echo " Stripe webhook LOCAL harness: PASS=$PASS  FAIL=$FAIL"
echo "════════════════════════════════════════════"
[[ "$FAIL" == "0" ]] && { echo "RESULT: PASS"; exit 0; } || { echo "RESULT: FAIL"; exit 1; }
