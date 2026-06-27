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
#   F  checkout.session.completed (paid, tier=solo) → 200; mirrors A:
#      pending_checkout→active transition + billing paid + plan=solo.
#   G  checkout.session.completed (paid, tier=team) → 200; mirrors A:
#      pending_checkout→active transition + billing paid + plan=team.
#   H  checkout.session.completed (paid, tier=pro) → 200; mirrors A:
#      pending_checkout→active transition + billing paid + plan=pro.
#
# Usage:
#   bash scripts/e2e-stripe-webhook-local.sh            # full 15/15 run (default)
#   bash scripts/e2e-stripe-webhook-local.sh --setup    # boot + leave running,
#                                                        # emit eval-able handles
#   bash scripts/e2e-stripe-webhook-local.sh --teardown # stop a --setup server
# Exit:   0 = all scenarios PASS (run) / ready (setup) / stopped (teardown);
#         1 = a scenario failed (run mode only; see output).
#
# REUSE (WP: e2e-real-client/stripe-test-env.sh): `--setup` applies the SAME
# real-DDL schema + boots the SAME wrangler-dev signup-worker as the default run,
# but instead of driving the 8 scenarios it persists the runtime handles to a
# state file and prints a clean, eval-able KEY=VALUE block on STDOUT
# (HARNESS_PORT / HARNESS_WHSEC / HARNESS_PERSIST / HARNESS_WRANGLER /
# HARNESS_WORKER_DIR / HARNESS_STATE_FILE) so a caller can point the e2e billing
# webhook journeys at this LOCAL receiver (never firing synthetic events at
# prod). `--teardown` stops it. The default (no-arg) run is byte-for-byte
# unchanged (still 15/15).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
WORKER_DIR="$REPO_ROOT/apps/signup-worker"
WRANGLER="$WORKER_DIR/node_modules/wrangler/bin/wrangler.js"
PORT="${PORT:-8799}"
DEVVARS="$WORKER_DIR/.dev.vars"
DEVVARS_BAK=""
DEV_PID=""
PERSIST=""

# Mode: run (default, full 15/15) | setup (boot + emit handles, stay alive) |
# teardown (stop a prior --setup). Parsed from the first arg.
MODE="run"
case "${1:-}" in
  --setup|--emit-env) MODE="setup" ;;
  --teardown)         MODE="teardown" ;;
  ""|--run)           MODE="run" ;;
  *) echo "ERROR: unknown arg '${1}' (use --setup | --teardown | no arg)" >&2; exit 2 ;;
esac

# Stable state-file path keyed by PORT so a later --teardown (same PORT) finds
# the --setup server's handles without arguments.
STATE_FILE="${TMPDIR:-/tmp}/corelink-e2e-stripe-harness.${PORT}.state"

# Progress goes to STDERR so `--setup` keeps STDOUT a clean, eval-able block.
log() { echo "$@" >&2; }

command -v node >/dev/null || { echo "ERROR: node not found" >&2; exit 1; }
[[ -f "$WRANGLER" ]] || { echo "ERROR: wrangler not found at $WRANGLER" >&2; exit 1; }

# ---------------------------------------------------------------------------
# --teardown: stop a server a prior --setup left running, then exit.
# ---------------------------------------------------------------------------
if [[ "$MODE" == "teardown" ]]; then
  if [[ -f "$STATE_FILE" ]]; then
    # shellcheck disable=SC1090
    . "$STATE_FILE"
    set +e
    [[ -n "${DEV_PID:-}" ]] && kill "$DEV_PID" 2>/dev/null
    pkill -f "wrangler.*dev.*--port ${PORT}" 2>/dev/null
    pkill -f "workerd.*${PORT}" 2>/dev/null
    if [[ -n "${DEVVARS_BAK:-}" && -f "${DEVVARS_BAK}" ]]; then
      mv -f "$DEVVARS_BAK" "$DEVVARS" 2>/dev/null
    else
      rm -f "$DEVVARS" 2>/dev/null
    fi
    [[ -n "${PERSIST:-}" ]] && rm -rf "$PERSIST" 2>/dev/null
    rm -f "$STATE_FILE" 2>/dev/null
    log "▶ teardown: stopped harness on port ${PORT}"
  else
    log "▶ teardown: no state file at $STATE_FILE (nothing to stop)"
  fi
  exit 0
fi

PERSIST="$(mktemp -d "${TMPDIR:-/tmp}/sw-stripe-harness.XXXXXX")"

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
# Only the full RUN mode auto-cleans on exit. `--setup` intentionally leaves the
# dev server + persist dir alive for the caller; `--teardown` reclaims them.
if [[ "$MODE" == "run" ]]; then
  trap cleanup EXIT
fi

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

log "▶ harness persist dir: $PERSIST"
log "▶ applying real-DDL schema to local D1 (CONFIG_DB)…"
node "$WRANGLER" d1 execute CONFIG_DB --local --persist-to "$PERSIST" \
  --config "$WORKER_DIR/wrangler.toml" --file "$SCHEMA" --yes >/dev/null

# Pre-seed the pending_checkout rows that scenarios A/F/G/H exercise. These are
# RUN-mode only: --setup boots a clean schema for the caller to seed its own
# (sub_e2e/cus_e2e) fixtures.
if [[ "$MODE" == "run" ]]; then
  # Scenario A — real pending_checkout→active transition (not just an insert).
  node "$WRANGLER" d1 execute CONFIG_DB --local --persist-to "$PERSIST" \
    --config "$WORKER_DIR/wrangler.toml" --yes --command \
    "INSERT INTO tier_selections (tenant_id,tier,subscription_state,correlation_id)
     VALUES ('harness-tenant-A','starter','pending_checkout','seed:A');" >/dev/null

  # Scenarios F/G/H — same pending_checkout→active transition as scenario A.
  node "$WRANGLER" d1 execute CONFIG_DB --local --persist-to "$PERSIST" \
    --config "$WORKER_DIR/wrangler.toml" --yes --command \
    "INSERT INTO tier_selections (tenant_id,tier,subscription_state,correlation_id)
     VALUES ('harness-tenant-SOLO','solo','pending_checkout','seed:SOLO');" >/dev/null
  node "$WRANGLER" d1 execute CONFIG_DB --local --persist-to "$PERSIST" \
    --config "$WORKER_DIR/wrangler.toml" --yes --command \
    "INSERT INTO tier_selections (tenant_id,tier,subscription_state,correlation_id)
     VALUES ('harness-tenant-TEAM','team','pending_checkout','seed:TEAM');" >/dev/null
  node "$WRANGLER" d1 execute CONFIG_DB --local --persist-to "$PERSIST" \
    --config "$WORKER_DIR/wrangler.toml" --yes --command \
    "INSERT INTO tier_selections (tenant_id,tier,subscription_state,correlation_id)
     VALUES ('harness-tenant-PRO','pro','pending_checkout','seed:PRO');" >/dev/null
fi

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
log "▶ booting wrangler dev --local on port ${PORT}"
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
[[ "$ready" == "1" ]] || { echo "ERROR: dev server did not become ready" >&2; tail -30 "$PERSIST/dev.log" >&2; exit 1; }
log "  ready."

# ---------------------------------------------------------------------------
# --setup: persist the runtime handles + emit an eval-able block, then exit
# WITHOUT cleaning up (the caller drives the journeys then runs --teardown).
# ---------------------------------------------------------------------------
if [[ "$MODE" == "setup" ]]; then
  cat > "$STATE_FILE" <<STATE
DEV_PID=$DEV_PID
PERSIST=$PERSIST
DEVVARS=$DEVVARS
DEVVARS_BAK=$DEVVARS_BAK
WHSEC=$WHSEC
PORT=$PORT
WRANGLER=$WRANGLER
WORKER_DIR=$WORKER_DIR
STATE
  log "▶ setup: harness ready on port ${PORT}; state → $STATE_FILE"
  # The ONLY stdout output: a clean, eval-able KEY=VALUE block.
  echo "HARNESS_PORT=$PORT"
  echo "HARNESS_WHSEC=$WHSEC"
  echo "HARNESS_PERSIST=$PERSIST"
  echo "HARNESS_WRANGLER=$WRANGLER"
  echo "HARNESS_WORKER_DIR=$WORKER_DIR"
  echo "HARNESS_STATE_FILE=$STATE_FILE"
  exit 0
fi

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
# Scenario F — paid checkout.session.completed → activate (solo)
# ---------------------------------------------------------------------------
echo "▶ F: checkout.session.completed (paid, solo) → activate"
F_BODY=$(cat <<EOF
{"id":"evt_F_$NOW","type":"checkout.session.completed","data":{"object":{
  "id":"cs_F","object":"checkout.session","payment_status":"paid",
  "customer":"cus_F","subscription":"sub_F","amount_total":1500,
  "metadata":{"tenant_id":"harness-tenant-SOLO","tier":"solo"}}}}
EOF
)
assert "HTTP 200" 200 "$(post_event 1 "$F_BODY")"
F_TS=$(query_d1 "SELECT subscription_state st, tier, subscription_started_at_ms sa FROM tier_selections WHERE tenant_id='harness-tenant-SOLO';")
assert "tier_selections.subscription_state=active" active "$(field "$F_TS" st)"
assert "tier_selections.tier=solo" solo "$(field "$F_TS" tier)"
F_SA=$(field "$F_TS" sa); [[ -n "$F_SA" && "$F_SA" != "None" ]] && assert "subscription_started_at_ms set (real CHECK satisfied)" ok ok || assert "subscription_started_at_ms set (real CHECK satisfied)" ok "missing"
F_TB=$(query_d1 "SELECT status, plan FROM tenant_billing WHERE tenant_id='harness-tenant-SOLO';")
assert "tenant_billing.status=paid" paid "$(field "$F_TB" status)"
assert "tenant_billing.plan=solo" solo "$(field "$F_TB" plan)"

# ---------------------------------------------------------------------------
# Scenario G — paid checkout.session.completed → activate (team)
# ---------------------------------------------------------------------------
echo "▶ G: checkout.session.completed (paid, team) → activate"
G_BODY=$(cat <<EOF
{"id":"evt_G_$NOW","type":"checkout.session.completed","data":{"object":{
  "id":"cs_G","object":"checkout.session","payment_status":"paid",
  "customer":"cus_G","subscription":"sub_G","amount_total":4500,
  "metadata":{"tenant_id":"harness-tenant-TEAM","tier":"team"}}}}
EOF
)
assert "HTTP 200" 200 "$(post_event 1 "$G_BODY")"
G_TS=$(query_d1 "SELECT subscription_state st, tier, subscription_started_at_ms sa FROM tier_selections WHERE tenant_id='harness-tenant-TEAM';")
assert "tier_selections.subscription_state=active" active "$(field "$G_TS" st)"
assert "tier_selections.tier=team" team "$(field "$G_TS" tier)"
G_SA=$(field "$G_TS" sa); [[ -n "$G_SA" && "$G_SA" != "None" ]] && assert "subscription_started_at_ms set (real CHECK satisfied)" ok ok || assert "subscription_started_at_ms set (real CHECK satisfied)" ok "missing"
G_TB=$(query_d1 "SELECT status, plan FROM tenant_billing WHERE tenant_id='harness-tenant-TEAM';")
assert "tenant_billing.status=paid" paid "$(field "$G_TB" status)"
assert "tenant_billing.plan=team" team "$(field "$G_TB" plan)"

# ---------------------------------------------------------------------------
# Scenario H — paid checkout.session.completed → activate (pro)
# ---------------------------------------------------------------------------
echo "▶ H: checkout.session.completed (paid, pro) → activate"
H_BODY=$(cat <<EOF
{"id":"evt_H_$NOW","type":"checkout.session.completed","data":{"object":{
  "id":"cs_H","object":"checkout.session","payment_status":"paid",
  "customer":"cus_H","subscription":"sub_H","amount_total":5000,
  "metadata":{"tenant_id":"harness-tenant-PRO","tier":"pro"}}}}
EOF
)
assert "HTTP 200" 200 "$(post_event 1 "$H_BODY")"
H_TS=$(query_d1 "SELECT subscription_state st, tier, subscription_started_at_ms sa FROM tier_selections WHERE tenant_id='harness-tenant-PRO';")
assert "tier_selections.subscription_state=active" active "$(field "$H_TS" st)"
assert "tier_selections.tier=pro" pro "$(field "$H_TS" tier)"
H_SA=$(field "$H_TS" sa); [[ -n "$H_SA" && "$H_SA" != "None" ]] && assert "subscription_started_at_ms set (real CHECK satisfied)" ok ok || assert "subscription_started_at_ms set (real CHECK satisfied)" ok "missing"
H_TB=$(query_d1 "SELECT status, plan FROM tenant_billing WHERE tenant_id='harness-tenant-PRO';")
assert "tenant_billing.status=paid" paid "$(field "$H_TB" status)"
assert "tenant_billing.plan=pro" pro "$(field "$H_TB" plan)"

# ---------------------------------------------------------------------------
# Verdict
# ---------------------------------------------------------------------------
echo
echo "════════════════════════════════════════════"
echo " Stripe webhook LOCAL harness: PASS=$PASS  FAIL=$FAIL"
echo "════════════════════════════════════════════"
[[ "$FAIL" == "0" ]] && { echo "RESULT: PASS"; exit 0; } || { echo "RESULT: FAIL"; exit 1; }
