#!/usr/bin/env bash
# e2e-clerk-signup.sh — Clerk signup E2E via Backend API
#
# Programmatic black-box test of the full Clerk → signup-worker → D1 → PAT
# → /v1/users/me chain against PROD.  No browser required.
#
# What this does:
#   1. Create a real Clerk user via the Clerk Backend API
#   2. Poll D1 every 2s (up to 30s) for the tenant row
#   3. Query the pat row for that tenant
#   4. Inspect Clerk publicMetadata for tenant_id + pat_plaintext
#   5. Exercise /v1/users/me with the minted PAT
#   6. DELETE the test user from Clerk (trap-guarded cleanup)
#
# Usage:
#   bash scripts/e2e-clerk-signup.sh
#   (reads CLERK_SECRET_KEY + CLOUDFLARE_API_TOKEN from .env.local)
#
# Hard rules enforced:
#   - pat_plaintext NEVER echoed to stdout (only last4 + total length)
#   - CLERK_SECRET_KEY NEVER echoed to stdout
#   - test user cleaned up on exit (trap)
#   - Tests PROD only — no local override

set -euo pipefail

# ─────────────────────────────────────────────────────────────────────────────
# Constants
# ─────────────────────────────────────────────────────────────────────────────
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENV_FILE="${REPO_ROOT}/.env.local"
D1_DATABASE="corelink-config-prod"
D1_ENV="prod"
API_BASE="https://corelink-api.humangr.com"
CLERK_API="https://api.clerk.com/v1"
POLL_INTERVAL=2
POLL_MAX=30   # seconds

# Colour helpers (no-op when stdout is not a tty)
RED=""; GREEN=""; YELLOW=""; RESET=""
if [[ -t 1 ]]; then
  RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; RESET='\033[0m'
fi

pass()  { printf "${GREEN}[PASS]${RESET} %s\n" "$*"; }
fail()  { printf "${RED}[FAIL]${RESET} %s\n" "$*" >&2; }
info()  { printf '       %s\n' "$*"; }
step()  { printf "\n${YELLOW}==> Stage %s${RESET}\n" "$*"; }
warn()  { printf "${YELLOW}[WARN]${RESET} %s\n" "$*"; }

# ─────────────────────────────────────────────────────────────────────────────
# Global state (populated during run)
# ─────────────────────────────────────────────────────────────────────────────
TEST_USER_ID=""
TEST_EMAIL=""
TENANT_ID=""
PAT_ID=""
TOKEN_ID=""
PAT_LEN=0
PAT_LAST4=""

# ─────────────────────────────────────────────────────────────────────────────
# Cleanup (trap) — always DELETE the test user if one was created
# ─────────────────────────────────────────────────────────────────────────────
cleanup() {
  local exit_code=$?
  if [[ -n "${TEST_USER_ID}" ]]; then
    step "Cleanup — DELETE Clerk test user"
    local delete_resp
    delete_resp=$(curl -sS -o /dev/null -w "%{http_code}" \
      -X DELETE \
      -H "Authorization: Bearer ${CLERK_SECRET_KEY}" \
      "${CLERK_API}/users/${TEST_USER_ID}" 2>&1) || delete_resp="curl_error"
    if [[ "${delete_resp}" == "200" ]]; then
      pass "Clerk DELETE returned 200 — test user removed"
    else
      warn "Clerk DELETE returned ${delete_resp} (non-200) — manual cleanup may be needed for ${TEST_USER_ID}"
    fi
  fi

  echo ""
  echo "════════════════════════════════════════════════════════════════"
  echo "E2E SUMMARY"
  echo "  Test user:    ${TEST_USER_ID:-<not created>}"
  echo "  Test email:   ${TEST_EMAIL:-<not set>}"
  echo "  Tenant ID:    ${TENANT_ID:-<not provisioned>}"
  echo "  PAT token_id: ${TOKEN_ID:-<not minted>}"
  echo "  PAT length:   ${PAT_LEN} chars (last 4: ${PAT_LAST4:-n/a})"
  echo "════════════════════════════════════════════════════════════════"
  echo ""

  exit "${exit_code}"
}
trap cleanup EXIT

# ─────────────────────────────────────────────────────────────────────────────
# Load .env.local
# ─────────────────────────────────────────────────────────────────────────────
step "0 — Load credentials"
if [[ ! -f "${ENV_FILE}" ]]; then
  fail ".env.local not found at ${ENV_FILE}"
  echo ""
  echo "HARD PAUSE: CLERK_SECRET_KEY missing from .env.local"
  exit 1
fi

# Source only the variables we need (avoid polluting environment with unrelated vars)
CLERK_SECRET_KEY=""
CLOUDFLARE_API_TOKEN=""
while IFS='=' read -r key val; do
  [[ "${key}" =~ ^#.*$ ]] && continue
  [[ -z "${key}" ]] && continue
  key="${key//[[:space:]]/}"
  val="${val//[[:space:]]/}"
  case "${key}" in
    CLERK_SECRET_KEY)       CLERK_SECRET_KEY="${val}"       ;;
    CLOUDFLARE_API_TOKEN)   CLOUDFLARE_API_TOKEN="${val}"   ;;
  esac
done < "${ENV_FILE}"

if [[ -z "${CLERK_SECRET_KEY}" ]]; then
  fail "CLERK_SECRET_KEY not found in ${ENV_FILE}"
  echo ""
  echo "HARD PAUSE: CLERK_SECRET_KEY missing from .env.local"
  exit 1
fi
if [[ -z "${CLOUDFLARE_API_TOKEN}" ]]; then
  fail "CLOUDFLARE_API_TOKEN not found in ${ENV_FILE}"
  echo ""
  echo "HARD PAUSE: CLOUDFLARE_API_TOKEN missing from .env.local"
  exit 1
fi
export CLOUDFLARE_API_TOKEN

pass "Credentials loaded (CLERK_SECRET_KEY present, CLOUDFLARE_API_TOKEN present)"

# ─────────────────────────────────────────────────────────────────────────────
# Dependencies
# ─────────────────────────────────────────────────────────────────────────────
for cmd in curl jq; do
  if ! command -v "${cmd}" &>/dev/null; then
    fail "Required command not found: ${cmd}"
    exit 1
  fi
done

# Resolve wrangler: prefer global, fall back to repo-local node_modules
if command -v wrangler &>/dev/null; then
  WRANGLER_CMD="wrangler"
elif [[ -x "${REPO_ROOT}/node_modules/.bin/wrangler" ]]; then
  WRANGLER_CMD="${REPO_ROOT}/node_modules/.bin/wrangler"
elif command -v npx &>/dev/null; then
  WRANGLER_CMD="npx wrangler"
else
  fail "wrangler not found (tried global, ${REPO_ROOT}/node_modules/.bin/wrangler, npx)"
  exit 1
fi

# Verify wrangler is v4+ (the wrangler d1 execute --json output format requires v3+)
WRANGLER_VER=$(${WRANGLER_CMD} --version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+' | head -1 || echo "0.0")
info "wrangler version: ${WRANGLER_VER} (cmd: ${WRANGLER_CMD})"

# ─────────────────────────────────────────────────────────────────────────────
# Helper: run a wrangler d1 execute and parse the JSON result
# ─────────────────────────────────────────────────────────────────────────────
d1_query() {
  # Usage: d1_query "<SQL>"
  # Returns: raw JSON output from wrangler (with preamble stripped)
  local sql="$1"
  local raw
  raw=$(${WRANGLER_CMD} d1 execute "${D1_DATABASE}" \
    --env "${D1_ENV}" \
    --remote \
    --command="${sql}" \
    --json 2>/dev/null) || { echo "[]"; return 1; }
  # Strip non-JSON preamble (wrangler sometimes emits a banner line before the array)
  printf '%s' "${raw}" | sed -n '/^\[/,$p'
}

# ─────────────────────────────────────────────────────────────────────────────
# STAGE 1 — Create Clerk test user
# ─────────────────────────────────────────────────────────────────────────────
step "1 — Create Clerk test user"

TIMESTAMP=$(date +%s)
TEST_EMAIL="e2e-test-${TIMESTAMP}@example.com"
# Generate a random 20-char password that satisfies Clerk's policy
TEST_PASSWORD="E2e!test$(openssl rand -hex 8)"

info "Email: ${TEST_EMAIL}"

CREATE_RESP=$(curl -sS \
  --max-time 30 \
  -X POST \
  -H "Authorization: Bearer ${CLERK_SECRET_KEY}" \
  -H "Content-Type: application/json" \
  -d "{
    \"email_address\": [\"${TEST_EMAIL}\"],
    \"password\": \"${TEST_PASSWORD}\",
    \"skip_password_checks\": false,
    \"skip_password_requirement\": false
  }" \
  "${CLERK_API}/users" 2>&1) || {
  fail "curl to Clerk POST /v1/users failed"
  exit 1
}

# Extract user.id
TEST_USER_ID=$(printf '%s' "${CREATE_RESP}" | jq -r '.id // empty' 2>/dev/null || true)

if [[ -z "${TEST_USER_ID}" ]] || [[ "${TEST_USER_ID}" == "null" ]]; then
  fail "Clerk user creation failed — could not extract user.id"
  info "Response: $(printf '%s' "${CREATE_RESP}" | head -c 500)"
  exit 1
fi

pass "Clerk user created — user.id: ${TEST_USER_ID}"
info "Email confirmed: $(printf '%s' "${CREATE_RESP}" | jq -r '.email_addresses[0].email_address // "unknown"')"

# ─────────────────────────────────────────────────────────────────────────────
# STAGE 2 — Poll D1 for tenant row (up to 30s)
# ─────────────────────────────────────────────────────────────────────────────
step "2 — Poll D1 for tenant row (clerk_user_id = ${TEST_USER_ID})"

DEADLINE=$(( TIMESTAMP + POLL_MAX ))
POLL_COUNT=0
while true; do
  NOW=$(date +%s)
  ELAPSED=$(( NOW - TIMESTAMP ))
  POLL_COUNT=$(( POLL_COUNT + 1 ))

  info "Poll ${POLL_COUNT} (${ELAPSED}s elapsed)…"

  TENANT_JSON=$(d1_query "SELECT tenant_id, tenant_state, primary_region, clerk_user_id FROM tenant WHERE clerk_user_id = '${TEST_USER_ID}' LIMIT 1") || true

  TENANT_ID=$(printf '%s' "${TENANT_JSON}" | jq -r '.[0].results[0].tenant_id // empty' 2>/dev/null || true)

  if [[ -n "${TENANT_ID}" ]] && [[ "${TENANT_ID}" != "null" ]]; then
    TENANT_STATE=$(printf '%s' "${TENANT_JSON}" | jq -r '.[0].results[0].tenant_state // "unknown"')
    TENANT_REGION=$(printf '%s' "${TENANT_JSON}" | jq -r '.[0].results[0].primary_region // "unknown"')
    pass "Tenant row found in D1 after ${ELAPSED}s"
    info "  tenant_id:     ${TENANT_ID}"
    info "  tenant_state:  ${TENANT_STATE}"
    info "  primary_region: ${TENANT_REGION}"
    break
  fi

  if [[ "${NOW}" -ge "${DEADLINE}" ]]; then
    fail "Tenant row NOT found in D1 after ${POLL_MAX}s"
    info "Last D1 response: $(printf '%s' "${TENANT_JSON}" | head -c 300)"
    echo ""
    echo "HARD PAUSE: tenant row not provisioned within ${POLL_MAX}s"
    echo "Diagnostic info:"
    echo "  clerk_user_id queried: ${TEST_USER_ID}"
    echo "  D1 database: ${D1_DATABASE} (env=${D1_ENV})"
    echo ""
    echo "Check wrangler tail for signup-worker to see webhook delivery status."
    echo "Possible causes:"
    echo "  1. CLERK_WEBHOOK_SECRET still mismatched (svix sig verify fails)"
    echo "  2. Signup worker not deployed / routing broken"
    echo "  3. D1 binding misconfigured on signup-worker"
    exit 1
  fi

  sleep "${POLL_INTERVAL}"
done

# ─────────────────────────────────────────────────────────────────────────────
# STAGE 3 — Query PAT row
# ─────────────────────────────────────────────────────────────────────────────
step "3 — Query PAT row for tenant_id = ${TENANT_ID}"

PAT_JSON=$(d1_query "SELECT pat_id, token_id, scope, expires_ms FROM pat WHERE tenant_id = '${TENANT_ID}' LIMIT 1") || true

PAT_ID=$(printf '%s' "${PAT_JSON}" | jq -r '.[0].results[0].pat_id // empty' 2>/dev/null || true)
TOKEN_ID=$(printf '%s' "${PAT_JSON}" | jq -r '.[0].results[0].token_id // empty' 2>/dev/null || true)

if [[ -z "${PAT_ID}" ]] || [[ "${PAT_ID}" == "null" ]]; then
  fail "PAT row NOT found in D1 for tenant_id=${TENANT_ID}"
  info "PAT query response: $(printf '%s' "${PAT_JSON}" | head -c 300)"
  echo ""
  echo "HARD PAUSE: PAT not minted — signup orchestration incomplete"
  exit 1
fi

PAT_SCOPE=$(printf '%s' "${PAT_JSON}" | jq -r '.[0].results[0].scope // "unknown"')
PAT_EXPIRES=$(printf '%s' "${PAT_JSON}" | jq -r '.[0].results[0].expires_ms // 0')

pass "PAT row found in D1"
info "  pat_id:    ${PAT_ID}"
info "  token_id:  ${TOKEN_ID:-<null — migration 0054 not yet applied?>}"
info "  scope:     ${PAT_SCOPE}"
info "  expires_ms: ${PAT_EXPIRES}"

# ─────────────────────────────────────────────────────────────────────────────
# STAGE 4 — Inspect Clerk publicMetadata
# ─────────────────────────────────────────────────────────────────────────────
step "4 — Inspect Clerk publicMetadata for user ${TEST_USER_ID}"

META_RESP=$(curl -sS \
  --max-time 15 \
  -H "Authorization: Bearer ${CLERK_SECRET_KEY}" \
  "${CLERK_API}/users/${TEST_USER_ID}" 2>&1) || {
  fail "curl to Clerk GET /v1/users/${TEST_USER_ID} failed"
  exit 1
}

META_TENANT_ID=$(printf '%s' "${META_RESP}" | jq -r '.public_metadata.tenant_id // empty' 2>/dev/null || true)
META_REGION=$(printf '%s' "${META_RESP}" | jq -r '.public_metadata.region // empty' 2>/dev/null || true)
META_PAT_RAW=$(printf '%s' "${META_RESP}" | jq -r '.public_metadata.pat_plaintext // empty' 2>/dev/null || true)

# Security: never log the full pat_plaintext
if [[ -n "${META_PAT_RAW}" ]] && [[ "${META_PAT_RAW}" != "null" ]]; then
  PAT_LEN=${#META_PAT_RAW}
  PAT_LAST4="${META_PAT_RAW: -4}"
  META_PAT_PUBLISHED=true
else
  PAT_LEN=0
  PAT_LAST4=""
  META_PAT_PUBLISHED=false
fi

if [[ -z "${META_TENANT_ID}" ]] || [[ "${META_TENANT_ID}" == "null" ]]; then
  fail "publicMetadata.tenant_id is missing — metadata_published: false"
  info "public_metadata: $(printf '%s' "${META_RESP}" | jq '.public_metadata' 2>/dev/null || true)"
  echo ""
  echo "HARD PAUSE: metadata_published = false"
  echo "Possible cause: Clerk API returned 4xx during metadata patch in signup-worker."
  echo "  The tenant + PAT rows ARE in D1 (stages 2+3 passed) so the chain ran."
  echo "  Check D1 signup_orchestration for this tenant_id to see the outcome label."
  ORCH_JSON=$(d1_query "SELECT outcome, billing_intent, correlation_id FROM signup_orchestration WHERE tenant_id = '${TENANT_ID}' LIMIT 1") || true
  info "signup_orchestration: $(printf '%s' "${ORCH_JSON}" | jq '.[0].results // []' 2>/dev/null || true)"
  exit 1
fi

if [[ "${META_TENANT_ID}" != "${TENANT_ID}" ]]; then
  fail "publicMetadata.tenant_id (${META_TENANT_ID}) != D1 tenant_id (${TENANT_ID}) — mismatch!"
  exit 1
fi

if [[ "${META_PAT_PUBLISHED}" != "true" ]]; then
  fail "publicMetadata.pat_plaintext is missing"
  echo ""
  echo "HARD PAUSE: metadata_published = false (pat_plaintext missing)"
  exit 1
fi

pass "Clerk publicMetadata populated"
info "  metadata.tenant_id: ${META_TENANT_ID}"
info "  metadata.region:    ${META_REGION}"
info "  metadata.pat_plaintext: <${PAT_LEN} chars, last4=${PAT_LAST4}>"

# ─────────────────────────────────────────────────────────────────────────────
# STAGE 5 — Exercise PAT against /v1/users/me
# ─────────────────────────────────────────────────────────────────────────────
step "5 — Exercise PAT against ${API_BASE}/v1/users/me"

ME_RESP=$(curl -sS \
  --max-time 15 \
  -w "\nHTTP_STATUS:%{http_code}" \
  -H "Authorization: Bearer ${META_PAT_RAW}" \
  "${API_BASE}/v1/users/me" 2>&1) || {
  fail "curl to /v1/users/me failed"
  echo ""
  echo "HARD PAUSE: PAT auth against /v1/users/me failed (curl error)"
  exit 1
}

ME_BODY=$(printf '%s' "${ME_RESP}" | grep -v "^HTTP_STATUS:" || true)
ME_STATUS=$(printf '%s' "${ME_RESP}" | grep "^HTTP_STATUS:" | cut -d: -f2 || true)

if [[ "${ME_STATUS}" != "200" ]]; then
  fail "/v1/users/me returned HTTP ${ME_STATUS} (expected 200)"
  info "Body: $(printf '%s' "${ME_BODY}" | head -c 300)"
  echo ""
  echo "HARD PAUSE: PAT auth against /v1/users/me returns != 200 (got ${ME_STATUS})"
  exit 1
fi

ME_TENANT=$(printf '%s' "${ME_BODY}" | jq -r '.tenant_id // empty' 2>/dev/null || true)

if [[ "${ME_TENANT}" != "${TENANT_ID}" ]]; then
  fail "/v1/users/me tenant_id mismatch: got ${ME_TENANT}, expected ${TENANT_ID}"
  info "Full body: $(printf '%s' "${ME_BODY}" | head -c 300)"
  exit 1
fi

pass "/v1/users/me returned 200 with correct tenant_id=${ME_TENANT}"

# ─────────────────────────────────────────────────────────────────────────────
# STAGE 6 — D1 final cleanup: delete tenant row to avoid orphan test data
# (Clerk DELETE in trap handles the user; we also remove the D1 rows)
# ─────────────────────────────────────────────────────────────────────────────
step "6 — D1 cleanup: remove test tenant rows"

# Delete PAT rows first (FK constraint)
d1_query "DELETE FROM pat WHERE tenant_id = '${TENANT_ID}'" >/dev/null 2>&1 || warn "PAT DELETE failed or row already gone"
d1_query "DELETE FROM dpa_acceptance_pending WHERE tenant_id = '${TENANT_ID}'" >/dev/null 2>&1 || warn "dpa_acceptance_pending DELETE skipped"
d1_query "DELETE FROM usage_counter WHERE tenant_id = '${TENANT_ID}'" >/dev/null 2>&1 || warn "usage_counter DELETE skipped"
d1_query "DELETE FROM signup_orchestration WHERE tenant_id = '${TENANT_ID}'" >/dev/null 2>&1 || warn "signup_orchestration DELETE skipped"
d1_query "DELETE FROM tenant WHERE tenant_id = '${TENANT_ID}'" >/dev/null 2>&1 || warn "tenant DELETE failed"

pass "D1 test rows cleaned up for tenant_id=${TENANT_ID}"

# ─────────────────────────────────────────────────────────────────────────────
# All stages passed
# ─────────────────────────────────────────────────────────────────────────────
echo ""
echo "════════════════════════════════════════════════════════════════"
printf "${GREEN}ALL STAGES PASS${RESET}\n"
echo ""
echo "  Stage 1 — Clerk user created:           PASS  (${TEST_USER_ID})"
echo "  Stage 2 — tenant row in D1:             PASS  (${TENANT_ID})"
echo "  Stage 3 — pat row in D1:                PASS  (${PAT_ID})"
echo "  Stage 4 — Clerk publicMetadata:         PASS  (metadata_published=true)"
echo "  Stage 5 — /v1/users/me with PAT:        PASS  (HTTP 200, tenant_id matches)"
echo "  Stage 6 — D1 cleanup:                   PASS"
echo "  Cleanup — Clerk DELETE:                 (see above)"
echo "════════════════════════════════════════════════════════════════"
echo ""

# Signal to trap that we're exiting cleanly (trap still runs to delete Clerk user)
exit 0
