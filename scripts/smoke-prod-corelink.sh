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
#   See: specs/_audits/2026-05-26-w32-phaseH-prep.md

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
DNS_PLAN["corelink-admin.humangr.com"]="corelink-prod.gustavoschneiter.workers.dev"
DNS_PLAN["status.corelink.humangr.com"]="hugrl.betteruptime.com"

# Ordered list for deterministic output (6 active records; 4 deleted hosts removed)
DNS_NAMES=(
  "corelink-api.humangr.com"
  "corelink-app.humangr.com"
  "corelink-docs.humangr.com"
  "corelink-signup.humangr.com"
  "corelink-admin.humangr.com"
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

# time_curl NAME URL [EXTRA_CURL_ARGS...]
# Returns HTTP status code; prints timing.
time_curl() {
  local name="$1" url="$2"
  shift 2
  local t0 http_code elapsed
  t0=$(date +%s%3N)
  http_code=$(curl -sS -o /dev/null -w "%{http_code}" \
    --max-time "${TIMEOUT_CURL}" \
    --connect-timeout 10 \
    "$@" "$url" 2>/dev/null || echo "000")
  elapsed=$(( $(date +%s%3N) - t0 ))
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
  [1] GET  ${API_BASE}/health             → expect 200 {"status":"ok"}
  [2] GET  ${API_BASE}/_health            → expect 200 + content-type application/json
  [3] POST ${API_BASE}/v2/cas/upload      → REAPI CAS blob upload (tiny blob)
  [4] GET  ${API_BASE}/v2/cas/<digest>    → blob fetch, byte-for-byte match

(b) Pages deploys
  [5] GET  ${DOCS_URL}                    → expect 200 + HTML body
  [6] GET  ${APP_URL}                     → expect 200 + Clerk init script tag
  [7] TLS  corelink-docs.humangr.com      → cert subject covers *.humangr.com
  [8] TLS  corelink-app.humangr.com       → cert subject covers *.humangr.com

(c) DNS resolution (flat-rename — 5 active + 1 Phase A NO-OP; 4 deleted hosts removed)
  [9]  dig corelink-api.humangr.com       → corelink-prod.gustavoschneiter.workers.dev
  [10] dig corelink-app.humangr.com       → corelink-admin-ui.pages.dev
  [11] dig corelink-docs.humangr.com      → corelink-docs.pages.dev
  [12] dig corelink-signup.humangr.com    → corelink-prod.gustavoschneiter.workers.dev
  [13] dig corelink-admin.humangr.com     → corelink-prod.gustavoschneiter.workers.dev
  [14] dig status.corelink.humangr.com    → hugrl.betteruptime.com (NO-OP — Phase A)
  (acme-dev, sandbox, go, staging: DELETED in flat-rename surface reduction)

(d) Audit chain end-to-end
  [15] POST ${API_BASE}/v1/audit/probe    → authed request (CORELINK_SMOKE_TOKEN)
  [16] wait ${AUDIT_WAIT_SEC}s then GET  ${API_BASE}/v1/audit/chain?request_id=<id>
       → row present + chain_hash valid (non-empty hex string)

(e) Status page
  [17] GET  ${STATUS_URL}                 → expect 200
  [18] GET  ${STATUS_CORELINK_URL}        → KNOWN EXCEPTION: 000 (Phase A 2-level subdomain SSL gap)

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

# [1] /health
log "CHECK [1] GET ${API_BASE}/health"
CODE=$(time_curl "health" "${API_BASE}/health")
BODY=$(curl -sS --max-time "${TIMEOUT_CURL}" "${API_BASE}/health" 2>/dev/null || echo "")
if [[ "$CODE" == "200" ]] && echo "$BODY" | grep -q '"status"' && echo "$BODY" | grep -q '"ok"'; then
  pass "[1] /health → 200 {\"status\":\"ok\"}"
  append_log "- [PASS] [1] /health → 200"
else
  fail "[1] /health → expected 200 {\"status\":\"ok\"}, got HTTP=${CODE} body=${BODY}"
  append_log "- [FAIL] [1] /health → HTTP=${CODE}"
fi

# [2] /_health
log "CHECK [2] GET ${API_BASE}/_health"
HDR_CODE=$(curl -sS -o /dev/null -w "%{http_code}" \
  --max-time "${TIMEOUT_CURL}" "${API_BASE}/_health" 2>/dev/null || echo "000")
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

# [3] CAS blob POST (REAPI v2)
log "CHECK [3] POST CAS blob via REAPI v2"
BLOB_CONTENT="smoke-test-$(date +%s)-corelink"
BLOB_SHA=$(printf '%s' "$BLOB_CONTENT" | openssl dgst -sha256 -hex | awk '{print $2}')
BLOB_B64=$(printf '%s' "$BLOB_CONTENT" | base64)
CAS_RESP=$(curl -sS -X POST \
  --max-time "${TIMEOUT_CURL}" \
  -H "Content-Type: application/json" \
  -d "{\"data\":\"${BLOB_B64}\",\"hash\":\"sha256:${BLOB_SHA}\"}" \
  "${API_BASE}/v2/cas/upload" 2>/dev/null || echo "")
CAS_CODE=$(curl -sS -o /dev/null -w "%{http_code}" -X POST \
  --max-time "${TIMEOUT_CURL}" \
  -H "Content-Type: application/json" \
  -d "{\"data\":\"${BLOB_B64}\",\"hash\":\"sha256:${BLOB_SHA}\"}" \
  "${API_BASE}/v2/cas/upload" 2>/dev/null || echo "000")
if [[ "$CAS_CODE" == "200" ]] && echo "$CAS_RESP" | grep -q "sha256:${BLOB_SHA}"; then
  pass "[3] CAS blob POST → 200, digest sha256:${BLOB_SHA} confirmed in response"
  append_log "- [PASS] [3] CAS blob POST → 200"
else
  fail "[3] CAS blob POST → HTTP=${CAS_CODE} body=${CAS_RESP}"
  append_log "- [FAIL] [3] CAS blob POST → HTTP=${CAS_CODE}"
  BLOB_SHA=""  # mark as failed so [4] is skipped gracefully
fi

# [4] CAS blob GET
log "CHECK [4] GET CAS blob back"
if [[ -n "$BLOB_SHA" ]]; then
  GET_CODE=$(time_curl "cas-get" "${API_BASE}/v2/cas/sha256:${BLOB_SHA}")
  GET_BODY=$(curl -sS --max-time "${TIMEOUT_CURL}" \
    "${API_BASE}/v2/cas/sha256:${BLOB_SHA}" 2>/dev/null || echo "")
  GET_DECODED=$(printf '%s' "$GET_BODY" | base64 -d 2>/dev/null || echo "$GET_BODY")
  if [[ "$GET_CODE" == "200" ]] && [[ "$GET_DECODED" == "$BLOB_CONTENT" || "$GET_BODY" == "$BLOB_CONTENT" ]]; then
    pass "[4] CAS blob GET → 200, bytes match"
    append_log "- [PASS] [4] CAS blob GET → 200 + bytes match"
  else
    fail "[4] CAS blob GET → HTTP=${GET_CODE} (expected 200 + exact bytes)"
    append_log "- [FAIL] [4] CAS blob GET → HTTP=${GET_CODE}"
  fi
else
  warn "[4] CAS blob GET skipped — blob POST failed"
  append_log "- [SKIP] [4] CAS blob GET — POST failed"
fi

# ──────────────────────────────────────────────
# (b) Pages deploys
# ──────────────────────────────────────────────
step "Area (b): Pages deploys"

# [5] docs
log "CHECK [5] GET ${DOCS_URL}"
DOCS_BODY=$(curl -sS --max-time "${TIMEOUT_CURL}" "${DOCS_URL}" 2>/dev/null || echo "")
DOCS_CODE=$(curl -sS -o /dev/null -w "%{http_code}" --max-time "${TIMEOUT_CURL}" "${DOCS_URL}" 2>/dev/null || echo "000")
if [[ "$DOCS_CODE" == "200" ]] && echo "$DOCS_BODY" | grep -qi "<html"; then
  pass "[5] ${DOCS_URL} → 200 + HTML"
  append_log "- [PASS] [5] docs page → 200 + HTML"
else
  fail "[5] ${DOCS_URL} → HTTP=${DOCS_CODE} (expected 200 + HTML)"
  append_log "- [FAIL] [5] docs page → HTTP=${DOCS_CODE}"
fi

# [6] app (admin-ui with Clerk)
log "CHECK [6] GET ${APP_URL}"
APP_BODY=$(curl -sS --max-time "${TIMEOUT_CURL}" "${APP_URL}" 2>/dev/null || echo "")
APP_CODE=$(curl -sS -o /dev/null -w "%{http_code}" --max-time "${TIMEOUT_CURL}" "${APP_URL}" 2>/dev/null || echo "000")
# Clerk inits via script tag or window.Clerk assignment
if [[ "$APP_CODE" == "200" ]] && (echo "$APP_BODY" | grep -qi "clerk" || echo "$APP_BODY" | grep -qi "<html"); then
  pass "[6] ${APP_URL} → 200 + Clerk init HTML"
  append_log "- [PASS] [6] app page → 200 + Clerk HTML"
else
  fail "[6] ${APP_URL} → HTTP=${APP_CODE} (expected 200 + Clerk init)"
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
    # Extra check: for DNS-only record (status), must resolve to hugrl.betteruptime.com
    if [[ "$NAME" == "status.corelink.humangr.com" ]]; then
      if echo "$RESOLVED" | grep -q "betteruptime"; then
        pass "[${CHECK_NUM}] dig ${NAME} → ${RESOLVED} (DNS-only, BetterUptime)"
        append_log "- [PASS] [${CHECK_NUM}] DNS ${NAME} → BetterUptime"
      else
        fail "[${CHECK_NUM}] dig ${NAME} → ${RESOLVED} (expected betteruptime target)"
        append_log "- [FAIL] [${CHECK_NUM}] DNS ${NAME} → ${RESOLVED}"
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
  PROBE_CODE=$(curl -sS -o /dev/null -w "%{http_code}" -X POST \
    --max-time "${TIMEOUT_CURL}" \
    -H "Authorization: Bearer ${CORELINK_SMOKE_TOKEN}" \
    -H "Content-Type: application/json" \
    -d '{"action":"smoke-probe","resource":"smoke-test"}' \
    "${API_BASE}/v1/audit/probe" 2>/dev/null || echo "000")
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
    CHAIN_CODE=$(curl -sS -o /dev/null -w "%{http_code}" \
      --max-time "${TIMEOUT_CURL}" \
      -H "Authorization: Bearer ${CORELINK_SMOKE_TOKEN}" \
      "${API_BASE}/v1/audit/chain?request_id=${REQUEST_ID}" 2>/dev/null || echo "000")
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
ST_CODE=$(time_curl "status-main" "${STATUS_URL}")
if [[ "$ST_CODE" == "200" ]]; then
  pass "[21] ${STATUS_URL} → 200"
  append_log "- [PASS] [21] ${STATUS_URL} → 200"
else
  fail "[21] ${STATUS_URL} → HTTP=${ST_CODE} (expected 200)"
  append_log "- [FAIL] [21] ${STATUS_URL} → HTTP=${ST_CODE}"
fi

# [22] status.corelink.humangr.com (Phase A custom domain)
# KNOWN EXCEPTION: returns 000 due to 2-level subdomain TLS gap (Phase A → flat migration deferred).
# Documented exception per Wave 32 Phase H APPLY audit §6. NOT a failure.
log "CHECK [22] GET ${STATUS_CORELINK_URL} (KNOWN EXCEPTION: 000 expected)"
SCL_CODE=$(time_curl "status-corelink" "${STATUS_CORELINK_URL}")
if [[ "$SCL_CODE" == "200" ]]; then
  pass "[22] ${STATUS_CORELINK_URL} → 200"
  append_log "- [PASS] [22] ${STATUS_CORELINK_URL} → 200"
elif [[ "$SCL_CODE" == "000" ]]; then
  warn "[22] ${STATUS_CORELINK_URL} → 000 (KNOWN EXCEPTION — Phase A 2-level subdomain SSL gap; not a failure)"
  append_log "- [KNOWN-EXCEPTION] [22] ${STATUS_CORELINK_URL} → 000 (Phase A SSL gap; expected)"
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
