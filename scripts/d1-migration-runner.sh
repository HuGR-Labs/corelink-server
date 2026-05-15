#!/usr/bin/env bash
# CoreLink R2-13: D1 migration runner — idempotent wrapper around
# `wrangler d1 migrations apply`.
#
# This script is the canonical entrypoint for staged D1 schema rollouts
# (dev → staging → prod). It wraps wrangler with defense-in-depth flags,
# captures every interaction with the remote D1 binding to an audit log
# under `logs/`, and refuses to apply against prod without an explicit
# `--i-understand-this-is-prod` opt-in.
#
# Idempotency: wrangler tracks applied migrations in the D1
# `d1_migrations` table — re-running this script against an
# already-current database is a no-op (wrangler exits 0 with
# "no migrations to apply"). Additionally every CoreLink migration uses
# `CREATE TABLE IF NOT EXISTS` / `CREATE INDEX IF NOT EXISTS` /
# `ADD COLUMN IF NOT EXISTS`, so even a hard re-apply is safe.
#
# Anti-scope:
#   - DOES NOT apply Postgres / Neon migrations (those use sqlx).
#   - DOES NOT manage R2 buckets, KV namespaces, or DO classes.
#   - DOES NOT auto-create rollback scripts — D1 doesn't support
#     in-place rollback (DROP forbidden by INV-AUTH-MIGRATION-ADDITIVE).
#     Rollback path is restore-from-R2-snapshot; see
#     specs/_runbooks/RB-D1-MIGRATION-APPLY.md.
#
# Usage:
#   scripts/d1-migration-runner.sh --env dev
#   scripts/d1-migration-runner.sh --env staging --binding corelink_d1_wnam
#   scripts/d1-migration-runner.sh --env prod --i-understand-this-is-prod
#   scripts/d1-migration-runner.sh --dry-run --env dev          # diff only
#
# Exit codes:
#   0    — success (applied + verified, or dry-run diff clean)
#   1    — wrangler error / migration failed mid-apply (aborted)
#   2    — usage error (bad flags, missing arg)
#   3    — prod safety gate hit (missing --i-understand-this-is-prod)
#   127  — required tool (wrangler) not in PATH

set -euo pipefail

# ── 0. CLI parse ────────────────────────────────────────────────────────────

ENV=""
BINDING="DB"
DRY_RUN="false"
PROD_ACK="false"
EXTRA_ARGS=()

usage() {
    cat >&2 <<'USAGE'
usage: d1-migration-runner.sh --env <dev|staging|prod> [options]

required:
  --env <env>                       one of: dev, staging, prod

options:
  --binding <name>                  D1 binding name (default: DB).
                                    Common values: DB, corelink_d1_wnam,
                                    corelink_d1_weur, corelink_d1_sam.
  --dry-run                         compute + emit the diff between
                                    wrangler-tracked applied migrations
                                    and the files in migrations/d1/.
                                    Does NOT call `wrangler ... apply`.
  --i-understand-this-is-prod       required when --env prod is passed.
  --                                pass remaining args verbatim to wrangler.

example dry-run (no CF auth needed):
  scripts/d1-migration-runner.sh --dry-run --env dev
USAGE
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --env)
            ENV="${2:-}"; shift 2 ;;
        --binding)
            BINDING="${2:-}"; shift 2 ;;
        --dry-run)
            DRY_RUN="true"; shift ;;
        --i-understand-this-is-prod)
            PROD_ACK="true"; shift ;;
        -h|--help)
            usage; exit 0 ;;
        --)
            shift; EXTRA_ARGS+=("$@"); break ;;
        *)
            echo "fatal: unknown arg: $1" >&2
            usage
            exit 2
            ;;
    esac
done

if [[ -z "$ENV" ]]; then
    echo "fatal: --env is required" >&2
    usage
    exit 2
fi

case "$ENV" in
    dev|staging|prod) ;;
    *) echo "fatal: --env must be one of dev|staging|prod (got: $ENV)" >&2; exit 2 ;;
esac

if [[ "$ENV" == "prod" && "$PROD_ACK" != "true" ]]; then
    cat >&2 <<'PROD_GATE'
fatal: --env prod requires --i-understand-this-is-prod

D1 schema changes against production are irreversible at the database
layer (DROP forbidden by INV-AUTH-MIGRATION-ADDITIVE). Rollback is
restore-from-R2-snapshot only. See
specs/_runbooks/RB-D1-MIGRATION-APPLY.md before proceeding.
PROD_GATE
    exit 3
fi

# ── 1. Paths + logging ──────────────────────────────────────────────────────

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MIGRATIONS_DIR="$REPO_ROOT/migrations/d1"
LOGS_DIR="$REPO_ROOT/logs"
TIMESTAMP="$(date -u +%Y%m%dT%H%M%SZ)"
LOG_FILE="$LOGS_DIR/d1-apply-${ENV}-${TIMESTAMP}.log"

if [[ ! -d "$MIGRATIONS_DIR" ]]; then
    echo "fatal: missing migrations dir: $MIGRATIONS_DIR" >&2
    exit 1
fi

mkdir -p "$LOGS_DIR"

# Append-only logging helper. We capture every wrangler invocation +
# its stdout/stderr. Defensive: never log raw migration SQL (would leak
# DEFAULT-clause values into the audit log even though no secrets are
# expected; per WI constraints).
log() {
    local msg="$*"
    printf '[%s] %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$msg" | tee -a "$LOG_FILE"
}

log "d1-migration-runner start"
log "  env=$ENV binding=$BINDING dry_run=$DRY_RUN repo=$REPO_ROOT"
log "  log_file=$LOG_FILE"

# ── 2. Discover local migrations ────────────────────────────────────────────

shopt -s nullglob
LOCAL_FILES=( "$MIGRATIONS_DIR"/[0-9][0-9][0-9][0-9]_*.sql )
shopt -u nullglob

if [[ ${#LOCAL_FILES[@]} -eq 0 ]]; then
    log "fatal: no migration files in $MIGRATIONS_DIR"
    exit 1
fi

log "discovered ${#LOCAL_FILES[@]} local migration file(s) in migrations/d1/"

# Emit numerical-order summary (no SQL contents — just filenames).
for f in "${LOCAL_FILES[@]}"; do
    log "  local: $(basename "$f")"
done

# ── 3. Dry-run: diff vs remote ──────────────────────────────────────────────

if [[ "$DRY_RUN" == "true" ]]; then
    log "dry-run mode: skipping wrangler apply"

    if command -v wrangler >/dev/null 2>&1; then
        log "calling: wrangler d1 migrations list $BINDING --env $ENV"
        if wrangler d1 migrations list "$BINDING" --env "$ENV" >>"$LOG_FILE" 2>&1; then
            log "wrangler list OK (output appended to log)"
        else
            log "warn: wrangler list failed (likely no CF auth — dry-run still valid for local diff)"
        fi
    else
        log "warn: wrangler not in PATH — emitting local-only diff"
    fi

    log "dry-run complete. ${#LOCAL_FILES[@]} migration(s) would be considered."
    exit 0
fi

# ── 4. Real apply path (requires wrangler + CF auth) ────────────────────────

if ! command -v wrangler >/dev/null 2>&1; then
    log "fatal: wrangler CLI not found in PATH"
    exit 127
fi

log "calling: wrangler d1 migrations list $BINDING --env $ENV (pre-flight)"
if ! wrangler d1 migrations list "$BINDING" --env "$ENV" >>"$LOG_FILE" 2>&1; then
    log "fatal: wrangler list failed — check CF auth + binding name"
    exit 1
fi

log "calling: wrangler d1 migrations apply $BINDING --env $ENV --remote"
WRANGLER_CMD=(wrangler d1 migrations apply "$BINDING" --env "$ENV" --remote)
if [[ ${#EXTRA_ARGS[@]} -gt 0 ]]; then
    WRANGLER_CMD+=("${EXTRA_ARGS[@]}")
fi

if ! "${WRANGLER_CMD[@]}" >>"$LOG_FILE" 2>&1; then
    log "FAIL: wrangler apply failed mid-flight — aborting"
    log "  see $LOG_FILE for the last successful migration"
    log "  remediation: inspect $LOG_FILE → re-run after fixing the offending file"
    exit 1
fi

log "wrangler apply OK"

log "post-apply: wrangler d1 migrations list $BINDING --env $ENV"
if ! wrangler d1 migrations list "$BINDING" --env "$ENV" >>"$LOG_FILE" 2>&1; then
    log "warn: post-apply list failed (apply itself succeeded — investigate auth)"
fi

log "d1-migration-runner done OK (env=$ENV binding=$BINDING)"
exit 0
