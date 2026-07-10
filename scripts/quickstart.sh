#!/usr/bin/env bash
# quickstart.sh — CoreLink 5-minute quickstart verification script
#
# Verifies a CAS PUT/GET roundtrip against the CoreLink production API.
# Matches the exact curl commands shown in:
#   apps/docs/docs/tutorials/quickstart-5min.mdx
#
# Usage:
#   export CORELINK_PAT="corelink_pat_..."
#   export CORELINK_TENANT="<uuid>"
#   bash scripts/quickstart.sh
#
# Exit code: 0 = PASS, 1 = FAIL, 2 = bad invocation (missing env vars)
#
# Hard rules:
#   - No secrets written to disk or printed to stdout.
#   - All network calls use -sSf (fail fast on non-2xx).
#   - Digest computed from the payload bytes, verified on roundtrip.

set -euo pipefail

# ─────────────────────────────────────────
# Constants
# ─────────────────────────────────────────
API_BASE="https://corelink-api.humangr.com"
TIMEOUT=15   # seconds per curl call

# Colour helpers (no-op when stdout is not a tty)
RED=""; GREEN=""; RESET=""
if [[ -t 1 ]]; then
  RED='\033[0;31m'; GREEN='\033[0;32m'; RESET='\033[0m'
fi

pass() { printf "${GREEN}[PASS]${RESET} %s\n" "$*"; }
fail() { printf "${RED}[FAIL]${RESET} %s\n" "$*" >&2; }
info() { printf '       %s\n' "$*"; }

# ─────────────────────────────────────────
# Prerequisite checks
# ─────────────────────────────────────────
if [[ -z "${CORELINK_PAT:-}" ]]; then
  echo "ERROR: CORELINK_PAT is not set." >&2
  echo "       Get your PAT from https://corelink-admin.humangr.com/sign-up" >&2
  echo "       then run: export CORELINK_PAT=\"corelink_pat_...\"" >&2
  exit 2
fi

if [[ -z "${CORELINK_TENANT:-}" ]]; then
  echo "ERROR: CORELINK_TENANT is not set." >&2
  echo "       Copy your Tenant ID (UUID) from the /welcome page after sign-up." >&2
  echo "       then run: export CORELINK_TENANT=\"xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx\"" >&2
  exit 2
fi

if ! command -v curl &>/dev/null; then
  echo "ERROR: curl not found in PATH. Install curl and re-run." >&2
  exit 2
fi

# ─────────────────────────────────────────
# BLAKE3 helper — the native CAS content-addresses by BLAKE3, so the
# digest in the URL MUST be the blake3 hex (a SHA-256 digest 422s).
# ─────────────────────────────────────────
blake3_file() {
  local file="$1"
  if command -v b3sum &>/dev/null; then
    b3sum "$file" | awk '{print $1}'
  else
    echo "ERROR: b3sum not found in PATH. Install it with 'brew install b3sum'" >&2
    echo "       (macOS) or 'cargo install b3sum' / your distribution's package (Linux)." >&2
    exit 2
  fi
}

# Wall-clock start
T_START=$(date +%s)

echo ""
echo "CoreLink quickstart — PUT/GET roundtrip"
echo "  API:    $API_BASE"
echo "  Tenant: $CORELINK_TENANT"
echo "  PAT:    ${CORELINK_PAT:0:24}... (truncated)"
echo ""

# ─────────────────────────────────────────
# Step 1 — create a unique test payload
# ─────────────────────────────────────────
PAYLOAD_FILE="$(mktemp /tmp/cl-quickstart-XXXXXX.txt)"
# Unique payload so each run creates a distinct blob
printf 'hello corelink quickstart — %s\n' "$(date -u +"%Y-%m-%dT%H:%M:%SZ")" > "$PAYLOAD_FILE"
PAYLOAD_SIZE=$(wc -c < "$PAYLOAD_FILE" | tr -d ' ')
DIGEST=$(blake3_file "$PAYLOAD_FILE")

info "Payload:  $PAYLOAD_FILE ($PAYLOAD_SIZE bytes)"
info "BLAKE3:   $DIGEST"
echo ""

# ─────────────────────────────────────────
# Step 2 — PUT the blob (expect 201)
# ─────────────────────────────────────────
echo "→ PUT /v1/cas/$CORELINK_TENANT/$DIGEST"
PUT_RESPONSE=$(curl -sS \
  --max-time "$TIMEOUT" \
  -w "\nHTTP_STATUS:%{http_code}" \
  -X PUT \
  -H "Authorization: Bearer $CORELINK_PAT" \
  -H "Content-Type: application/octet-stream" \
  --data-binary "@$PAYLOAD_FILE" \
  "${API_BASE}/v1/cas/${CORELINK_TENANT}/${DIGEST}" 2>&1) || {
  fail "PUT request failed (curl error)"
  rm -f "$PAYLOAD_FILE"
  exit 1
}

PUT_BODY=$(echo "$PUT_RESPONSE" | grep -v "^HTTP_STATUS:" || true)
PUT_STATUS=$(echo "$PUT_RESPONSE" | grep "^HTTP_STATUS:" | cut -d: -f2)

if [[ "$PUT_STATUS" == "201" ]]; then
  pass "PUT → 201 Created  body=$PUT_BODY"
else
  fail "PUT → expected 201, got HTTP $PUT_STATUS  body=$PUT_BODY"
  info "Hint: check that CORELINK_PAT and CORELINK_TENANT are correct."
  rm -f "$PAYLOAD_FILE"
  exit 1
fi

# ─────────────────────────────────────────
# Step 3 — GET the blob back (expect 200)
# ─────────────────────────────────────────
DOWNLOAD_FILE="$(mktemp /tmp/cl-quickstart-dl-XXXXXX.txt)"
echo ""
echo "→ GET /v1/cas/$CORELINK_TENANT/$DIGEST"
GET_STATUS=$(curl -sS \
  --max-time "$TIMEOUT" \
  -o "$DOWNLOAD_FILE" \
  -w "%{http_code}" \
  -H "Authorization: Bearer $CORELINK_PAT" \
  "${API_BASE}/v1/cas/${CORELINK_TENANT}/${DIGEST}" 2>/dev/null) || {
  fail "GET request failed (curl error)"
  rm -f "$PAYLOAD_FILE" "$DOWNLOAD_FILE"
  exit 1
}

if [[ "$GET_STATUS" != "200" ]]; then
  fail "GET → expected 200, got HTTP $GET_STATUS"
  info "Body: $(cat "$DOWNLOAD_FILE" 2>/dev/null | head -3)"
  rm -f "$PAYLOAD_FILE" "$DOWNLOAD_FILE"
  exit 1
fi

pass "GET → 200 OK"

# ─────────────────────────────────────────
# Step 4 — byte-exact roundtrip check
# ─────────────────────────────────────────
DOWNLOAD_DIGEST=$(blake3_file "$DOWNLOAD_FILE")
echo ""
if [[ "$DOWNLOAD_DIGEST" == "$DIGEST" ]]; then
  pass "Roundtrip — BLAKE3 matches: $DIGEST"
else
  fail "Roundtrip — digest mismatch!"
  info "  uploaded:   $DIGEST"
  info "  downloaded: $DOWNLOAD_DIGEST"
  rm -f "$PAYLOAD_FILE" "$DOWNLOAD_FILE"
  exit 1
fi

# ─────────────────────────────────────────
# Summary
# ─────────────────────────────────────────
T_END=$(date +%s)
ELAPSED=$(( T_END - T_START ))

rm -f "$PAYLOAD_FILE" "$DOWNLOAD_FILE"

echo ""
echo "════════════════════════════════════════"
printf "${GREEN}PASS${RESET} — CoreLink PUT/GET roundtrip verified\n"
echo "  Wall-clock: ${ELAPSED}s"
echo "  Digest:     $DIGEST"
echo "  Tenant:     $CORELINK_TENANT"
echo "  API:        $API_BASE"
echo "════════════════════════════════════════"
echo ""
echo "Next: wire your build tool → apps/docs/docs/integrations/"
echo "      or run the 10-minute quickstart for Bazel cache HITs."
exit 0
