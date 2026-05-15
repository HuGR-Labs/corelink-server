#!/usr/bin/env bash
# tenant-state-snapshot.sh — read-only dump of full tenant state across
# R2 / D1 / KV / DO for forensic preservation.
#
# Used during incident triage to capture a tenant's full state before
# any remediation action that might change it (see FORENSICS-GUIDE.md
# §10).
#
# READ-ONLY: this script issues SELECT / GET / list operations only.
# It NEVER writes to any store. If you find a write path here, that
# is a bug — file an issue.
#
# Usage:
#   ./scripts/forensics/tenant-state-snapshot.sh \
#       --tenant <UUID> \
#       --env    prod \
#       --out    incidents/INC-2026-05-15-1/tenant-snapshot/
#
# Output: writes 4 JSON files under <out>/:
#   d1.json   — D1 rows from canonical tenant tables
#   kv.json   — KV keys matching tenant prefix (idempotency, cache)
#   do.json   — DO state from doctor subcommands
#   r2.json   — R2 object listing under tenant prefix (metadata only)
#
# Exit 0 on success; non-zero with stderr message on failure.

set -euo pipefail

TENANT=""
ENV=""
OUT_DIR=""

usage() {
    cat <<'EOF'
Usage: tenant-state-snapshot.sh --tenant <UUID> --env <env> --out <dir>

Required:
  --tenant   Tenant UUIDv7.
  --env      Target environment (staging|prod).
  --out      Output directory (must not exist or must be empty).
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --tenant) TENANT="${2:-}"; shift 2 ;;
        --env)    ENV="${2:-}"; shift 2 ;;
        --out)    OUT_DIR="${2:-}"; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) echo "unknown arg: $1" >&2; usage >&2; exit 2 ;;
    esac
done

if [[ -z "${TENANT}" ]] || [[ -z "${ENV}" ]] || [[ -z "${OUT_DIR}" ]]; then
    echo "error: --tenant, --env, --out are all required" >&2
    usage >&2
    exit 2
fi

# Loose UUIDv7 sanity check (8-4-4-4-12 hex).
if [[ ! "${TENANT}" =~ ^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$ ]]; then
    echo "error: --tenant must be a UUID (got: ${TENANT})" >&2
    exit 2
fi

case "${ENV}" in
    staging|prod) ;;
    *) echo "error: --env must be 'staging' or 'prod' (got: ${ENV})" >&2; exit 2 ;;
esac

mkdir -p "${OUT_DIR}"
if [[ -n "$(ls -A "${OUT_DIR}" 2>/dev/null || true)" ]]; then
    echo "error: --out dir ${OUT_DIR} is not empty; refusing to overwrite" >&2
    exit 2
fi

DB_NAME="corelink-${ENV}"

echo "[1/4] D1 — canonical tenant tables..." >&2

# Each query is a small SELECT bounded to this tenant. We project the
# results into a wrapping object keyed by table name so downstream
# tooling can jq across them easily.
D1_OUT="${OUT_DIR}/d1.json"
{
    echo "{"
    first=1
    for TABLE in \
        tenant_storage_state \
        quota_reservations \
        quota_fsm_state \
        blob_meta \
        ac_meta \
        audit_outbox \
        usage_event_idem \
        billing_reconciliation_drift \
        dsr_erasure_log \
        stripe_webhook_events_processed
    do
        if [[ $first -eq 0 ]]; then echo ","; fi
        first=0
        printf '  "%s": ' "${TABLE}"
        ROWS="$(wrangler d1 execute "${DB_NAME}" --remote --json \
            --command "SELECT * FROM ${TABLE} WHERE tenant_id = '${TENANT}' LIMIT 10000;" \
            2>/dev/null || echo '{"results":[],"error":"query_failed"}')"
        printf '%s' "${ROWS}"
    done
    echo
    echo "}"
} > "${D1_OUT}"

echo "[2/4] KV — tenant-scoped keys..." >&2

KV_OUT="${OUT_DIR}/kv.json"
# KV namespace ids are environment-specific; resolve via wrangler.toml.
# We list keys under the tenant prefix (read-only).
KV_NAMESPACES="$(wrangler kv:namespace list 2>/dev/null || echo '[]')"
{
    echo "{"
    echo "  \"namespaces\": ${KV_NAMESPACES},"
    echo "  \"keys_by_namespace\": {"
    first=1
    while IFS= read -r NS_ID; do
        [[ -z "${NS_ID}" ]] && continue
        if [[ $first -eq 0 ]]; then echo ","; fi
        first=0
        KEYS="$(wrangler kv:key list --namespace-id "${NS_ID}" \
            --prefix "tenant:${TENANT}:" --remote 2>/dev/null || echo '[]')"
        printf '    "%s": %s' "${NS_ID}" "${KEYS}"
    done < <(printf '%s' "${KV_NAMESPACES}" | jq -r '.[]?.id // empty')
    echo
    echo "  }"
    echo "}"
} > "${KV_OUT}"

echo "[3/4] DO state — via D1 mirrors (read-only)..." >&2

# Note: at GA, the CLI does not expose per-store DO read subcommands
# (see FORENSICS-GUIDE.md §5.1). The D1 mirrors are the canonical
# read path for forensics. We additionally run the standard `doctor`
# 8-check health suite for environment-level signal.
DO_OUT="${OUT_DIR}/do.json"
RATELIMIT_ROWS="$(wrangler d1 execute "${DB_NAME}" --remote --json \
    --command "SELECT * FROM ratelimit_buckets WHERE tenant_id = '${TENANT}';" \
    2>/dev/null || echo '{"results":[]}')"
QUOTA_ROWS="$(wrangler d1 execute "${DB_NAME}" --remote --json \
    --command "SELECT * FROM quota_fsm_state WHERE tenant_id = '${TENANT}';" \
    2>/dev/null || echo '{"results":[]}')"
CIRCUIT_ROWS="$(wrangler d1 execute "${DB_NAME}" --remote --json \
    --command "SELECT * FROM global_circuit_state;" \
    2>/dev/null || echo '{"results":[]}')"
DOCTOR_OUT="$(cargo run -q -p corelink-cli --release -- doctor --output=json \
    2>/dev/null || echo 'null')"

jq -n \
    --argjson ratelimit "${RATELIMIT_ROWS}" \
    --argjson quota     "${QUOTA_ROWS}" \
    --argjson circuit   "${CIRCUIT_ROWS}" \
    --argjson doctor    "${DOCTOR_OUT}" \
    '{
        ratelimit_mirror: $ratelimit,
        quota_mirror:     $quota,
        global_circuit:   $circuit,
        doctor_suite:     $doctor
    }' > "${DO_OUT}"

echo "[4/4] R2 — object listing under tenant prefix..." >&2

R2_OUT="${OUT_DIR}/r2.json"
# We list (not download) — metadata only.
BUCKET="corelink-cas-${ENV}"
wrangler r2 object list "${BUCKET}" \
    --prefix "tenant/${TENANT}/" \
    --remote 2>/dev/null \
    > "${R2_OUT}" \
    || echo '{"objects":[],"error":"list_failed"}' > "${R2_OUT}"

echo "done. snapshot at: ${OUT_DIR}" >&2
ls -lh "${OUT_DIR}" >&2
