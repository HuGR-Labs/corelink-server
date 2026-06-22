#!/usr/bin/env bash
# scripts/e2e-real-client/provision-and-run-suite.sh
#
# One-command go-live cert for the BLACK-BOX Rust journey suite
# (tests/e2e-user-journeys). It provisions the full persona set THE REAL WAY a
# customer would (Clerk signup → PAT; customer keys.create for read-only; a
# create→revoke for the revoked persona), exports the CORELINK_E2E_* contract,
# runs the suite, and DSR-deletes the test users it created.
#
# WHY: the Rust security/regression matrix GATES (correctly) without the persona
# tokens; an operator who provisions them WRONG (e.g. the wrong pat_id field) gets
# a false-RED. This script provisions them correctly + deterministically so the
# suite runs GREEN — the security half of the protection moat, repeatable.
#
# Real clients (docker/cargo/brew/native) are certified by the sibling run.sh;
# this is the HTTP black-box half (cross-tenant isolation, revocation, OCI 9-fix
# regressions, adapter regressions, money/compliance journeys).
#
# Usage:  bash scripts/e2e-real-client/provision-and-run-suite.sh
# Env:    CLERK_LIVE_SECRET_KEY|CLERK_SECRET_KEY (from env or .env.local)
#         CORELINK_E2E_ENDPOINT     (default https://corelink-api.humangr.com)
#         CORELINK_E2E_OCI_ENDPOINT (default https://corelink-oci.humangr.com)
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
  for i in $(seq 1 20); do
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

echo "==> provisioning personas the real way (Clerk signup + customer keys)"
A="$(bootstrap_user A)"; CREATED_USERS+=("${A%%|*}"); PA="${A#*|}"
B="$(bootstrap_user B)"; CREATED_USERS+=("${B%%|*}"); PB="${B#*|}"
TA="$(tenant_of "$PA")"; TB="$(tenant_of "$PB")"
RO="$(customer_key "$PA" 'e2e-ro' '["cache:read"]')"; PAT_RO="${RO%%|*}"
RV="$(customer_key "$PA" 'e2e-rev' '["cache:read","cache:write"]')"; PAT_REV="${RV%%|*}"; REV_ID="${RV#*|}"
[ -n "$REV_ID" ] && curl -s -o /dev/null -X POST "$API/v1/customer/keys/$REV_ID/revoke" -H "authorization: Bearer $PA" --max-time 15
sleep 4
echo "  P1=${TA:-?} P6=${TB:-?} P2(ro)=${PAT_RO:+ok} P4(revoked)=${PAT_REV:+ok}"

echo "==> running the black-box journey suite vs ${API}"
cd "$REPO_ROOT"
CORELINK_E2E_ENDPOINT="$API" \
CORELINK_E2E_OCI_ENDPOINT="$OCI" \
CORELINK_E2E_TENANT="$TA" CORELINK_E2E_PAT_RW="$PA" \
CORELINK_E2E_TENANT_B="$TB" CORELINK_E2E_PAT_TENANT_B="$PB" \
CORELINK_E2E_PAT_RO="$PAT_RO" CORELINK_E2E_PAT_REVOKED="$PAT_REV" \
  cargo run -q -p e2e-user-journeys
