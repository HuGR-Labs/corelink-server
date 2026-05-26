#!/usr/bin/env bash
# provision-cf-corelink-prod.sh — Wave 32 Phase C
# Idempotent provisioning of CoreLink production Cloudflare resources:
#   - 1 D1 database:          corelink-prod-d1
#   - 5 KV namespaces:        corelink-prod-{jwks,cache,rate-limit,session,pilot-signup}-kv
#   - 6 R2 buckets:           corelink-cas-prod + corelink-ac-{sam,iad,lhr,nrt,syd}
#
# Idempotency: list-by-name → if found reuse ID → if absent POST to create.
# Re-running on an already-provisioned account is a no-op (prints existing IDs).
#
# Credentials: loaded from .env.local at repo root (CLOUDFLARE_API_TOKEN,
# CLOUDFLARE_ACCOUNT_ID). NEVER hardcode values here — CTRL-CRED-001.
#
# Charter compliance:
#   CTRL-CRED-001 — no token value in any committed file.
#   INV-DATA-RESIDENCY — each R2 bucket pinned to its region via locationHint.
#
# Usage:
#   bash scripts/provision-cf-corelink-prod.sh [--dry-run]
#
# Exit codes:
#   0 — all resources provisioned or already exist
#   1 — provisioning failure
#   2 — pre-flight error (missing credentials, curl unavailable, etc.)

set -euo pipefail

# ----------------------------------------------------------------------------
# Configuration
# ----------------------------------------------------------------------------

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
ENV_FILE="$REPO_ROOT/.env.local"
WRANGLER_TOML="$REPO_ROOT/wrangler.toml"
CF_API="https://api.cloudflare.com/client/v4"

DRY_RUN=false
if [[ "${1:-}" == "--dry-run" ]]; then
    DRY_RUN=true
    echo "[DRY-RUN] No real resources will be created."
fi

# ----------------------------------------------------------------------------
# Load credentials
# ----------------------------------------------------------------------------

if [[ ! -f "$ENV_FILE" ]]; then
    echo "FATAL: .env.local not found at $ENV_FILE" >&2
    exit 2
fi

# Source only the CF_ vars — do not export arbitrary env.
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
    echo "FATAL: python3 not found in PATH (needed for JSON parsing)" >&2
    exit 2
fi

echo "============================================================"
echo " CoreLink Production Cloudflare Provisioning"
echo " Account: ${CLOUDFLARE_ACCOUNT_ID:0:8}... (redacted)"
echo " Dry-run: $DRY_RUN"
echo "============================================================"

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

# Parse a JSON field from response using python3
json_get() {
    local json="$1"
    local field="$2"
    echo "$json" | python3 -c "import json,sys; d=json.load(sys.stdin); print(d.get('$field',''))" 2>/dev/null || echo ""
}

json_get_nested() {
    local json="$1"
    local keys="$2"  # dot-separated
    echo "$json" | python3 -c "
import json, sys
d = json.load(sys.stdin)
keys = '$keys'.split('.')
for k in keys:
    if isinstance(d, dict):
        d = d.get(k, '')
    else:
        d = ''
        break
print(d)
" 2>/dev/null || echo ""
}

check_success() {
    local response="$1"
    local resource="$2"
    local ok
    ok=$(json_get "$response" "success")
    if [[ "$ok" != "True" && "$ok" != "true" ]]; then
        local errors
        errors=$(echo "$response" | python3 -c "import json,sys; d=json.load(sys.stdin); [print(e) for e in d.get('errors',[])]" 2>/dev/null || echo "unknown error")
        echo "ERROR: Failed to provision $resource: $errors" >&2
        return 1
    fi
    return 0
}

# Replace a placeholder in wrangler.toml (in-place, single occurrence)
replace_placeholder() {
    local placeholder="$1"
    local real_id="$2"

    if [[ "$DRY_RUN" == "true" ]]; then
        echo "    [DRY-RUN] Would replace '$placeholder' with '$real_id' in wrangler.toml"
        return 0
    fi

    if grep -q "$placeholder" "$WRANGLER_TOML"; then
        # Use python3 for safe in-place replacement (avoids sed platform differences)
        python3 - "$WRANGLER_TOML" "$placeholder" "$real_id" <<'PYEOF'
import sys
path, old, new = sys.argv[1], sys.argv[2], sys.argv[3]
with open(path, 'r') as f:
    content = f.read()
if old not in content:
    print(f"  WARN: placeholder '{old}' not found in wrangler.toml", flush=True)
    sys.exit(0)
new_content = content.replace(old, new, 1)
with open(path, 'w') as f:
    f.write(new_content)
PYEOF
        echo "    wrangler.toml: replaced '$placeholder' -> '$real_id'"
    else
        echo "    wrangler.toml: placeholder '$placeholder' not found (already replaced or unexpected)"
    fi
}

# ----------------------------------------------------------------------------
# D1 Database
# ----------------------------------------------------------------------------

provision_d1() {
    local name="$1"
    local placeholder_id="$2"

    echo ""
    echo "--- D1 Database: $name ---"

    local list_resp
    list_resp=$(cf_api GET "/accounts/$CLOUDFLARE_ACCOUNT_ID/d1/database")
    if ! check_success "$list_resp" "D1 list"; then return 1; fi

    local existing_id
    existing_id=$(echo "$list_resp" | python3 -c "
import json, sys
d = json.load(sys.stdin)
for db in d.get('result', []):
    if db.get('name') == '$name':
        print(db.get('uuid',''))
        break
" 2>/dev/null || echo "")

    if [[ -n "$existing_id" ]]; then
        echo "  EXISTS: $name (id=$existing_id)"
        replace_placeholder "$placeholder_id" "$existing_id"
        echo "  D1_ID=$existing_id"
        return 0
    fi

    echo "  Creating D1 database: $name"
    if [[ "$DRY_RUN" == "true" ]]; then
        echo "  [DRY-RUN] Would POST to create D1 database"
        return 0
    fi

    local create_resp
    create_resp=$(cf_api POST "/accounts/$CLOUDFLARE_ACCOUNT_ID/d1/database" \
        "{\"name\":\"$name\"}")
    if ! check_success "$create_resp" "D1 $name"; then return 1; fi

    local new_id
    new_id=$(echo "$create_resp" | python3 -c "
import json, sys
d = json.load(sys.stdin)
print(d.get('result', {}).get('uuid', ''))
" 2>/dev/null || echo "")

    if [[ -z "$new_id" ]]; then
        echo "ERROR: Could not parse D1 UUID from response: $create_resp" >&2
        return 1
    fi

    echo "  CREATED: $name (id=$new_id)"
    replace_placeholder "$placeholder_id" "$new_id"
    echo "  D1_ID=$new_id"
}

# ----------------------------------------------------------------------------
# KV Namespace
# ----------------------------------------------------------------------------

provision_kv() {
    local title="$1"
    local placeholder_id="$2"

    echo ""
    echo "--- KV Namespace: $title ---"

    local list_resp
    list_resp=$(cf_api GET "/accounts/$CLOUDFLARE_ACCOUNT_ID/storage/kv/namespaces")
    if ! check_success "$list_resp" "KV list"; then return 1; fi

    local existing_id
    existing_id=$(echo "$list_resp" | python3 -c "
import json, sys
d = json.load(sys.stdin)
for ns in d.get('result', []):
    if ns.get('title') == '$title':
        print(ns.get('id', ''))
        break
" 2>/dev/null || echo "")

    if [[ -n "$existing_id" ]]; then
        echo "  EXISTS: $title (id=$existing_id)"
        replace_placeholder "$placeholder_id" "$existing_id"
        echo "  KV_ID=$existing_id"
        return 0
    fi

    echo "  Creating KV namespace: $title"
    if [[ "$DRY_RUN" == "true" ]]; then
        echo "  [DRY-RUN] Would POST to create KV namespace"
        return 0
    fi

    local create_resp
    create_resp=$(cf_api POST "/accounts/$CLOUDFLARE_ACCOUNT_ID/storage/kv/namespaces" \
        "{\"title\":\"$title\"}")
    if ! check_success "$create_resp" "KV $title"; then return 1; fi

    local new_id
    new_id=$(echo "$create_resp" | python3 -c "
import json, sys
d = json.load(sys.stdin)
print(d.get('result', {}).get('id', ''))
" 2>/dev/null || echo "")

    if [[ -z "$new_id" ]]; then
        echo "ERROR: Could not parse KV ID from response: $create_resp" >&2
        return 1
    fi

    echo "  CREATED: $title (id=$new_id)"
    replace_placeholder "$placeholder_id" "$new_id"
    echo "  KV_ID=$new_id"
}

# ----------------------------------------------------------------------------
# R2 Bucket
# ----------------------------------------------------------------------------

# CF R2 locationHint codes (per CF API — lowercase required):
#   wnam  — Western North America
#   enam  — Eastern North America
#   weur  — Western Europe
#   eeur  — Eastern Europe
#   apac  — Asia Pacific
#   oc    — Oceania
#   auto  — Cloudflare-chosen optimal region

provision_r2() {
    local bucket_name="$1"
    local location_hint="${2:-}"   # empty = no hint (global)

    echo ""
    echo "--- R2 Bucket: $bucket_name (location=${location_hint:-global}) ---"

    # Check if bucket exists via individual GET (list API omits location field)
    local get_resp
    get_resp=$(cf_api GET "/accounts/$CLOUDFLARE_ACCOUNT_ID/r2/buckets/$bucket_name")
    local get_ok
    get_ok=$(echo "$get_resp" | python3 -c "import json,sys; d=json.load(sys.stdin); print(d.get('success',''))" 2>/dev/null || echo "")

    if [[ "$get_ok" == "True" || "$get_ok" == "true" ]]; then
        local existing_location
        existing_location=$(echo "$get_resp" | python3 -c "
import json, sys
d = json.load(sys.stdin)
print(d.get('result', {}).get('location', 'no-location'))
" 2>/dev/null || echo "no-location")

        echo "  EXISTS: $bucket_name (location=$existing_location)"
        # Validate region is correct if we expected a specific location (case-insensitive: CF returns uppercase)
        local existing_lower
        existing_lower=$(echo "$existing_location" | tr '[:upper:]' '[:lower:]')
        local hint_lower_chk
        hint_lower_chk=$(echo "$location_hint" | tr '[:upper:]' '[:lower:]')
        if [[ -n "$location_hint" && "$existing_lower" != "$hint_lower_chk" ]]; then
            echo "HALT: Bucket $bucket_name exists but location=$existing_location, expected=$location_hint" >&2
            echo "HALT: Manual triage required — name collision with wrong region (hard pause trigger #4)." >&2
            return 1
        fi
        echo "  R2_BUCKET=$bucket_name location=$existing_location"
        return 0
    fi

    echo "  Creating R2 bucket: $bucket_name"
    if [[ "$DRY_RUN" == "true" ]]; then
        echo "  [DRY-RUN] Would POST to create R2 bucket"
        return 0
    fi

    local body
    if [[ -n "$location_hint" ]]; then
        body="{\"name\":\"$bucket_name\",\"locationHint\":\"$location_hint\"}"
    else
        body="{\"name\":\"$bucket_name\"}"
    fi

    local create_resp
    create_resp=$(cf_api POST "/accounts/$CLOUDFLARE_ACCOUNT_ID/r2/buckets" "$body")
    if ! check_success "$create_resp" "R2 $bucket_name"; then return 1; fi

    local actual_location
    actual_location=$(echo "$create_resp" | python3 -c "
import json, sys
d = json.load(sys.stdin)
print(d.get('result', {}).get('location', 'no-location'))
" 2>/dev/null || echo "no-location")

    # Hard pause: if region pinning silently failed (case-insensitive compare: CF returns uppercase)
    local actual_lower
    actual_lower=$(echo "$actual_location" | tr '[:upper:]' '[:lower:]')
    local hint_lower
    hint_lower=$(echo "$location_hint" | tr '[:upper:]' '[:lower:]')
    if [[ -n "$location_hint" && "$actual_lower" != "$hint_lower" ]]; then
        echo "HALT: R2 bucket $bucket_name created but location=$actual_location, expected=$location_hint" >&2
        echo "HALT: Region pinning failed or CF silently ignored it (hard pause trigger #3)." >&2
        return 1
    fi

    echo "  CREATED: $bucket_name (location=$actual_location)"
    echo "  R2_BUCKET=$bucket_name location=$actual_location"
}

# ----------------------------------------------------------------------------
# Main provisioning sequence
# ----------------------------------------------------------------------------

echo ""
echo "=== Step 1: D1 Database ==="
provision_d1 "corelink-prod-d1" "PLACEHOLDER_PROD_D1_CONFIG_DB_ID"

echo ""
echo "=== Step 2: KV Namespaces ==="
# Spec names → wrangler.toml prod binding mapping:
#   corelink-prod-jwks-kv       → CLERK_JWKS_KV    (PLACEHOLDER_PROD_CLERK_JWKS_KV_ID)
#   corelink-prod-cache-kv      → METADATA_KV       (PLACEHOLDER_PROD_METADATA_KV_ID)
#   corelink-prod-rate-limit-kv → NEGATIVE_CACHE_KV (PLACEHOLDER_PROD_NEG_CACHE_KV_ID)
#   corelink-prod-session-kv    → no current wrangler.toml placeholder (provisioned for Phase B)
#   corelink-prod-pilot-signup-kv → no current wrangler.toml placeholder (provisioned for Phase B)
provision_kv "corelink-prod-jwks-kv"       "PLACEHOLDER_PROD_CLERK_JWKS_KV_ID"
provision_kv "corelink-prod-cache-kv"      "PLACEHOLDER_PROD_METADATA_KV_ID"
provision_kv "corelink-prod-rate-limit-kv" "PLACEHOLDER_PROD_NEG_CACHE_KV_ID"
provision_kv "corelink-prod-session-kv"    "NO_WRANGLER_PLACEHOLDER"
provision_kv "corelink-prod-pilot-signup-kv" "NO_WRANGLER_PLACEHOLDER"

echo ""
echo "=== Step 3: R2 Buckets ==="
# CAS (canonical, no specific region required — global)
provision_r2 "corelink-cas-prod" ""

# AC per-region (INV-DATA-RESIDENCY: pinned by locationHint — lowercase per CF API)
# CF locationHint codes as of 2026-05-26: wnam, enam, weur, eeur, apac, oc, auto
#   sam = enam (closest: Eastern N. America; CF has no WLAM/SAM code)
#   iad = enam (Eastern North America — Washington DC)
#   lhr = weur (Western Europe — London)
#   nrt = apac (Asia Pacific — Tokyo)
#   syd = oc   (Oceania — Sydney)
# CF R2 does not have a WLAM/SAM-specific location code as of 2026-05-26.
# Available codes: wnam, enam, weur, eeur, apac, oc, auto.
# SAM (São Paulo) is mapped to enam (Eastern North America) — closest available
# CF R2 region per current platform capability. Documented in Phase C audit §2.
provision_r2 "corelink-ac-sam" "enam"
provision_r2 "corelink-ac-iad" "enam"
provision_r2 "corelink-ac-lhr" "weur"
provision_r2 "corelink-ac-nrt" "apac"
provision_r2 "corelink-ac-syd" "oc"

echo ""
echo "============================================================"
echo " Provisioning complete."
echo " Re-run this script to verify idempotency (should be no-op)."
echo "============================================================"
