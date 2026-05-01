#!/usr/bin/env bash
# CoreLink AC staging seeder (WI-S04-002 §6.1.7) — DEV / STAGING ONLY.
#
# Populates ~100 deterministic ac_meta rows in the target D1 binding
# for smoke-testing the GET / UPDATE handler flows. Refuses to run
# against production; the guard is enforced by env env-name match
# AND by the `CORELINK_ENV` env var.
#
# Usage:
#   CORELINK_ENV=staging scripts/seed_ac_staging.sh staging <region>
#
# Anti-scope:
#   - NEVER targets `production` env.
#   - NEVER deletes existing rows.
#   - All seed digests are static fixtures (deterministic; safe to re-run).

set -euo pipefail

if [[ $# -ne 2 ]]; then
    echo "usage: $0 <env> <region>" >&2
    echo "  env:    staging | dev (NOT production)" >&2
    echo "  region: sam | iad | lhr | nrt | syd" >&2
    exit 64
fi

ENV="$1"
REGION="$2"

# Hard guard: refuse to seed prod.
if [[ "$ENV" == "production" ]]; then
    echo "fatal: this seeder is DEV/STAGING only; refusing to run against production" >&2
    exit 65
fi
if [[ "${CORELINK_ENV:-}" != "$ENV" ]]; then
    echo "fatal: CORELINK_ENV must match \$1; got CORELINK_ENV='${CORELINK_ENV:-<unset>}', \$1='$ENV'" >&2
    exit 65
fi

case "$ENV" in
    staging|dev) ;;
    *) echo "fatal: unknown env: $ENV (allowed: staging|dev)" >&2; exit 64 ;;
esac

case "$REGION" in
    sam|iad|lhr|nrt|syd) ;;
    *) echo "fatal: unknown region: $REGION (allowed: sam|iad|lhr|nrt|syd)" >&2; exit 64 ;;
esac

if ! command -v wrangler >/dev/null 2>&1; then
    echo "fatal: wrangler CLI not found in PATH" >&2
    exit 127
fi

DB_BINDING="corelink_d1_${REGION}"
TENANT_ID_STAGING="00000000-0000-0000-0000-0000ace5ee10"  # canonical fixture tenant
NOW_MS="$(python3 -c 'import time; print(int(time.time()*1000))')"
SEED_COUNT="${SEED_COUNT:-100}"

echo "Seeding $SEED_COUNT ac_meta rows in env=$ENV region=$REGION (binding=$DB_BINDING)"
echo "  fixture tenant_id = $TENANT_ID_STAGING"

TMPSQL="$(mktemp -t corelink_seed_ac.XXXXXX.sql)"
trap 'rm -f "$TMPSQL"' EXIT

{
    for i in $(seq 1 "$SEED_COUNT"); do
        action_digest="$(printf 'seed-action-%05d' "$i" | sha256sum | head -c 64)"
        result_hash="$(printf 'seed-result-%05d' "$i" | sha256sum | head -c 64)"
        # 16 raw bytes hex => 32 chars: BLOB literal X'...'.
        tenant_prefix_hex="$(printf '%032s' "$(printf '%x' "$i")" | tr ' ' '0')"
        cat <<SQL
INSERT INTO ac_meta (
    tenant_id, tenant_prefix, path_key_id,
    action_digest, result_hash, blob_refs, blob_refs_count, result_size_bytes,
    created_at, last_hit_at, expires_at,
    sig_key_id, sig_alg, region,
    created_by_pat_id, created_by_request_id
) VALUES (
    '${TENANT_ID_STAGING}', X'${tenant_prefix_hex}', 1,
    '${action_digest}', '${result_hash}', '[]', 0, 64,
    ${NOW_MS}, ${NOW_MS}, NULL,
    1, 'hkdf-sha256', '${REGION}',
    'pat-staging-seeder', 'req-staging-${i}'
)
ON CONFLICT (tenant_id, action_digest) DO UPDATE SET
    last_hit_at = excluded.last_hit_at;
SQL
    done
} > "$TMPSQL"

echo "  applying $TMPSQL ..."
wrangler d1 execute "$DB_BINDING" --env "$ENV" --file "$TMPSQL" --remote
echo "OK — $SEED_COUNT rows seeded into env=$ENV region=$REGION."
