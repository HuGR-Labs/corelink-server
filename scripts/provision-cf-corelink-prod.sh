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
# SAFETY: Default mode is DRY-RUN. Pass --live to make real API calls.
#         Only the account Owner should run --live.
#
# Usage:
#   bash scripts/provision-cf-corelink-prod.sh [--dry-run] [--validate-token]
#   bash scripts/provision-cf-corelink-prod.sh --live [--validate-token]
#
# Exit codes:
#   0 — all resources provisioned or already exist (or dry-run completed)
#   1 — provisioning failure or token-scope check failed
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

# Default: DRY-RUN (safe). Operator must pass --live to execute real API calls.
DRY_RUN=true
VALIDATE_TOKEN=false

for arg in "$@"; do
    case "$arg" in
        --live)
            DRY_RUN=false
            ;;
        --dry-run)
            DRY_RUN=true
            ;;
        --validate-token)
            VALIDATE_TOKEN=true
            ;;
        *)
            echo "Unknown argument: $arg" >&2
            echo "Usage: $0 [--dry-run|--live] [--validate-token]" >&2
            exit 2
            ;;
    esac
done

if [[ "$DRY_RUN" == "true" ]]; then
    echo "[DRY-RUN] No real resources will be created. Pass --live to execute."
fi

# ----------------------------------------------------------------------------
# Load credentials
# ----------------------------------------------------------------------------

# Source only the CF_ vars — do not export arbitrary env.
CLOUDFLARE_API_TOKEN=""
CLOUDFLARE_ACCOUNT_ID=""

if [[ -f "$ENV_FILE" ]]; then
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
fi

if [[ -z "$CLOUDFLARE_API_TOKEN" || -z "$CLOUDFLARE_ACCOUNT_ID" ]]; then
    if [[ "$VALIDATE_TOKEN" == "true" ]]; then
        echo "ERROR: CLOUDFLARE_API_TOKEN and CLOUDFLARE_ACCOUNT_ID must be set in $ENV_FILE" >&2
        echo "       Token-scope check requires valid credentials." >&2
        exit 1
    fi
    if [[ "$DRY_RUN" == "false" ]]; then
        echo "FATAL: CLOUDFLARE_API_TOKEN and CLOUDFLARE_ACCOUNT_ID must be set in $ENV_FILE" >&2
        exit 2
    fi
    # Dry-run without credentials: allowed — print plan only.
    CLOUDFLARE_ACCOUNT_ID="${CLOUDFLARE_ACCOUNT_ID:-<not-set>}"
fi

if ! command -v curl >/dev/null 2>&1; then
    echo "FATAL: curl not found in PATH" >&2
    exit 2
fi
if ! command -v python3 >/dev/null 2>&1; then
    echo "FATAL: python3 not found in PATH (needed for JSON parsing)" >&2
    exit 2
fi

# ----------------------------------------------------------------------------
# Optional: token-scope pre-flight via wrangler whoami
# ----------------------------------------------------------------------------

if [[ "$VALIDATE_TOKEN" == "true" ]]; then
    if ! command -v wrangler >/dev/null 2>&1; then
        echo "ERROR: wrangler CLI not found in PATH — required for --validate-token" >&2
        exit 1
    fi
    echo "[VALIDATE-TOKEN] Running wrangler whoami to verify token scope..."
    # Export the token so wrangler picks it up
    CLOUDFLARE_API_TOKEN_ORIG="$CLOUDFLARE_API_TOKEN"
    export CLOUDFLARE_API_TOKEN="$CLOUDFLARE_API_TOKEN_ORIG"
    if ! wrangler whoami 2>&1; then
        echo "ERROR: wrangler whoami failed — check that your token has:" >&2
        echo "  - Account:D1:Edit" >&2
        echo "  - Account:KV:Edit (Workers KV Storage:Edit)" >&2
        echo "  - Account:R2:Edit (R2 Storage:Edit)" >&2
        exit 1
    fi
    echo "[VALIDATE-TOKEN] wrangler whoami succeeded. Token appears valid."
    echo "[VALIDATE-TOKEN] NOTE: wrangler whoami does not enumerate scope grants;" >&2
    echo "  verify manually that the token has D1:Edit + KV:Edit + R2:Edit." >&2
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

    if [[ "$DRY_RUN" == "true" ]]; then
        echo "  [DRY-RUN] Would list D1 databases and create '$name' if absent"
        echo "  [DRY-RUN] Would replace wrangler.toml '$placeholder_id' with real UUID"
        return 0
    fi

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

    if [[ "$DRY_RUN" == "true" ]]; then
        echo "  [DRY-RUN] Would list KV namespaces and create '$title' if absent"
        if [[ "$placeholder_id" != "NO_WRANGLER_PLACEHOLDER" ]]; then
            echo "  [DRY-RUN] Would replace wrangler.toml '$placeholder_id' with real ID"
        else
            echo "  [DRY-RUN] No wrangler.toml placeholder — ID echoed to stdout only (Phase B)"
        fi
        return 0
    fi

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

    if [[ "$DRY_RUN" == "true" ]]; then
        echo "  [DRY-RUN] Would GET/check R2 bucket '$bucket_name' and create if absent"
        if [[ -n "$location_hint" ]]; then
            echo "  [DRY-RUN] Would set locationHint=$location_hint (INV-DATA-RESIDENCY)"
        fi
        return 0
    fi

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

if [[ "$DRY_RUN" == "true" ]]; then
    echo ""
    echo "=== DRY-RUN: 12 resources this script WOULD provision ==="
    echo ""
    echo "  D1 Databases (1):"
    echo "    corelink-prod-d1  [wrangler.toml placeholder: PLACEHOLDER_D1_CONFIG_DB_ID]"
    echo ""
    echo "  KV Namespaces (5):"
    echo "    corelink-prod-jwks-kv         [binding: CLERK_JWKS_KV    → PLACEHOLDER_CLERK_JWKS_KV_ID]"
    echo "    corelink-prod-cache-kv        [binding: METADATA_KV      → TODO_KV_NAMESPACE_ID]"
    echo "    corelink-prod-rate-limit-kv   [binding: NEGATIVE_CACHE_KV → PLACEHOLDER_NEG_CACHE_KV_ID]"
    echo "    corelink-prod-session-kv      [no wrangler.toml placeholder — Phase B]"
    echo "    corelink-prod-pilot-signup-kv [no wrangler.toml placeholder — Phase B]"
    echo ""
    echo "  R2 Buckets (6):"
    echo "    corelink-cas-prod  [global, no locationHint]"
    echo "    corelink-ac-sam    [locationHint=enam (Eastern North America — CF R2 closest to SAM)]"
    echo "    corelink-ac-iad    [locationHint=enam (Eastern North America — Washington DC)]"
    echo "    corelink-ac-lhr    [locationHint=weur (Western Europe — London)]"
    echo "    corelink-ac-nrt    [locationHint=apac (Asia Pacific — Tokyo)]"
    echo "    corelink-ac-syd    [locationHint=oc   (Oceania — Sydney)]"
    echo ""
    echo "  No API calls are made in dry-run. Pass --live to execute (Owner-only)."
    echo "======================================================="
fi

echo ""
echo "=== Step 1: D1 Database ==="
# HIGH-2 fix: placeholder names aligned to wrangler.toml authoritative values.
# wrangler.toml [d1_databases] dev binding has: database_id = "PLACEHOLDER_D1_CONFIG_DB_ID"
provision_d1 "corelink-prod-d1" "PLACEHOLDER_D1_CONFIG_DB_ID"

echo ""
echo "=== Step 2: KV Namespaces ==="
# HIGH-2 fix: placeholder names aligned to wrangler.toml authoritative values.
# wrangler.toml [kv_namespaces] dev bindings:
#   CLERK_JWKS_KV      id = "PLACEHOLDER_CLERK_JWKS_KV_ID"
#   METADATA_KV        id = "TODO_KV_NAMESPACE_ID"
#   NEGATIVE_CACHE_KV  id = "PLACEHOLDER_NEG_CACHE_KV_ID"
#
# Spec names → wrangler.toml dev binding → placeholder (authoritative):
#   corelink-prod-jwks-kv       → CLERK_JWKS_KV    → PLACEHOLDER_CLERK_JWKS_KV_ID
#   corelink-prod-cache-kv      → METADATA_KV       → TODO_KV_NAMESPACE_ID
#   corelink-prod-rate-limit-kv → NEGATIVE_CACHE_KV → PLACEHOLDER_NEG_CACHE_KV_ID
#   corelink-prod-session-kv    → no current wrangler.toml placeholder (provisioned for Phase B)
#   corelink-prod-pilot-signup-kv → no current wrangler.toml placeholder (provisioned for Phase B)
provision_kv "corelink-prod-jwks-kv"       "PLACEHOLDER_CLERK_JWKS_KV_ID"
provision_kv "corelink-prod-cache-kv"      "TODO_KV_NAMESPACE_ID"
provision_kv "corelink-prod-rate-limit-kv" "PLACEHOLDER_NEG_CACHE_KV_ID"
provision_kv "corelink-prod-session-kv"    "NO_WRANGLER_PLACEHOLDER"
provision_kv "corelink-prod-pilot-signup-kv" "NO_WRANGLER_PLACEHOLDER"

echo ""
echo "=== Step 3: R2 Buckets ==="
# CAS (canonical, no specific region required — global)
provision_r2 "corelink-cas-prod" ""

# Turbo (Turborepo remote-cache artifact store). `R2_TURBO_BUCKET` defaults to
# `corelink-turbo-prod` (storage/r2_kv.rs) and is unset across all envs, so this
# ONE bucket backs every env. It was previously NEVER provisioned → every
# `GET/PUT /v8/artifacts/*` 500'd ("internal", missing-bucket S3 error) while
# /status still 200'd — Turborepo was non-functional in prod. Caught by the e2e
# user-journey suite (tests/e2e-user-journeys), fixed by creating this bucket.
provision_r2 "corelink-turbo-prod" ""

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
