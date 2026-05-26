#!/usr/bin/env bash
# teardown-cf-corelink-prod.sh — Wave 32 Phase C rollback
# Deletes all CoreLink production Cloudflare resources provisioned by
# provision-cf-corelink-prod.sh:
#   - 1 D1 database:   corelink-prod-d1
#   - 5 KV namespaces: corelink-prod-{jwks,cache,rate-limit,session,pilot-signup}-kv
#   - 6 R2 buckets:    corelink-cas-prod + corelink-ac-{sam,iad,lhr,nrt,syd}
#
# MANDATORY CONFIRMATION: operator must type DESTROY at the prompt.
# Re-running after all resources are deleted is a no-op.
#
# Credentials: loaded from .env.local (CLOUDFLARE_API_TOKEN, CLOUDFLARE_ACCOUNT_ID).
#
# Usage:
#   bash scripts/teardown-cf-corelink-prod.sh [--dry-run]
#
# Exit codes:
#   0 — all targeted resources deleted (or already absent)
#   1 — deletion failure
#   2 — pre-flight error or operator cancelled

set -euo pipefail

# ----------------------------------------------------------------------------
# Configuration
# ----------------------------------------------------------------------------

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
ENV_FILE="$REPO_ROOT/.env.local"
CF_API="https://api.cloudflare.com/client/v4"

DRY_RUN=false
if [[ "${1:-}" == "--dry-run" ]]; then
    DRY_RUN=true
    echo "[DRY-RUN] No real resources will be deleted."
fi

# ----------------------------------------------------------------------------
# Load credentials
# ----------------------------------------------------------------------------

if [[ ! -f "$ENV_FILE" ]]; then
    echo "FATAL: .env.local not found at $ENV_FILE" >&2
    exit 2
fi

CLOUDFLARE_API_TOKEN=""
CLOUDFLARE_ACCOUNT_ID=""

while IFS='=' read -r key val; do
    [[ "$key" =~ ^# ]] && continue
    [[ -z "$key" ]] && continue
    val="${val%\"}"
    val="${val#\"}"
    val="${val%\'}"
    val="${val#\'}"
    case "$key" in
        CLOUDFLARE_API_TOKEN)  CLOUDFLARE_API_TOKEN="$val"  ;;
        CLOUDFLARE_ACCOUNT_ID) CLOUDFLARE_ACCOUNT_ID="$val" ;;
    esac
done < "$ENV_FILE"

if [[ -z "$CLOUDFLARE_API_TOKEN" || -z "$CLOUDFLARE_ACCOUNT_ID" ]]; then
    echo "FATAL: CLOUDFLARE_API_TOKEN and CLOUDFLARE_ACCOUNT_ID must be set in $ENV_FILE" >&2
    exit 2
fi

if ! command -v curl >/dev/null 2>&1; then
    echo "FATAL: curl not found in PATH" >&2
    exit 2
fi
if ! command -v python3 >/dev/null 2>&1; then
    echo "FATAL: python3 not found in PATH" >&2
    exit 2
fi

# ----------------------------------------------------------------------------
# Resources to destroy (mirrors provision script exactly)
# ----------------------------------------------------------------------------

D1_DATABASES=(
    "corelink-prod-d1"
)

KV_NAMESPACES=(
    "corelink-prod-jwks-kv"
    "corelink-prod-cache-kv"
    "corelink-prod-rate-limit-kv"
    "corelink-prod-session-kv"
    "corelink-prod-pilot-signup-kv"
)

R2_BUCKETS=(
    "corelink-cas-prod"
    "corelink-ac-sam"
    "corelink-ac-iad"
    "corelink-ac-lhr"
    "corelink-ac-nrt"
    "corelink-ac-syd"
)

# ----------------------------------------------------------------------------
# Print DELETE plan
# ----------------------------------------------------------------------------

echo "============================================================"
echo " CoreLink Production Cloudflare TEARDOWN"
echo " Account: ${CLOUDFLARE_ACCOUNT_ID:0:8}... (redacted)"
echo " Dry-run: $DRY_RUN"
echo "============================================================"
echo ""
echo "DELETE PLAN:"
echo ""
echo "  D1 Databases:"
for db in "${D1_DATABASES[@]}"; do
    echo "    DELETE corelink-prod-d1: $db"
done
echo ""
echo "  KV Namespaces:"
for ns in "${KV_NAMESPACES[@]}"; do
    echo "    DELETE KV namespace: $ns"
done
echo ""
echo "  R2 Buckets:"
for b in "${R2_BUCKETS[@]}"; do
    echo "    DELETE R2 bucket: $b  (WARNING: all objects will be lost)"
done
echo ""

# ----------------------------------------------------------------------------
# Mandatory confirmation prompt
# ----------------------------------------------------------------------------

if [[ "$DRY_RUN" == "true" ]]; then
    echo "[DRY-RUN] Skipping confirmation prompt. Above is the DELETE plan."
    echo "[DRY-RUN] Re-run without --dry-run to execute (confirmation required)."
    exit 0
fi

echo "WARNING: This action is IRREVERSIBLE. All data in the above resources will be permanently deleted."
echo ""
echo -n "Type DESTROY to confirm and proceed: "
read -r confirmation
if [[ "$confirmation" != "DESTROY" ]]; then
    echo "Cancelled. You typed: '$confirmation' (expected: DESTROY)"
    exit 2
fi

echo ""
echo "Confirmed. Proceeding with deletion..."

# ----------------------------------------------------------------------------
# Helper: CF API call
# ----------------------------------------------------------------------------

cf_api() {
    local method="$1"
    local path="$2"
    local body="${3:-}"
    local url="$CF_API$path"

    if [[ -n "$body" ]]; then
        curl -s -X "$method" "$url" \
            -H "Authorization: Bearer $CLOUDFLARE_API_TOKEN" \
            -H "Content-Type: application/json" \
            -d "$body"
    else
        curl -s -X "$method" "$url" \
            -H "Authorization: Bearer $CLOUDFLARE_API_TOKEN"
    fi
}

check_success() {
    local response="$1"
    local resource="$2"
    local ok
    ok=$(echo "$response" | python3 -c "import json,sys; d=json.load(sys.stdin); print(d.get('success',''))" 2>/dev/null || echo "")
    if [[ "$ok" != "True" && "$ok" != "true" ]]; then
        local errors
        errors=$(echo "$response" | python3 -c "import json,sys; d=json.load(sys.stdin); [print(e) for e in d.get('errors',[])]" 2>/dev/null || echo "unknown error")
        echo "ERROR: Failed to delete $resource: $errors" >&2
        return 1
    fi
    return 0
}

# ----------------------------------------------------------------------------
# Delete D1 databases
# ----------------------------------------------------------------------------

echo ""
echo "=== Deleting D1 Databases ==="

for db_name in "${D1_DATABASES[@]}"; do
    echo ""
    echo "--- D1: $db_name ---"

    local_list=$(cf_api GET "/accounts/$CLOUDFLARE_ACCOUNT_ID/d1/database")
    db_uuid=$(echo "$local_list" | python3 -c "
import json, sys
d = json.load(sys.stdin)
for db in d.get('result', []):
    if db.get('name') == '$db_name':
        print(db.get('uuid', ''))
        break
else:
    print('')
" 2>/dev/null || echo "")

    if [[ -z "$db_uuid" ]]; then
        echo "  SKIPPED: $db_name not found (already deleted or never created)"
        continue
    fi

    echo "  Deleting D1: $db_name (uuid=$db_uuid)"
    del_resp=$(cf_api DELETE "/accounts/$CLOUDFLARE_ACCOUNT_ID/d1/database/$db_uuid")
    if check_success "$del_resp" "D1 $db_name"; then
        echo "  DELETED: $db_name"
    fi
done

# ----------------------------------------------------------------------------
# Delete KV namespaces
# ----------------------------------------------------------------------------

echo ""
echo "=== Deleting KV Namespaces ==="

for ns_title in "${KV_NAMESPACES[@]}"; do
    echo ""
    echo "--- KV: $ns_title ---"

    local_list=$(cf_api GET "/accounts/$CLOUDFLARE_ACCOUNT_ID/storage/kv/namespaces")
    ns_id=$(echo "$local_list" | python3 -c "
import json, sys
d = json.load(sys.stdin)
for ns in d.get('result', []):
    if ns.get('title') == '$ns_title':
        print(ns.get('id', ''))
        break
else:
    print('')
" 2>/dev/null || echo "")

    if [[ -z "$ns_id" ]]; then
        echo "  SKIPPED: $ns_title not found (already deleted or never created)"
        continue
    fi

    echo "  Deleting KV: $ns_title (id=$ns_id)"
    del_resp=$(cf_api DELETE "/accounts/$CLOUDFLARE_ACCOUNT_ID/storage/kv/namespaces/$ns_id")
    if check_success "$del_resp" "KV $ns_title"; then
        echo "  DELETED: $ns_title"
    fi
done

# ----------------------------------------------------------------------------
# Delete R2 buckets
# ----------------------------------------------------------------------------

echo ""
echo "=== Deleting R2 Buckets ==="

for bucket_name in "${R2_BUCKETS[@]}"; do
    echo ""
    echo "--- R2: $bucket_name ---"

    local_list=$(cf_api GET "/accounts/$CLOUDFLARE_ACCOUNT_ID/r2/buckets?per_page=100")
    b_exists=$(echo "$local_list" | python3 -c "
import json, sys
d = json.load(sys.stdin)
for b in d.get('result', {}).get('buckets', []):
    if b.get('name') == '$bucket_name':
        print('yes')
        break
else:
    print('')
" 2>/dev/null || echo "")

    if [[ -z "$b_exists" ]]; then
        echo "  SKIPPED: $bucket_name not found (already deleted or never created)"
        continue
    fi

    echo "  Deleting R2 bucket: $bucket_name"
    del_resp=$(cf_api DELETE "/accounts/$CLOUDFLARE_ACCOUNT_ID/r2/buckets/$bucket_name")
    if check_success "$del_resp" "R2 $bucket_name"; then
        echo "  DELETED: $bucket_name"
    fi
done

echo ""
echo "============================================================"
echo " Teardown complete."
echo " Run provision-cf-corelink-prod.sh to re-provision if needed."
echo "============================================================"
