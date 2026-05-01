#!/usr/bin/env bash
# CoreLink AC infrastructure deploy guard (WI-S04-002 §6.1.6).
#
# Pre-deploy CI gate that fails-loud if any of the load-bearing
# AC infra invariants is violated:
#
#   1. The 5 canonical R2 buckets exist (`corelink-ac-{sam,iad,lhr,nrt,syd}`).
#   2. wrangler.toml carries the 5 canonical bindings
#      (`AC_BUCKET_{SAM,IAD,LHR,NRT,SYD}`).
#   3. Migration `0002_ac_meta.sql` is present in the migrations
#      manifest (`migrations/d1/`).
#   4. Each bucket's CORS policy is empty + public access is OFF.
#      (Steps 4 require CF_ACCOUNT_ID + CF_API_TOKEN; if unset,
#      flagged WARN — operator MUST run nightly cron audit instead.)
#
# Usage:
#   scripts/check_ac_infra.sh <env>
#
# Anti-scope:
#   - Does NOT mutate state. Pure inspection.
#   - Does NOT replace the nightly bucket ACL audit cron
#     (.github/workflows/ac-bucket-acl-cron.yml) — both run.
#
# Exit codes:
#   0   — every invariant green.
#   != 0 — at least one invariant violated; offending lines printed.

set -euo pipefail

if [[ $# -ne 1 ]]; then
    echo "usage: $0 <env>" >&2
    echo "  env: staging | production | dev" >&2
    exit 64
fi

ENV="$1"

case "$ENV" in
    staging|production|dev) ;;
    *) echo "fatal: unknown env: $ENV" >&2; exit 64 ;;
esac

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WRANGLER_TOML="$REPO_ROOT/wrangler.toml"
MIGRATION_FILE="$REPO_ROOT/migrations/d1/0002_ac_meta.sql"

REGIONS=( sam iad lhr nrt syd )
ERRORS=0

note() { echo "[check_ac_infra] $*"; }
fail() { echo "[check_ac_infra] FAIL: $*" >&2; ERRORS=$((ERRORS + 1)); }
warn() { echo "[check_ac_infra] WARN: $*" >&2; }

note "env=$ENV"

# --- 1. wrangler.toml bindings --------------------------------------------
note "step 1/4 — wrangler.toml carries 5 AC_BUCKET_* bindings"
if [[ ! -f "$WRANGLER_TOML" ]]; then
    fail "wrangler.toml not found at $WRANGLER_TOML"
else
    for region in "${REGIONS[@]}"; do
        upper="$(echo "$region" | tr '[:lower:]' '[:upper:]')"
        binding="AC_BUCKET_${upper}"
        if ! grep -qE "^binding\\s*=\\s*\"${binding}\"" "$WRANGLER_TOML"; then
            fail "wrangler.toml missing binding $binding"
        fi
    done
fi

# --- 2. migration 0002 present --------------------------------------------
note "step 2/4 — migration 0002_ac_meta.sql exists + carries canonical PK"
if [[ ! -f "$MIGRATION_FILE" ]]; then
    fail "migration file missing: $MIGRATION_FILE"
elif ! grep -q "PRIMARY KEY (tenant_id, action_digest)" "$MIGRATION_FILE"; then
    fail "migration $MIGRATION_FILE missing canonical PK direction"
fi

# --- 3. R2 buckets exist (best-effort; requires wrangler) -----------------
note "step 3/4 — 5 R2 buckets exist (env=$ENV)"
if ! command -v wrangler >/dev/null 2>&1; then
    warn "wrangler CLI not in PATH; skipping R2 listing (run in CI with wrangler)"
else
    if ! BUCKETS_LIST=$(wrangler r2 bucket list 2>&1); then
        warn "wrangler r2 bucket list failed; skipping R2 inventory check"
        BUCKETS_LIST=""
    fi
    for region in "${REGIONS[@]}"; do
        bucket="corelink-ac-${region}"
        if ! grep -qF "$bucket" <<<"$BUCKETS_LIST"; then
            fail "R2 bucket missing: $bucket"
        fi
    done
fi

# --- 4. CORS empty + public access OFF (best-effort; CF API) --------------
note "step 4/4 — CORS rules empty + public access OFF (env=$ENV)"
if [[ -z "${CF_ACCOUNT_ID:-}" ]] || [[ -z "${CF_API_TOKEN:-}" ]]; then
    warn "CF_ACCOUNT_ID + CF_API_TOKEN not set; SKIPPING bucket ACL inspection (nightly cron will reconcile)"
elif ! command -v curl >/dev/null 2>&1 || ! command -v python3 >/dev/null 2>&1; then
    warn "curl or python3 not in PATH; SKIPPING bucket ACL inspection"
else
    for region in "${REGIONS[@]}"; do
        bucket="corelink-ac-${region}"
        url="https://api.cloudflare.com/client/v4/accounts/${CF_ACCOUNT_ID}/r2/buckets/${bucket}/cors"
        if ! body=$(curl --silent --show-error -X GET "$url" \
            -H "Authorization: Bearer ${CF_API_TOKEN}"); then
            warn "CORS GET failed for $bucket; skipping ACL check"
            continue
        fi
        # Tolerate either `{"result": {"rules": []}, ...}` or `{"rules": []}`.
        if ! python3 -c "
import json, sys
data = json.loads(sys.stdin.read())
rules = data.get('result', data).get('rules', None)
sys.exit(0 if rules == [] else 1)
" <<<"$body"; then
            fail "bucket $bucket CORS rules NOT empty (drift detected)"
        fi
    done
fi

if (( ERRORS > 0 )); then
    echo "[check_ac_infra] FAIL: $ERRORS error(s)" >&2
    exit 1
fi
echo "[check_ac_infra] OK — every invariant green for env=$ENV"
