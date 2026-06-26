#!/usr/bin/env bash
# scripts/e2e-real-client/provision-and-run-suite.sh
#
# One-command go-live cert for the BLACK-BOX Rust journey suite
# (tests/e2e-user-journeys). It provisions the full persona set THE REAL WAY a
# customer would (Clerk signup → PAT; customer keys.create for read-only / admin;
# a create→revoke for the revoked persona), exports the CORELINK_E2E_* contract,
# runs the suite, and DSR-deletes the test users it created.
#
# WHY: the Rust security/regression matrix GATES (correctly) without the persona
# tokens; an operator who provisions them WRONG (e.g. the wrong pat_id field) gets
# a false-RED. This script provisions them correctly + deterministically so the
# suite runs GREEN — the security half of the protection moat, repeatable.
#
# WHAT THIS SCRIPT CAN AND CANNOT PROVISION (black-box, no D1 access):
#   - SELF-MINTABLE via the real signup + keys.create path (provisioned here):
#       P1 RW, P6 TenantB     — Clerk signup → tenant + RW PAT.
#       P2 RO                 — keys.create with ["cache:read"].
#       P3 Admin              — keys.create with ["cache:read","cache:write","admin"].
#       P4 Revoked            — keys.create then keys/:id/revoke.
#   - NOT self-mintable (need a D1 seed row this black-box script cannot write —
#     tier_selections / runners_entitlement / subscription_state / PAT expiry).
#     These are read from env / OOB and GATE cleanly when absent (never faked):
#       P5  Expired           — CORELINK_E2E_PAT_EXPIRED (keys.create has no
#                               expires_at; a real short-TTL PAT must be supplied).
#       P7  Free  tenant      — CORELINK_E2E_PAT_FREE        (+ seeded tenant).
#       P8  Solo  tenant      — CORELINK_E2E_PAT_SOLO        (+ seeded tenant).
#       P9  Pro   tenant      — CORELINK_E2E_PAT_PRO         (+ seeded tenant).
#       P10 Enterprise tenant — CORELINK_E2E_PAT_ENTERPRISE  (+ seeded tenant).
#       P11 PastDue           — CORELINK_E2E_PAT_PASTDUE      (sub != active).
#       Runner (M8)           — CORELINK_E2E_PAT_RUNNER + CORELINK_E2E_RUNNER_TENANT
#                               (needs a runners_entitlement row).
#       Money  (M4)           — CORELINK_E2E_STRIPE_TEST=1 + the Stripe-test
#                               fixtures the billing journeys read (see below).
#       Moat   (M2)           — CORELINK_E2E_PUBLIC_HASH (a known _public artifact).
#       Team   (M13)          — CORELINK_E2E_TEAM_INVITE_EMAIL.
#
# Real clients (docker/cargo/brew/native) are certified by the sibling run.sh;
# this is the HTTP black-box half (cross-tenant isolation, revocation, OCI 9-fix
# regressions, adapter regressions, money/compliance/runners journeys).
#
# Usage:  bash scripts/e2e-real-client/provision-and-run-suite.sh
# Env:    CLERK_LIVE_SECRET_KEY|CLERK_SECRET_KEY (from env or .env.local)
#         CORELINK_E2E_ENDPOINT     (default https://corelink-api.humangr.com)
#         CORELINK_E2E_OCI_ENDPOINT (default https://corelink-oci.humangr.com)
#         CORELINK_E2E_MIN_PASS     (default 40 — the substantive-pass floor; the
#                                    suite goes RED if a provisioning regression
#                                    re-gates it below this many PASS rows)
#         Owner-supplied persona/cred env (read + gated, NEVER hardcoded):
#           CORELINK_E2E_PAT_{EXPIRED,FREE,SOLO,PRO,ENTERPRISE,PASTDUE,RUNNER}
#           CORELINK_E2E_RUNNER_TENANT
#           CORELINK_E2E_PUBLIC_HASH, CORELINK_E2E_TEAM_INVITE_EMAIL
#         Money (M4) — Stripe-TEST fixtures, all read from env/OOB:
#           CORELINK_E2E_STRIPE_TEST=1
#           CORELINK_E2E_CLERK_SESSION           (Clerk session bearer; checkout)
#           CORELINK_E2E_STRIPE_WEBHOOK_TEST=1   (opt into the mutating webhook sim)
#           CORELINK_E2E_SIGNUP_WORKER_ENDPOINT  (signup-worker base URL)
#           CORELINK_E2E_STRIPE_WEBHOOK_SECRET   (whsec_… signing secret)
#           CORELINK_E2E_STRIPE_SUBSCRIPTION_ID  (test tenant Stripe sub id)
#           CORELINK_E2E_STRIPE_CUSTOMER_ID      (test tenant Stripe customer id)
#           CORELINK_E2E_STRIPE_PRICE_ID         (optional; drives price→tier map)
#         Opt-in destructive/charging GATES (off by default; forwarded if set):
#           CORELINK_E2E_QUOTA_TEST, CORELINK_E2E_DSR_TEST, CORELINK_E2E_STRIPE_WEBHOOK_TEST
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
ENV_FILE="${REPO_ROOT}/.env.local"
API="${CORELINK_E2E_ENDPOINT:-https://corelink-api.humangr.com}"
OCI="${CORELINK_E2E_OCI_ENDPOINT:-https://corelink-oci.humangr.com}"

# ── Clerk secret (env first, then .env.local; never echoed) ──────────────────
SK="${CLERK_LIVE_SECRET_KEY:-${CLERK_SECRET_KEY:-}}"
if [ -z "$SK" ] && [ -f "$ENV_FILE" ]; then
  SK="$(grep -E '^CLERK_LIVE_SECRET_KEY=' "$ENV_FILE" 2>/dev/null | head -1 | cut -d= -f2- || true)"
  [ -z "$SK" ] && SK="$(grep -E '^CLERK_SECRET_KEY=' "$ENV_FILE" 2>/dev/null | head -1 | cut -d= -f2- || true)"
fi
[ -z "$SK" ] && { echo "FATAL: no CLERK secret (CLERK_LIVE_SECRET_KEY / CLERK_SECRET_KEY)"; exit 2; }

CREATED_USERS=()
cleanup() {
  for u in "${CREATED_USERS[@]:-}"; do
    [ -n "$u" ] && curl -s -o /dev/null -X DELETE "https://api.clerk.com/v1/users/$u" \
      -H "Authorization: Bearer $SK" --max-time 20 && echo "  DSR-deleted test user $u" || true
  done
}
trap cleanup EXIT

# bootstrap_user <label> → echoes "<clerk_user_id>|<pat>"; provisions via real signup.
bootstrap_user() {
  local label="$1" ts email resp cu i u pat
  ts="$(date +%s%N 2>/dev/null || date +%s)"
  email="corelink-e2e-suite-${label}-${ts}@humangr.com"
  resp="$(curl -s -X POST 'https://api.clerk.com/v1/users' -H "Authorization: Bearer $SK" \
    -H 'Content-Type: application/json' --max-time 25 \
    -d "{\"email_address\":[\"$email\"],\"password\":\"Corelink-${ts}-Xq9\",\"skip_password_checks\":true,\"public_metadata\":{\"e2e_test\":true}}")"
  cu="$(printf '%s' "$resp" | python3 -c 'import sys,json;print(json.load(sys.stdin).get("id") or "")' 2>/dev/null || true)"
  [ -z "$cu" ] && { echo "FATAL: clerk create $label failed" >&2; return 1; }
  for _ in $(seq 1 20); do
    sleep 5
    u="$(curl -s "https://api.clerk.com/v1/users/$cu" -H "Authorization: Bearer $SK" --max-time 15)"
    pat="$(printf '%s' "$u" | python3 -c 'import sys,json;pm=json.load(sys.stdin).get("private_metadata") or {};print(pm.get("pat_plaintext") or "")' 2>/dev/null || true)"
    [ -n "$pat" ] && { echo "${cu}|${pat}"; return 0; }
  done
  echo "FATAL: $label provisioning timed out" >&2; return 1
}

# tenant_of <pat> → the tenant id via the real identity endpoint.
tenant_of() { curl -s "$API/v1/users/me" -H "authorization: Bearer $1" --max-time 15 \
  | python3 -c 'import sys,json;print(json.load(sys.stdin).get("tenant_id") or "")' 2>/dev/null || true; }

# customer_key <pat> <name> <scopes-json> → echoes "<token>|<pat_id>" (real keys.create path).
customer_key() {
  local r
  r="$(curl -s -X POST "$API/v1/customer/keys" -H "authorization: Bearer $1" -H 'content-type: application/json' \
    --max-time 15 -d "{\"name\":\"$2\",\"scopes\":$3}")"
  printf '%s' "$r" | python3 -c 'import sys,json;d=json.load(sys.stdin);print((d.get("token") or "")+"|"+((d.get("pat") or {}).get("pat_id") or ""))' 2>/dev/null || true
}

# env_or_empty <VAR> → echoes the env value, or empty (treats unset as empty).
env_or_empty() { eval "printf '%s' \"\${$1:-}\""; }

# ── self-mintable personas (the real signup + keys.create path) ───────────────
echo "==> provisioning personas the real way (Clerk signup + customer keys)"
A="$(bootstrap_user A)"; CREATED_USERS+=("${A%%|*}"); PA="${A#*|}"
B="$(bootstrap_user B)"; CREATED_USERS+=("${B%%|*}"); PB="${B#*|}"
TA="$(tenant_of "$PA")"; TB="$(tenant_of "$PB")"

# P2 read-only.
RO="$(customer_key "$PA" 'e2e-ro' '["cache:read"]')"; PAT_RO="${RO%%|*}"
# P3 admin — PREFER an operator-provided CORELINK_E2E_PAT_ADMIN (admin scope is
# NOT self-serve grantable: customer keys.create refuses it — see
# customer_d1.rs::map_requested_scopes). `provision-personas.py` operator-mints
# one via /_internal/pat/mint + a D1 seed; we use it when present. The
# keys.create attempt below is a best-effort fallback (it returns empty for the
# admin scope, so without the env var P3 simply gates — never a fake pass).
PAT_ADMIN="$(env_or_empty CORELINK_E2E_PAT_ADMIN)"
if [ -z "$PAT_ADMIN" ]; then
  ADM="$(customer_key "$PA" 'e2e-admin' '["cache:read","cache:write","admin"]')"; PAT_ADMIN="${ADM%%|*}"
fi
# P4 revoked — create then revoke.
RV="$(customer_key "$PA" 'e2e-rev' '["cache:read","cache:write"]')"; PAT_REV="${RV%%|*}"; REV_ID="${RV#*|}"
[ -n "$REV_ID" ] && curl -s -o /dev/null -X POST "$API/v1/customer/keys/$REV_ID/revoke" -H "authorization: Bearer $PA" --max-time 15
sleep 4

# ── owner-supplied personas (read from env/OOB; GATE cleanly when absent) ─────
# These need a D1 seed row a black-box script CANNOT write (tier / sub-state /
# runner cap / PAT expiry). We READ them from env and only forward what is set;
# an absent var simply means the suite GATES the journeys that need it (the Rust
# harness records a Gated row — never a silent skip, never a fake credential).
PAT_EXPIRED="$(env_or_empty CORELINK_E2E_PAT_EXPIRED)"
PAT_FREE="$(env_or_empty CORELINK_E2E_PAT_FREE)"
PAT_SOLO="$(env_or_empty CORELINK_E2E_PAT_SOLO)"
PAT_PRO="$(env_or_empty CORELINK_E2E_PAT_PRO)"
PAT_ENTERPRISE="$(env_or_empty CORELINK_E2E_PAT_ENTERPRISE)"
PAT_PASTDUE="$(env_or_empty CORELINK_E2E_PAT_PASTDUE)"
PAT_RUNNER="$(env_or_empty CORELINK_E2E_PAT_RUNNER)"
RUNNER_TENANT="$(env_or_empty CORELINK_E2E_RUNNER_TENANT)"
PUBLIC_HASH="$(env_or_empty CORELINK_E2E_PUBLIC_HASH)"
# Moat (M2): the shared-cache HIT journey addresses a public BREW BOTTLE PATH
# (the only black-box-provable cross-tenant `_public` dedup surface) — a bare
# content hash is not a routable brew path, so this is its own knob.
PUBLIC_BREW_PATH="$(env_or_empty CORELINK_E2E_PUBLIC_BREW_PATH)"
TEAM_INVITE_EMAIL="$(env_or_empty CORELINK_E2E_TEAM_INVITE_EMAIL)"

# Money (M4): a dedicated Stripe-TEST tenant path. The billing journeys read
# these directly from the process env (billing.rs); we forward whatever is set
# and gate the rest. The whole money path stays opt-in + fail-safe: with NONE of
# these set, the billing journeys GATE (recorded), never fail.
STRIPE_TEST="$(env_or_empty CORELINK_E2E_STRIPE_TEST)"
CLERK_SESSION="$(env_or_empty CORELINK_E2E_CLERK_SESSION)"
STRIPE_WEBHOOK_TEST="$(env_or_empty CORELINK_E2E_STRIPE_WEBHOOK_TEST)"
SIGNUP_WORKER_ENDPOINT="$(env_or_empty CORELINK_E2E_SIGNUP_WORKER_ENDPOINT)"
STRIPE_WEBHOOK_SECRET="$(env_or_empty CORELINK_E2E_STRIPE_WEBHOOK_SECRET)"
STRIPE_SUBSCRIPTION_ID="$(env_or_empty CORELINK_E2E_STRIPE_SUBSCRIPTION_ID)"
STRIPE_CUSTOMER_ID="$(env_or_empty CORELINK_E2E_STRIPE_CUSTOMER_ID)"
STRIPE_PRICE_ID="$(env_or_empty CORELINK_E2E_STRIPE_PRICE_ID)"

# ── provisioning summary (cred presence only — NEVER the secret values) ───────
seen() { [ -n "$1" ] && printf 'ok' || printf '—'; }
echo "  self-minted:  P1=${TA:-?} P6=${TB:-?} P2(ro)=$(seen "$PAT_RO") P3(admin)=$(seen "$PAT_ADMIN") P4(revoked)=$(seen "$PAT_REV")"
echo "  owner-supplied personas:"
echo "    P5 expired=$(seen "$PAT_EXPIRED")  P7 free=$(seen "$PAT_FREE")  P8 solo=$(seen "$PAT_SOLO")  P9 pro=$(seen "$PAT_PRO")  P10 ent=$(seen "$PAT_ENTERPRISE")  P11 pastdue=$(seen "$PAT_PASTDUE")"
echo "    runner pat=$(seen "$PAT_RUNNER") tenant=$(seen "$RUNNER_TENANT")  public-hash=$(seen "$PUBLIC_HASH")  team-invite=$(seen "$TEAM_INVITE_EMAIL")"
echo "  money (M4): stripe-test=$(seen "$STRIPE_TEST") clerk-session=$(seen "$CLERK_SESSION") webhook-sim=$(seen "$STRIPE_WEBHOOK_TEST") signup-worker=$(seen "$SIGNUP_WORKER_ENDPOINT") whsec=$(seen "$STRIPE_WEBHOOK_SECRET") sub=$(seen "$STRIPE_SUBSCRIPTION_ID") cust=$(seen "$STRIPE_CUSTOMER_ID")"

# ── the substantive-pass floor (M1) ───────────────────────────────────────────
# Default the floor to 40 so a provisioning regression that silently re-gates the
# suite (the "green by vacuum" the lead fixed) flips it RED. The operator can
# lower it for a partial run, but the committed default is a real floor.
MIN_PASS="${CORELINK_E2E_MIN_PASS:-40}"

# Optional MAX-GATED ceiling (the floor's dual): RED if MORE journeys gate than
# expected — catches a single load-bearing journey silently flipping Pass→Gated
# (a 5xx outage, an unbuilt feature, a dropped cred), which the floor alone misses.
# Unset by default because the gated count depends on how much this run
# provisions (a full-cred run gates ~13; a light run gates more). A CI job that
# pins its provisioning should export CORELINK_E2E_MAX_GATED=<expected> to arm it.
MAX_GATED="${CORELINK_E2E_MAX_GATED:-}"

echo "==> running the black-box journey suite vs ${API} (MIN_PASS=${MIN_PASS}${MAX_GATED:+ MAX_GATED=$MAX_GATED})"
cd "$REPO_ROOT"
CORELINK_E2E_ENDPOINT="$API" \
CORELINK_E2E_OCI_ENDPOINT="$OCI" \
CORELINK_E2E_MIN_PASS="$MIN_PASS" \
${MAX_GATED:+CORELINK_E2E_MAX_GATED="$MAX_GATED"} \
CORELINK_E2E_TENANT="$TA" CORELINK_E2E_PAT_RW="$PA" \
CORELINK_E2E_TENANT_B="$TB" CORELINK_E2E_PAT_TENANT_B="$PB" \
CORELINK_E2E_PAT_RO="$PAT_RO" CORELINK_E2E_PAT_ADMIN="$PAT_ADMIN" CORELINK_E2E_PAT_REVOKED="$PAT_REV" \
CORELINK_E2E_PAT_EXPIRED="$PAT_EXPIRED" \
CORELINK_E2E_PAT_FREE="$PAT_FREE" CORELINK_E2E_PAT_SOLO="$PAT_SOLO" \
CORELINK_E2E_PAT_PRO="$PAT_PRO" CORELINK_E2E_PAT_ENTERPRISE="$PAT_ENTERPRISE" \
CORELINK_E2E_PAT_PASTDUE="$PAT_PASTDUE" \
CORELINK_E2E_PAT_RUNNER="$PAT_RUNNER" CORELINK_E2E_RUNNER_TENANT="$RUNNER_TENANT" \
CORELINK_E2E_PUBLIC_HASH="$PUBLIC_HASH" CORELINK_E2E_PUBLIC_BREW_PATH="$PUBLIC_BREW_PATH" \
CORELINK_E2E_TEAM_INVITE_EMAIL="$TEAM_INVITE_EMAIL" \
CORELINK_E2E_STRIPE_TEST="$STRIPE_TEST" CORELINK_E2E_CLERK_SESSION="$CLERK_SESSION" \
CORELINK_E2E_STRIPE_WEBHOOK_TEST="$STRIPE_WEBHOOK_TEST" \
CORELINK_E2E_SIGNUP_WORKER_ENDPOINT="$SIGNUP_WORKER_ENDPOINT" \
CORELINK_E2E_STRIPE_WEBHOOK_SECRET="$STRIPE_WEBHOOK_SECRET" \
CORELINK_E2E_STRIPE_SUBSCRIPTION_ID="$STRIPE_SUBSCRIPTION_ID" \
CORELINK_E2E_STRIPE_CUSTOMER_ID="$STRIPE_CUSTOMER_ID" \
CORELINK_E2E_STRIPE_PRICE_ID="$STRIPE_PRICE_ID" \
  cargo run -q -p e2e-user-journeys
