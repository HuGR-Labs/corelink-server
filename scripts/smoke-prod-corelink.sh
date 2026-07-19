#!/usr/bin/env bash
# smoke-prod-corelink.sh — Wave 32 Phase H PREP
#
# End-to-end smoke test for the full CoreLink production stack.
# Covers 5 check areas: Worker+DO+Container, Pages, DNS, Audit chain, Status page.
#
# Usage:
#   bash scripts/smoke-prod-corelink.sh              # Live smoke
#   bash scripts/smoke-prod-corelink.sh --dry-run    # Print what would be checked; no network calls
#   bash scripts/smoke-prod-corelink.sh --help
#
# Exit code: number of failures (0 = all green)
#
# CTRL-CRED-001: No credentials in this script. Anonymous probes only.
# Authed requests (audit chain check) read CORELINK_SMOKE_TOKEN from env.
# If the token is absent, the audit chain check is skipped (WARN, not FAIL).
#
# Phase H APPLY prerequisite:
#   - Phase E (container deploy) complete
#   - Phase F (pages deploy) complete
#   - Phase G (DNS production) complete
#   See: specs/_audits/sealed/2026-05-26-w32-phaseH-prep.md

set -euo pipefail

# ──────────────────────────────────────────────
# Constants
# ──────────────────────────────────────────────
API_BASE="https://corelink-api.humangr.com"
DOCS_URL="https://corelink-docs.humangr.com"
APP_URL="https://corelink-app.humangr.com"
STATUS_URL="https://status.humangr.com"
STATUS_CORELINK_URL="https://status.corelink.humangr.com"

# DNS rows — flat-rename scheme (Wave 32 Phase H APPLY patch)
# 4 deleted hosts (acme-dev, sandbox, go, staging) removed — surface reduction per flat-rename.
# status.corelink.humangr.com retained as DNS-only Phase A record (BetterStack).
declare -A DNS_PLAN
DNS_PLAN["corelink-api.humangr.com"]="corelink-prod.gustavoschneiter.workers.dev"
DNS_PLAN["corelink-app.humangr.com"]="corelink-admin-ui.pages.dev"
DNS_PLAN["corelink-docs.humangr.com"]="corelink-docs.pages.dev"
DNS_PLAN["corelink-signup.humangr.com"]="corelink-prod.gustavoschneiter.workers.dev"
DNS_PLAN["humangr.com"]="corelink-prod.gustavoschneiter.workers.dev"
DNS_PLAN["status.corelink.humangr.com"]="hugrl.betteruptime.com"

# Ordered list for deterministic output (6 active records; 4 deleted hosts removed)
DNS_NAMES=(
  "corelink-api.humangr.com"
  "corelink-app.humangr.com"
  "corelink-docs.humangr.com"
  "corelink-signup.humangr.com"
  "humangr.com"
  "status.corelink.humangr.com"
)

TIMEOUT_CURL=15   # seconds per curl call
TIMEOUT_DNS=5     # seconds per dig call
AUDIT_WAIT_SEC=2  # wait after authed request before querying audit chain

# ──────────────────────────────────────────────
# State
# ──────────────────────────────────────────────
FAIL_COUNT=0
DRY_RUN=false
LOG_FILE=""
SMOKE_START=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

# ──────────────────────────────────────────────
# Helpers
# ──────────────────────────────────────────────
log()  { printf '[%s] %s\n' "$(date -u +"%H:%M:%SZ")" "$*"; }
pass() { printf '[PASS] %s\n' "$*"; }
fail() { printf '[FAIL] %s\n' "$*" >&2; FAIL_COUNT=$(( FAIL_COUNT + 1 )); }
warn() { printf '[WARN] %s\n' "$*"; }
step() { printf '\n── %s ──\n' "$*"; }

usage() {
  cat <<EOF
smoke-prod-corelink.sh — Wave 32 Phase H end-to-end smoke test

Usage:
  bash scripts/smoke-prod-corelink.sh [OPTIONS]

Options:
  --dry-run    Print check inventory; skip all network calls
  --log FILE   Append results to FILE (markdown format)
  --help       Show this help

Environment:
  CORELINK_SMOKE_TOKEN   Bearer token for authed audit-chain check (optional).
                         If absent, audit chain check is skipped with WARN.

Exit code: number of failures (0 = all green).

Check areas:
  (a) Worker + DO + Container  — health, _health, CAS blob POST/GET
  (b) Pages deploys            — docs, app, TLS chain
  (c) DNS resolution           — 9 plan rows via dig
  (d) Audit chain              — authed request → audit row present + hash valid
  (e) Status page              — HTTP 200 reachable

EOF
  exit 0
}

# now_ms — wall-clock ms via python3.
# BSD `date` (macOS default) does NOT support `%3N`; it leaves "3N" as a
# literal suffix, producing e.g. "17799864333N" and breaking the
# arithmetic in time_curl. python3 is in PATH on every supported runner
# (CI + local dev) and gives reliable millisecond precision.
now_ms() { python3 -c 'import time; print(int(time.time()*1000))'; }

# time_curl NAME URL [EXTRA_CURL_ARGS...]
# Returns HTTP status code; prints timing.
time_curl() {
  local name="$1" url="$2"
  shift 2
  local t0 http_code elapsed
  t0=$(now_ms)
  http_code=$(curl -s -o /dev/null -w "%{http_code}" \
    --max-time "${TIMEOUT_CURL}" \
    --connect-timeout 10 \
    "$@" "$url" 2>/dev/null || true)
  elapsed=$(( $(now_ms) - t0 ))
  printf '  %s: HTTP %s (%dms)\n' "$name" "$http_code" "$elapsed"
  echo "$http_code"
}

tls_verify() {
  local host="$1"
  # Returns 0 if cert covers *.humangr.com or the exact host
  openssl s_client -connect "${host}:443" -servername "$host" </dev/null 2>/dev/null \
    | openssl x509 -noout -subject -issuer -dates 2>/dev/null
}

append_log() {
  if [[ -n "$LOG_FILE" ]]; then
    echo "$*" >> "$LOG_FILE"
  fi
}

# ──────────────────────────────────────────────
# Argument parsing
# ──────────────────────────────────────────────
while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run) DRY_RUN=true ;;
    --log)     LOG_FILE="$2"; shift ;;
    --help|-h) usage ;;
    *) echo "Unknown option: $1" >&2; exit 2 ;;
  esac
  shift
done

# ──────────────────────────────────────────────
# Dry-run output
# ──────────────────────────────────────────────
if $DRY_RUN; then
  cat <<EOF
smoke-prod-corelink.sh — DRY-RUN (no network calls)
Date: ${SMOKE_START}

Check inventory:

(a) Worker + DO + Container
  [1] GET  ${API_BASE}/health             → expect 200 + JSON body with status==ok (extra fields ignored)
  [2] GET  ${API_BASE}/_health            → expect 200 + content-type application/json
  [3] PUT  ${API_BASE}/v1/cas/<tenant>/<sha256>  → CAS blob upload; 201 when CORELINK_SMOKE_TOKEN set, WARN-skipped otherwise
  [4] GET  ${API_BASE}/v1/cas/<tenant>/<sha256>  → blob fetch, byte-for-byte match; WARN-skipped without token

(b) Pages deploys
  [5] GET  ${DOCS_URL}                    → expect 200 + HTML body
  [6] GET  ${APP_URL}                     → expect 200 + non-empty body (content-type not asserted; Clerk widget is dynamic-imported)
  [7] TLS  corelink-docs.humangr.com      → cert subject covers *.humangr.com
  [8] TLS  corelink-app.humangr.com       → cert subject covers *.humangr.com

(c) DNS resolution (flat-rename — 5 active + 1 Phase A NO-OP; 4 deleted hosts removed)
  [9]  dig corelink-api.humangr.com       → corelink-prod.gustavoschneiter.workers.dev
  [10] dig corelink-app.humangr.com       → corelink-admin-ui.pages.dev
  [11] dig corelink-docs.humangr.com      → corelink-docs.pages.dev
  [12] dig corelink-signup.humangr.com    → corelink-prod.gustavoschneiter.workers.dev
  [13] dig humangr.com     → corelink-prod.gustavoschneiter.workers.dev
  [14] dig status.corelink.humangr.com    → hugrl.betteruptime.com (NO-OP — Phase A)
  (acme-dev, sandbox, go, staging: DELETED in flat-rename surface reduction)

(d) Audit chain end-to-end
  [15] POST ${API_BASE}/v1/audit/probe    → authed request (CORELINK_SMOKE_TOKEN)
  [16] wait ${AUDIT_WAIT_SEC}s then GET  ${API_BASE}/v1/audit/chain?request_id=<id>
       → row present + chain_hash valid (non-empty hex string)

(e) Status page
  [17] GET  ${STATUS_URL}                 → expect 200
  [18] GET  ${STATUS_CORELINK_URL}        → KNOWN EXCEPTION: 000 (BetterStack TLS not provisioned; operator must Enable SSL in BetterStack console for page 247652)

Total active checks: 18 (down from 22; 4 deleted hosts removed)
Exit code will equal number of failures.
EOF
  exit 0
fi

# ──────────────────────────────────────────────
# Prerequisite checks
# ──────────────────────────────────────────────
log "smoke-prod-corelink.sh starting — ${SMOKE_START}"

if ! command -v curl &>/dev/null; then
  echo "ERROR: curl not found in PATH" >&2
  exit 1
fi
if ! command -v dig &>/dev/null; then
  echo "ERROR: dig not found in PATH. Install bind-tools or dnsutils." >&2
  exit 1
fi

# Hard pause trigger 1: dns-prod-plan.sh must be present (Phase G prerequisite)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [[ ! -f "${SCRIPT_DIR}/dns-prod-plan.sh" ]]; then
  echo "HARD PAUSE TRIGGER 1: scripts/dns-prod-plan.sh not found." >&2
  echo "Phase G prerequisite missing. Cannot verify DNS smoke." >&2
  exit 1
fi

# Hard pause trigger 2: wrangler version check (warn only in smoke; hard fail in cutover)
if ! command -v wrangler &>/dev/null; then
  warn "wrangler CLI not in PATH. CAS blob smoke requires wrangler context."
  warn "HARD PAUSE TRIGGER 2: wrangler missing. Cutover script will hard-fail."
fi

append_log "## Smoke run: ${SMOKE_START}"
append_log ""

# ──────────────────────────────────────────────
# (a) Worker + DO + Container
# ──────────────────────────────────────────────
step "Area (a): Worker + DO + Container"

# [1] /health — superset match: status==ok + content-type application/json; extra fields (e.g. env:prod) ignored.
log "CHECK [1] GET ${API_BASE}/health"
HEALTH_CODE=$(curl -s -o /dev/null -w "%{http_code}" \
  --max-time "${TIMEOUT_CURL}" "${API_BASE}/health" 2>/dev/null || true)
BODY=$(curl -sS --max-time "${TIMEOUT_CURL}" "${API_BASE}/health" 2>/dev/null || echo "")
HDR_CT_1=$(curl -sS -D - -o /dev/null \
  --max-time "${TIMEOUT_CURL}" "${API_BASE}/health" 2>/dev/null \
  | grep -i "^content-type:" | head -1 || echo "")
printf '  health: HTTP %s\n' "$HEALTH_CODE"
if [[ "$HEALTH_CODE" == "200" ]] \
   && echo "$BODY" | grep -q '"status"' \
   && echo "$BODY" | grep -q '"ok"' \
   && echo "$HDR_CT_1" | grep -qi "application/json"; then
  pass "[1] /health → 200 + JSON + status:ok (extra fields ignored)"
  append_log "- [PASS] [1] /health → 200"
else
  fail "[1] /health → expected 200 + application/json + {\"status\":\"ok\",...}; got HTTP=${HEALTH_CODE} ct=${HDR_CT_1} body=${BODY}"
  append_log "- [FAIL] [1] /health → HTTP=${HEALTH_CODE}"
fi

# [2] /_health
log "CHECK [2] GET ${API_BASE}/_health"
HDR_CODE=$(curl -s -o /dev/null -w "%{http_code}" \
  --max-time "${TIMEOUT_CURL}" "${API_BASE}/_health" 2>/dev/null || true)
HDR_CT=$(curl -sS -D - -o /dev/null \
  --max-time "${TIMEOUT_CURL}" "${API_BASE}/_health" 2>/dev/null \
  | grep -i "^content-type:" | head -1 || echo "")
if [[ "$HDR_CODE" == "200" ]] && echo "$HDR_CT" | grep -qi "application/json"; then
  pass "[2] /_health → 200 + content-type: application/json"
  append_log "- [PASS] [2] /_health → 200 + JSON"
else
  fail "[2] /_health → HTTP=${HDR_CODE}, content-type=${HDR_CT}"
  append_log "- [FAIL] [2] /_health → HTTP=${HDR_CODE}"
fi

# [3] CAS blob PUT — real route: PUT /v1/cas/<tenant>/<sha256>
# Requires CORELINK_SMOKE_TOKEN; WARN-skipped when absent.
# Route /v2/cas/upload never existed in production (was a stale spec artifact).
log "CHECK [3] PUT CAS blob via /v1/cas/<tenant>/<sha256>"
BLOB_CONTENT="smoke-test-$(date +%s)-corelink"
BLOB_SHA=$(printf '%s' "$BLOB_CONTENT" | shasum -a 256 | awk '{print $1}')
SMOKE_TENANT="${CORELINK_SMOKE_TENANT:-3f9d662a-f879-451c-8270-243207b161ef}"
CAS_PUT_SKIP=false
if [[ -z "${CORELINK_SMOKE_TOKEN:-}" ]]; then
  warn "[3] CAS blob PUT skipped — CORELINK_SMOKE_TOKEN not set"
  append_log "- [SKIP] [3] CAS blob PUT — CORELINK_SMOKE_TOKEN not set"
  CAS_PUT_SKIP=true
  BLOB_SHA=""
else
  CAS_CODE=$(curl -s -o /dev/null -w "%{http_code}" \
    --max-time "${TIMEOUT_CURL}" \
    -X PUT \
    -H "Authorization: Bearer ${CORELINK_SMOKE_TOKEN}" \
    -H "Content-Type: application/octet-stream" \
    --data-binary "$BLOB_CONTENT" \
    "${API_BASE}/v1/cas/${SMOKE_TENANT}/${BLOB_SHA}" 2>/dev/null || true)
  if [[ "$CAS_CODE" == "201" ]]; then
    pass "[3] CAS blob PUT → 201, digest ${BLOB_SHA:0:16}... stored"
    append_log "- [PASS] [3] CAS blob PUT → 201"
  else
    fail "[3] CAS blob PUT → HTTP=${CAS_CODE} (expected 201)"
    append_log "- [FAIL] [3] CAS blob PUT → HTTP=${CAS_CODE}"
    BLOB_SHA=""  # mark as failed so [4] is skipped gracefully
  fi
fi

# [4] CAS blob GET — real route: GET /v1/cas/<tenant>/<sha256>
log "CHECK [4] GET CAS blob back"
if $CAS_PUT_SKIP; then
  warn "[4] CAS blob GET skipped — CORELINK_SMOKE_TOKEN not set"
  append_log "- [SKIP] [4] CAS blob GET — CORELINK_SMOKE_TOKEN not set"
elif [[ -n "$BLOB_SHA" ]]; then
  GET_CODE=$(curl -s -o /dev/null -w "%{http_code}" \
    --max-time "${TIMEOUT_CURL}" \
    -H "Authorization: Bearer ${CORELINK_SMOKE_TOKEN}" \
    "${API_BASE}/v1/cas/${SMOKE_TENANT}/${BLOB_SHA}" 2>/dev/null || true)
  GET_BODY=$(curl -s \
    --max-time "${TIMEOUT_CURL}" \
    -H "Authorization: Bearer ${CORELINK_SMOKE_TOKEN}" \
    "${API_BASE}/v1/cas/${SMOKE_TENANT}/${BLOB_SHA}" 2>/dev/null || echo "")
  if [[ "$GET_CODE" == "200" ]] && [[ "$GET_BODY" == "$BLOB_CONTENT" ]]; then
    pass "[4] CAS blob GET → 200, bytes match"
    append_log "- [PASS] [4] CAS blob GET → 200 + bytes match"
  else
    fail "[4] CAS blob GET → HTTP=${GET_CODE} (expected 200 + exact bytes)"
    append_log "- [FAIL] [4] CAS blob GET → HTTP=${GET_CODE}"
  fi
else
  warn "[4] CAS blob GET skipped — blob PUT failed"
  append_log "- [SKIP] [4] CAS blob GET — PUT failed"
fi

# ──────────────────────────────────────────────
# (b) Pages deploys
# ──────────────────────────────────────────────
step "Area (b): Pages deploys"

# [5] docs
log "CHECK [5] GET ${DOCS_URL}"
DOCS_BODY=$(curl -sS --max-time "${TIMEOUT_CURL}" "${DOCS_URL}" 2>/dev/null || echo "")
DOCS_CODE=$(curl -s -o /dev/null -w "%{http_code}" --max-time "${TIMEOUT_CURL}" "${DOCS_URL}" 2>/dev/null || true)
if [[ "$DOCS_CODE" == "200" ]] && echo "$DOCS_BODY" | grep -qi "<html"; then
  pass "[5] ${DOCS_URL} → 200 + HTML"
  append_log "- [PASS] [5] docs page → 200 + HTML"
else
  fail "[5] ${DOCS_URL} → HTTP=${DOCS_CODE} (expected 200 + HTML)"
  append_log "- [FAIL] [5] docs page → HTTP=${DOCS_CODE}"
fi

# [6] app (admin-ui — post OpenNext migration)
# Clerk widget is dynamic-imported client-side (next/dynamic ssr:false); initial HTML will NOT contain
# "Clerk init" or similar strings. Check only: 200 + content-type text/html + non-empty body.
log "CHECK [6] GET ${APP_URL}"
APP_BODY=$(curl -sS --max-time "${TIMEOUT_CURL}" "${APP_URL}" 2>/dev/null || echo "")
APP_CODE=$(curl -s -o /dev/null -w "%{http_code}" --max-time "${TIMEOUT_CURL}" "${APP_URL}" 2>/dev/null || true)
APP_CT=$(curl -sS -D - -o /dev/null \
  --max-time "${TIMEOUT_CURL}" "${APP_URL}" 2>/dev/null \
  | grep -i "^content-type:" | head -1 || echo "")
# content-type may be text/html or text/plain depending on OpenNext serving mode;
# assert only: 200 + non-empty body.
if [[ "$APP_CODE" == "200" ]] && [[ -n "$APP_BODY" ]]; then
  pass "[6] ${APP_URL} → 200 + non-empty body (ct=${APP_CT})"
  append_log "- [PASS] [6] app page → 200 + non-empty body"
else
  fail "[6] ${APP_URL} → HTTP=${APP_CODE} ct=${APP_CT} (expected 200 + non-empty body)"
  append_log "- [FAIL] [6] app page → HTTP=${APP_CODE}"
fi

# [7] TLS chain — docs
log "CHECK [7] TLS chain: corelink-docs.humangr.com"
TLS_DOCS=$(tls_verify "corelink-docs.humangr.com")
if echo "$TLS_DOCS" | grep -q "humangr.com"; then
  pass "[7] TLS corelink-docs.humangr.com — cert covers humangr.com"
  append_log "- [PASS] [7] TLS corelink-docs.humangr.com"
else
  fail "[7] TLS corelink-docs.humangr.com — cert does not cover humangr.com or check failed"
  append_log "- [FAIL] [7] TLS corelink-docs.humangr.com"
fi

# [8] TLS chain — app
log "CHECK [8] TLS chain: corelink-app.humangr.com"
TLS_APP=$(tls_verify "corelink-app.humangr.com")
if echo "$TLS_APP" | grep -q "humangr.com"; then
  pass "[8] TLS corelink-app.humangr.com — cert covers humangr.com"
  append_log "- [PASS] [8] TLS corelink-app.humangr.com"
else
  fail "[8] TLS corelink-app.humangr.com — cert does not cover humangr.com or check failed"
  append_log "- [FAIL] [8] TLS corelink-app.humangr.com"
fi

# ──────────────────────────────────────────────
# (c) DNS resolution (Phase G plan)
# ──────────────────────────────────────────────
step "Area (c): DNS resolution (Phase G — 9 rows + 1 NO-OP)"

CHECK_NUM=9
for NAME in "${DNS_NAMES[@]}"; do
  EXPECTED="${DNS_PLAN[$NAME]}"
  log "CHECK [${CHECK_NUM}] dig +short ${NAME}"
  RESOLVED=$(dig +short +time="${TIMEOUT_DNS}" "$NAME" 2>/dev/null | tail -1 || echo "NXDOMAIN")
  if [[ -z "$RESOLVED" ]]; then
    RESOLVED="NXDOMAIN"
  fi
  # For proxied records, CF will return CNAME -> CF IPs (not the workers.dev target directly).
  # Accept: exact CNAME match OR non-empty resolution (CF proxied records resolve to CF IPs).
  # The key invariant is that the name resolves (not NXDOMAIN).
  if [[ "$RESOLVED" == "NXDOMAIN" ]]; then
    fail "[${CHECK_NUM}] dig ${NAME} → NXDOMAIN (expected to resolve toward ${EXPECTED})"
    append_log "- [FAIL] [${CHECK_NUM}] DNS ${NAME} → NXDOMAIN"
  else
    # Extra check: for DNS-only record (status), must resolve via hugrl.betteruptime.com CNAME.
    # Use `dig CNAME +short` because `dig +short | tail -1` returns the final resolved IP
    # (CF IPs from BetterStack's CDN), not the CNAME itself.  Ticket: smoke-check-14-cname-fix.
    if [[ "$NAME" == "status.corelink.humangr.com" ]]; then
      CNAME_TARGET=$(dig CNAME +short +time="${TIMEOUT_DNS}" "$NAME" 2>/dev/null | head -1 || echo "")
      if echo "$CNAME_TARGET" | grep -q "betteruptime"; then
        pass "[${CHECK_NUM}] dig CNAME ${NAME} → ${CNAME_TARGET} (DNS-only, BetterUptime)"
        append_log "- [PASS] [${CHECK_NUM}] DNS ${NAME} → BetterUptime CNAME (${CNAME_TARGET})"
      else
        fail "[${CHECK_NUM}] dig CNAME ${NAME} → '${CNAME_TARGET}' (expected betteruptime CNAME; resolved=${RESOLVED})"
        append_log "- [FAIL] [${CHECK_NUM}] DNS ${NAME} → CNAME='${CNAME_TARGET}' resolved=${RESOLVED}"
      fi
    else
      pass "[${CHECK_NUM}] dig ${NAME} → ${RESOLVED} (resolved OK)"
      append_log "- [PASS] [${CHECK_NUM}] DNS ${NAME} → ${RESOLVED}"
    fi
  fi
  CHECK_NUM=$(( CHECK_NUM + 1 ))
done

# ──────────────────────────────────────────────
# (d) Audit chain end-to-end
# ──────────────────────────────────────────────
step "Area (d): Audit chain end-to-end"

if [[ -z "${CORELINK_SMOKE_TOKEN:-}" ]]; then
  warn "[19-20] CORELINK_SMOKE_TOKEN not set — audit chain check SKIPPED"
  warn "        Set CORELINK_SMOKE_TOKEN=<bearer> to enable this check"
  append_log "- [SKIP] [19] audit probe — CORELINK_SMOKE_TOKEN not set"
  append_log "- [SKIP] [20] audit chain query — CORELINK_SMOKE_TOKEN not set"
else
  # [19] Issue authed request to produce audit row
  log "CHECK [19] POST ${API_BASE}/v1/audit/probe (authed)"
  PROBE_RESP=$(curl -sS -X POST \
    --max-time "${TIMEOUT_CURL}" \
    -H "Authorization: Bearer ${CORELINK_SMOKE_TOKEN}" \
    -H "Content-Type: application/json" \
    -d '{"action":"smoke-probe","resource":"smoke-test"}' \
    "${API_BASE}/v1/audit/probe" 2>/dev/null || echo "")
  PROBE_CODE=$(curl -s -o /dev/null -w "%{http_code}" -X POST \
    --max-time "${TIMEOUT_CURL}" \
    -H "Authorization: Bearer ${CORELINK_SMOKE_TOKEN}" \
    -H "Content-Type: application/json" \
    -d '{"action":"smoke-probe","resource":"smoke-test"}' \
    "${API_BASE}/v1/audit/probe" 2>/dev/null || true)
  REQUEST_ID=$(echo "$PROBE_RESP" | grep -o '"request_id":"[^"]*"' | cut -d'"' -f4 || echo "")
  if [[ "$PROBE_CODE" == "200" ]] && [[ -n "$REQUEST_ID" ]]; then
    pass "[19] audit probe → 200, request_id=${REQUEST_ID}"
    append_log "- [PASS] [19] audit probe → 200 request_id=${REQUEST_ID}"
    # [20] Wait and query audit chain
    log "Waiting ${AUDIT_WAIT_SEC}s for audit row propagation..."
    sleep "${AUDIT_WAIT_SEC}"
    log "CHECK [20] GET ${API_BASE}/v1/audit/chain?request_id=${REQUEST_ID}"
    CHAIN_RESP=$(curl -sS \
      --max-time "${TIMEOUT_CURL}" \
      -H "Authorization: Bearer ${CORELINK_SMOKE_TOKEN}" \
      "${API_BASE}/v1/audit/chain?request_id=${REQUEST_ID}" 2>/dev/null || echo "")
    CHAIN_CODE=$(curl -s -o /dev/null -w "%{http_code}" \
      --max-time "${TIMEOUT_CURL}" \
      -H "Authorization: Bearer ${CORELINK_SMOKE_TOKEN}" \
      "${API_BASE}/v1/audit/chain?request_id=${REQUEST_ID}" 2>/dev/null || true)
    # Expect row present (request_id in response) + chain_hash is a non-empty hex string
    CHAIN_HASH=$(echo "$CHAIN_RESP" | grep -o '"chain_hash":"[^"]*"' | cut -d'"' -f4 || echo "")
    if [[ "$CHAIN_CODE" == "200" ]] \
       && echo "$CHAIN_RESP" | grep -q "$REQUEST_ID" \
       && [[ -n "$CHAIN_HASH" ]] \
       && echo "$CHAIN_HASH" | grep -qE '^[0-9a-f]{16,}$'; then
      pass "[20] audit chain → 200, row present, chain_hash=${CHAIN_HASH:0:16}..."
      append_log "- [PASS] [20] audit chain → 200 + hash valid"
    else
      fail "[20] audit chain → HTTP=${CHAIN_CODE} (expected 200 + request_id row + valid chain_hash)"
      append_log "- [FAIL] [20] audit chain → HTTP=${CHAIN_CODE}"
    fi
  else
    fail "[19] audit probe → HTTP=${PROBE_CODE} (expected 200 + request_id)"
    append_log "- [FAIL] [19] audit probe → HTTP=${PROBE_CODE}"
    warn "[20] audit chain query skipped — probe failed"
    append_log "- [SKIP] [20] audit chain query — probe failed"
  fi
fi

# ──────────────────────────────────────────────
# (e) Status page
# ──────────────────────────────────────────────
step "Area (e): Status page"

# [21] status.humangr.com
log "CHECK [21] GET ${STATUS_URL}"
ST_CODE=$(curl -s -o /dev/null -w "%{http_code}" \
  --max-time "${TIMEOUT_CURL}" "${STATUS_URL}" 2>/dev/null || true)
printf '  status-main: HTTP %s\n' "$ST_CODE"
if [[ "$ST_CODE" == "200" ]]; then
  pass "[21] ${STATUS_URL} → 200"
  append_log "- [PASS] [21] ${STATUS_URL} → 200"
else
  fail "[21] ${STATUS_URL} → HTTP=${ST_CODE} (expected 200)"
  append_log "- [FAIL] [21] ${STATUS_URL} → HTTP=${ST_CODE}"
fi

# [22] status.corelink.humangr.com (Phase A custom domain)
# KNOWN EXCEPTION: returns 000 due to TLS cert not issued for this 2-level subdomain.
# Root cause (investigated 2026-05-30): BetterStack's CDN edge sees the domain (HTTP 80 → 301 works)
# but has no TLS cert for status.corelink.humangr.com. ACME HTTP-01 is blocked (403) so auto-
# provisioning never completes. BetterStack requires a manual "Enable SSL" click in their web
# console (Settings → Custom domain) to issue the cert via Cloudflare for Platforms.
# CF Universal SSL covers *.humangr.com only (1-level wildcard; Free plan — no Advanced certs).
# Operator action required: BetterStack console → page 247652 → Settings → Custom domain → Enable SSL.
# See: docs/operator/betterstack-state-2026-05-29.md § TLS Provisioning
# Once operator clicks "Enable SSL" and curl returns 200, remove this KNOWN EXCEPTION block and
# change the elif ["$SCL_CODE" == "000"] branch to: fail "[22] ... → 000 (expected 200)".
# Documented exception per Wave 32 Phase H APPLY audit §6. NOT a failure until cert is issued.
log "CHECK [22] GET ${STATUS_CORELINK_URL} (KNOWN EXCEPTION: 000 until operator enables SSL in BetterStack console)"
SCL_CODE=$(curl -s -o /dev/null -w "%{http_code}" \
  --max-time "${TIMEOUT_CURL}" "${STATUS_CORELINK_URL}" 2>/dev/null || true)
printf '  status-corelink: HTTP %s\n' "$SCL_CODE"
if [[ "$SCL_CODE" == "200" ]]; then
  pass "[22] ${STATUS_CORELINK_URL} → 200"
  append_log "- [PASS] [22] ${STATUS_CORELINK_URL} → 200"
elif [[ "$SCL_CODE" == "000" ]]; then
  warn "[22] ${STATUS_CORELINK_URL} → 000 (KNOWN EXCEPTION — BetterStack TLS not provisioned; operator must click 'Enable SSL' in BetterStack console for page 247652)"
  append_log "- [KNOWN-EXCEPTION] [22] ${STATUS_CORELINK_URL} → 000 (BetterStack SSL not activated; operator action required)"
else
  fail "[22] ${STATUS_CORELINK_URL} → HTTP=${SCL_CODE} (unexpected; expected 200 or 000)"
  append_log "- [FAIL] [22] ${STATUS_CORELINK_URL} → HTTP=${SCL_CODE}"
fi

# ──────────────────────────────────────────────
# Summary
# ──────────────────────────────────────────────
SMOKE_END=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
printf '\n══════════════════════════════════════════════\n'
printf 'SMOKE SUMMARY  start=%s  end=%s\n' "$SMOKE_START" "$SMOKE_END"
printf '  Failures: %d\n' "$FAIL_COUNT"
if [[ "$FAIL_COUNT" -eq 0 ]]; then
  printf '  Result:   ALL GREEN\n'
else
  printf '  Result:   %d FAILURE(S) — see [FAIL] lines above\n' "$FAIL_COUNT"
fi
printf '══════════════════════════════════════════════\n'

append_log ""
append_log "**Smoke result:** ${FAIL_COUNT} failure(s) — ${SMOKE_END}"
append_log ""

exit "${FAIL_COUNT}"
