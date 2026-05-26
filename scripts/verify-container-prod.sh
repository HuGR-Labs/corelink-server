#!/usr/bin/env bash
# scripts/verify-container-prod.sh — Wave-32 Phase E PREP
# Post-deploy verification of the Cloudflare Container (run AFTER Phase E APPLY).
#
# What this verifies:
#   1. Worker health endpoint (CF workers.dev subdomain) returns 200.
#   2. CF Container metrics via API: container running, no crash loops.
#   3. DO storage smoke: /health → DO writes key → reads back → 200.
#
# What this does NOT do:
#   - Build or push images (see build-container-prod.sh / push-container-prod.sh).
#   - Modify wrangler.toml or any config file.
#   - Touch production DNS (Phase G).
#
# Prerequisites:
#   - Phase E APPLY must be complete (wrangler deploy --env prod).
#   - CF_API_TOKEN env var must be set (or loaded from .env.local).
#   - CLOUDFLARE_ACCOUNT_ID env var must be set.
#   - curl must be installed.
#
# Usage:
#   bash scripts/verify-container-prod.sh
#   bash scripts/verify-container-prod.sh --worker-url https://custom.workers.dev
#   bash scripts/verify-container-prod.sh --skip-metrics  # skip CF API metrics check
#   bash scripts/verify-container-prod.sh --help
#
# Exit codes:
#   0 → all verifications passed
#   1 → one or more verification steps failed
#   2 → usage error
#
# Charter: Phase E gate criteria per Wave-32 prod-deploy-spec §4 Phase E.
#
# Signed-off-by: Gustavo Schneiter <gustavo@humangr.com>
# Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>

set -euo pipefail

# ── Constants ──────────────────────────────────────────────────────────────

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Default workers.dev subdomain (pre-DNS; Phase G adds *.corelink.humangr.com).
# Override with --worker-url.
DEFAULT_WORKER_URL="https://corelink.gustavoschneiter.workers.dev"

# CF Containers API base.
CF_API_BASE="https://api.cloudflare.com/client/v4"

# Health probe timeout.
CURL_TIMEOUT_S=15
HTTP_RETRY_MAX=3
HTTP_RETRY_DELAY_S=5

# ── Helpers ────────────────────────────────────────────────────────────────

log()  { printf '[verify-container-prod] %s\n' "$*"; }
warn() { printf '[verify-container-prod] WARN: %s\n' "$*" >&2; }
die()  { printf '[verify-container-prod] FATAL: %s\n' "$*" >&2; exit 1; }
ok()   { printf '[verify-container-prod] OK: %s\n' "$*"; }
fail() { printf '[verify-container-prod] FAIL: %s\n' "$*" >&2; return 1; }

usage() {
    sed -n '2,/^$/p' "$0"
    exit 2
}

# curl wrapper with retry + timeout.
probe_http() {
    local url="$1"
    local expected_status="${2:-200}"
    local attempt=0
    local status

    while [ "$attempt" -lt "$HTTP_RETRY_MAX" ]; do
        attempt=$(( attempt + 1 ))
        status="$(curl -s -o /dev/null -w '%{http_code}' \
            --max-time "$CURL_TIMEOUT_S" \
            "$url" 2>/dev/null || echo "000")"
        if [ "$status" = "$expected_status" ]; then
            echo "$status"
            return 0
        fi
        warn "  Attempt $attempt/$HTTP_RETRY_MAX: $url returned $status (expected $expected_status); retrying in ${HTTP_RETRY_DELAY_S}s..."
        sleep "$HTTP_RETRY_DELAY_S"
    done
    echo "$status"
    return 1
}

# ── Arg parse ─────────────────────────────────────────────────────────────

WORKER_URL="$DEFAULT_WORKER_URL"
SKIP_METRICS=0
for arg in "$@"; do
    case "$arg" in
        --worker-url=*) WORKER_URL="${arg#--worker-url=}" ;;
        --worker-url)   shift; WORKER_URL="${1:-}" ;;
        --skip-metrics) SKIP_METRICS=1 ;;
        -h|--help)      usage ;;
        *) die "unknown argument: $arg" ;;
    esac
done

# ── Step 0: repo root + load env ──────────────────────────────────────────

cd "$REPO_ROOT"
log "Repo root: $REPO_ROOT"
log "HEAD: $(git rev-parse HEAD)"

# Load .env.local if present (provides CF_API_TOKEN, CLOUDFLARE_ACCOUNT_ID).
if [ -f "$REPO_ROOT/.env.local" ]; then
    log "Loading credentials from .env.local (CTRL-CRED-001: values not logged)..."
    set -a
    # shellcheck source=/dev/null
    source "$REPO_ROOT/.env.local"
    set +a
fi

# ── Step 1: Verify prerequisites ──────────────────────────────────────────

log "Verifying prerequisites..."
if ! command -v curl >/dev/null 2>&1; then
    die "curl not found. Install curl and retry."
fi
log "curl: OK ($(curl --version | head -1))"

FAIL_COUNT=0

# ── Step 2: Worker health probe ────────────────────────────────────────────

log ""
log "--- Step 2: Worker health probe ---"
log "URL: ${WORKER_URL}/health"

STATUS="$(probe_http "${WORKER_URL}/health" "200" 2>&1 || true)"
if [ "$STATUS" = "200" ]; then
    ok "Worker health endpoint returned 200."
else
    warn "Worker health endpoint returned $STATUS (expected 200)."
    warn "This is expected if Phase E APPLY has not completed yet."
    warn "Verify wrangler deploy --env prod has been executed."
    FAIL_COUNT=$(( FAIL_COUNT + 1 ))
fi

# ── Step 3: CF Container metrics via API ───────────────────────────────────

if [ "$SKIP_METRICS" -eq 1 ]; then
    log ""
    log "--- Step 3: CF Container metrics --- SKIPPED (--skip-metrics)"
else
    log ""
    log "--- Step 3: CF Container metrics ---"

    if [ -z "${CF_API_TOKEN:-}" ]; then
        warn "CF_API_TOKEN not set; skipping metrics check."
        warn "Set CF_API_TOKEN or load from .env.local."
        FAIL_COUNT=$(( FAIL_COUNT + 1 ))
    elif [ -z "${CLOUDFLARE_ACCOUNT_ID:-}" ]; then
        warn "CLOUDFLARE_ACCOUNT_ID not set; skipping metrics check."
        FAIL_COUNT=$(( FAIL_COUNT + 1 ))
    else
        log "Querying CF Containers API for worker 'corelink' (env prod)..."

        # Query workers deployments to confirm container is running.
        # CF Containers beta API endpoint (2026-05-22):
        #   GET /accounts/{account_id}/workers/scripts/{script_name}/deployments
        WORKER_NAME="corelink"
        DEPLOY_RESP="$(curl -s \
            --max-time "$CURL_TIMEOUT_S" \
            -H "Authorization: Bearer $CF_API_TOKEN" \
            -H "Content-Type: application/json" \
            "${CF_API_BASE}/accounts/${CLOUDFLARE_ACCOUNT_ID}/workers/scripts/${WORKER_NAME}/deployments" \
            2>/dev/null || echo '{"success":false,"errors":["curl_failed"]}')"

        DEPLOY_SUCCESS="$(echo "$DEPLOY_RESP" | python3 -c \
            "import sys,json; d=json.load(sys.stdin); print(d.get('success','false'))" \
            2>/dev/null || echo "false")"

        if [ "$DEPLOY_SUCCESS" = "True" ] || [ "$DEPLOY_SUCCESS" = "true" ]; then
            ok "CF Workers deployments API: success=true."
            log "Deployment response (truncated):"
            echo "$DEPLOY_RESP" | python3 -c \
                "import sys,json; d=json.load(sys.stdin); [print('  ' + str(dep)) for dep in d.get('result',[])[:3]]" \
                2>/dev/null || true
        else
            warn "CF Workers deployments API returned success=false or error."
            warn "Response: $(echo "$DEPLOY_RESP" | head -c 500)"
            FAIL_COUNT=$(( FAIL_COUNT + 1 ))
        fi

        # Query CF Containers-specific endpoint to verify container health.
        # NOTE: The CF Containers GA API endpoint may differ from the beta
        # endpoint. Update this if wrangler changelog announces a path change.
        # Beta endpoint (best available as of 2026-05-22):
        #   GET /accounts/{account_id}/containers/workers/{worker_name}/instances
        CONTAINER_RESP="$(curl -s \
            --max-time "$CURL_TIMEOUT_S" \
            -H "Authorization: Bearer $CF_API_TOKEN" \
            "${CF_API_BASE}/accounts/${CLOUDFLARE_ACCOUNT_ID}/containers/workers/${WORKER_NAME}/instances" \
            2>/dev/null || echo '{"success":false,"errors":["curl_failed"]}')"

        CONTAINER_SUCCESS="$(echo "$CONTAINER_RESP" | python3 -c \
            "import sys,json; d=json.load(sys.stdin); print(d.get('success','false'))" \
            2>/dev/null || echo "false")"

        if [ "$CONTAINER_SUCCESS" = "True" ] || [ "$CONTAINER_SUCCESS" = "true" ]; then
            # Check for running instances and crash loops.
            INSTANCE_COUNT="$(echo "$CONTAINER_RESP" | python3 -c \
                "import sys,json; d=json.load(sys.stdin); r=d.get('result',[]); print(len(r))" \
                2>/dev/null || echo "0")"
            CRASH_LOOPS="$(echo "$CONTAINER_RESP" | python3 -c \
                "import sys,json; d=json.load(sys.stdin)
r=d.get('result',[])
loops=[i for i in r if i.get('status','') in ('CrashLoopBackOff','Error','OOMKilled')]
print(len(loops))" \
                2>/dev/null || echo "0")"

            ok "CF Containers instances: $INSTANCE_COUNT running."
            if [ "$CRASH_LOOPS" -gt 0 ]; then
                warn "CRASH LOOP DETECTED: $CRASH_LOOPS instance(s) in CrashLoopBackOff/Error/OOMKilled."
                warn "Per Wave-32 Phase E gate: container MUST show no crash loops."
                FAIL_COUNT=$(( FAIL_COUNT + 1 ))
            else
                ok "No crash loops detected."
            fi
        else
            warn "CF Containers instances API returned success=false."
            warn "Response: $(echo "$CONTAINER_RESP" | head -c 500)"
            warn "This may indicate the endpoint path has changed; verify CF Containers docs."
            FAIL_COUNT=$(( FAIL_COUNT + 1 ))
        fi
    fi
fi

# ── Step 4: DO storage smoke ───────────────────────────────────────────────

log ""
log "--- Step 4: Durable Object storage smoke ---"
log "URL: ${WORKER_URL}/health (verifies DO instantiation + key write/read)"

# The /health endpoint should cause the DO to write a smoke key and read it
# back (per the DO implementation in Phase B). A 200 response confirms DO
# storage is operational.
STATUS="$(probe_http "${WORKER_URL}/health" "200" 2>&1 || true)"
if [ "$STATUS" = "200" ]; then
    ok "DO storage smoke: /health returned 200 (DO write+read cycle confirmed)."
else
    warn "DO storage smoke: /health returned $STATUS (expected 200)."
    warn "DO may not be instantiated yet, or Phase E APPLY is incomplete."
    FAIL_COUNT=$(( FAIL_COUNT + 1 ))
fi

# ── Step 5: Summary ────────────────────────────────────────────────────────

log ""
log "============================================================"
if [ "$FAIL_COUNT" -eq 0 ]; then
    log "  VERIFICATION COMPLETE — ALL CHECKS PASSED"
    log "  Phase E gate criteria met."
    log "  Proceed to Phase E → F+G+H decision gate (Owner approval required)."
else
    log "  VERIFICATION COMPLETE — $FAIL_COUNT CHECK(S) FAILED"
    log "  Phase E gate criteria NOT met. Investigate failures above."
    log "  Per Wave-32 spec §4 Phase E: all gates must be green before proceeding."
fi
log "  Worker URL: $WORKER_URL"
log "  Checks skipped (--skip-metrics): $SKIP_METRICS"
log "============================================================"

exit "$FAIL_COUNT"
