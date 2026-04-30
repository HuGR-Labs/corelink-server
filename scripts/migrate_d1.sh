#!/usr/bin/env bash
# CoreLink D1 migration runner (WI-S01-004 §6.1.4).
#
# Applies every `migrations/d1/NNNN_*.sql` file in numeric order against the
# D1 instance bound to the given environment + region. Idempotent — every
# canonical migration uses `CREATE TABLE IF NOT EXISTS` / `CREATE INDEX IF
# NOT EXISTS`, so applying twice is a no-op.
#
# Usage:
#   scripts/migrate_d1.sh <env> <region>
#
# Examples:
#   scripts/migrate_d1.sh staging wnam
#   scripts/migrate_d1.sh production weur
#
# Requires:
#   - wrangler CLI authenticated against the target Cloudflare account.
#   - The D1 database binding `corelink_d1_<region>` declared in wrangler.toml
#     for the target environment.
#
# Anti-scope (WI §6.2): this script DOES NOT manage Postgres (Neon)
# migrations — those use sqlx. D1 (this script) and Postgres (sqlx) are
# distinct migration domains and never share files.

set -euo pipefail

if [[ $# -ne 2 ]]; then
    echo "usage: $0 <env> <region>" >&2
    echo "  env:    staging | production | dev" >&2
    echo "  region: wnam | weur | sam" >&2
    exit 64
fi

ENV="$1"
REGION="$2"
DB_BINDING="corelink_d1_${REGION}"
MIGRATIONS_DIR="$(cd "$(dirname "$0")/.." && pwd)/migrations/d1"

if [[ ! -d "$MIGRATIONS_DIR" ]]; then
    echo "fatal: migrations dir not found: $MIGRATIONS_DIR" >&2
    exit 70
fi

case "$ENV" in
    staging|production|dev) ;;
    *) echo "fatal: unknown env: $ENV" >&2; exit 64 ;;
esac

case "$REGION" in
    wnam|weur|sam) ;;
    *) echo "fatal: unknown region: $REGION" >&2; exit 64 ;;
esac

if ! command -v wrangler >/dev/null 2>&1; then
    echo "fatal: wrangler CLI not found in PATH" >&2
    exit 127
fi

echo "Applying D1 migrations to env=$ENV region=$REGION (binding=$DB_BINDING)"
shopt -s nullglob
files=( "$MIGRATIONS_DIR"/*.sql )
shopt -u nullglob
if [[ ${#files[@]} -eq 0 ]]; then
    echo "fatal: no migration files in $MIGRATIONS_DIR" >&2
    exit 71
fi

for sql in "${files[@]}"; do
    name="$(basename "$sql")"
    echo "  applying $name"
    wrangler d1 execute "$DB_BINDING" --env "$ENV" --file "$sql" --remote
done
echo "OK — D1 schema for env=$ENV region=$REGION up to date."
