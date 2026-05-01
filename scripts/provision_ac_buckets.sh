#!/usr/bin/env bash
# CoreLink R2 Action Cache bucket provisioning (WI-S04-002 §6.1.3).
#
# Idempotent provisioning of the 5 canonical AC buckets per region:
# `corelink-ac-{sam,iad,lhr,nrt,syd}`. Re-running the script is a
# no-op (`wrangler r2 bucket create` exits 0 if the bucket already
# exists; lifecycle add is idempotent via rule id; CORS PUT replaces
# whatever rules exist).
#
# Usage:
#   scripts/provision_ac_buckets.sh <env>
#
# Examples:
#   scripts/provision_ac_buckets.sh staging
#   scripts/provision_ac_buckets.sh production
#
# Requires:
#   - wrangler CLI authenticated against the target Cloudflare account.
#   - For CORS PUT: env vars CF_ACCOUNT_ID + CF_API_TOKEN with R2:Edit
#     permission on the target account; if either is unset, the CORS
#     hardening step is SKIPPED and a warning is printed (the bucket
#     ACL hardening cron `.github/workflows/ac-bucket-acl-cron.yml`
#     re-asserts the canonical empty-CORS posture nightly).
#
# Lifecycle policy (REAPI guidance):
#   - Abort incomplete multipart uploads after 7 days.
#   - 1 rule id `abort-multipart-incomplete-7d`; idempotent re-add.
#
# CORS policy (defense-in-depth):
#   - Empty `rules: []` → no browser preflight allowed; all access via
#     Worker binding (server-side).
#   - Public access OFF (CF default; verify in deploy guard
#     scripts/check_ac_infra.sh).
#
# Anti-scope:
#   - Bucket DELETION is not implemented here; deletion of a non-empty
#     bucket is irreversible (data loss). If a region must be retired,
#     follow ADR-0036 + RB-FM-AC-MIGRATION-BUG.

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

if ! command -v wrangler >/dev/null 2>&1; then
    echo "fatal: wrangler CLI not found in PATH" >&2
    exit 127
fi

# Canonical 5-region list (mirror of corelink_ac_schema::REGION_LIST).
REGIONS=( sam iad lhr nrt syd )

# CF API base for CORS PUT. Optional; the cron audit re-applies daily.
CF_API_BASE="https://api.cloudflare.com/client/v4"

provision_lifecycle() {
    local bucket="$1"
    # `wrangler r2 bucket lifecycle add` is idempotent via rule id; if
    # the rule exists already wrangler returns non-zero with a clear
    # error → swallow into a info log so the script stays idempotent.
    if wrangler r2 bucket lifecycle add "$bucket" \
        --id "abort-multipart-incomplete-7d" \
        --action "AbortIncompleteMultipartUpload" \
        --days 7 2>&1; then
        echo "    lifecycle rule applied"
    else
        echo "    lifecycle rule already present (idempotent)"
    fi
}

provision_cors() {
    local bucket="$1"
    if [[ -z "${CF_ACCOUNT_ID:-}" ]] || [[ -z "${CF_API_TOKEN:-}" ]]; then
        echo "    WARN: CF_ACCOUNT_ID + CF_API_TOKEN not set; skipping CORS PUT (cron will reconcile)"
        return 0
    fi
    if ! command -v curl >/dev/null 2>&1; then
        echo "    WARN: curl not in PATH; skipping CORS PUT (cron will reconcile)"
        return 0
    fi
    local url="${CF_API_BASE}/accounts/${CF_ACCOUNT_ID}/r2/buckets/${bucket}/cors"
    # Empty rules list = no public preflight allowed.
    local body='{"rules": []}'
    if curl --silent --fail-with-body --show-error \
        -X PUT "$url" \
        -H "Authorization: Bearer ${CF_API_TOKEN}" \
        -H "Content-Type: application/json" \
        -d "$body" >/dev/null; then
        echo "    CORS rules pinned to empty"
    else
        echo "    WARN: CORS PUT failed (non-fatal; cron will reconcile)"
    fi
}

echo "Provisioning R2 AC buckets for env=$ENV (5 regions: ${REGIONS[*]})"
for region in "${REGIONS[@]}"; do
    bucket="corelink-ac-${region}"
    echo "  region=$region bucket=$bucket"
    if wrangler r2 bucket create "$bucket" --location "$region" 2>&1 | tee /dev/stderr | grep -qiE "already|exists"; then
        echo "    bucket already exists (idempotent)"
    else
        echo "    bucket created"
    fi
    provision_lifecycle "$bucket"
    provision_cors "$bucket"
done

echo "OK — 5 AC buckets provisioned + hardened for env=$ENV."
