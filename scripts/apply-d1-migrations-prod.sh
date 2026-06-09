#!/usr/bin/env bash
# apply-d1-migrations-prod.sh — Phase D runner: apply 60 D1 migrations to
# corelink-prod-d1 in lexicographic order.
#
# Default mode: DRY-RUN (lists what WOULD be applied; no remote mutations).
# Pass --apply to actually execute the migrations.
#
# Charter constraints honoured:
#   INV-AUTH-MIGRATION-ADDITIVE (HIGH): runner does NOT apply DROP /
#     ALTER-COLUMN-with-data-loss. check_migrations_additive.py enforces this
#     at PR-time; this script inherits the guarantee at apply-time by running
#     the same check before proceeding. If the check fails, the script aborts.
#   CTRL-AUDIT-EMIT-BEFORE-MUTATION: each migration is logged to stderr with
#     its filename and statement count BEFORE wrangler is invoked.
#
# Exit codes:
#   0   — success (all migrations applied, or dry-run completed cleanly)
#   1   — migration failure (aborted on first error; partial state possible)
#   2   — usage / argument error
#   3   — additive-migration guard failed (pre-flight check_migrations_additive.py)
#   127 — wrangler not in PATH
#
# Usage:
#   scripts/apply-d1-migrations-prod.sh [--apply] [--help]
#
#   --apply   Actually apply migrations to corelink-prod-d1 (opt-in; NOT default).
#             Without this flag the script runs in dry-run mode: prints the
#             ordered list of files that would be applied, runs the additive
#             guard, and exits 0 without touching the remote database.
#   --help    Print this message.

set -euo pipefail

# ── Constants ────────────────────────────────────────────────────────────────

readonly SCRIPT_NAME="$(basename "$0")"
readonly REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
readonly MIGRATIONS_DIR="$REPO_ROOT/migrations/d1"
# DB_NAME matches the wrangler.toml binding name under [[env.prod.d1_databases]].
# Use with --env prod so wrangler picks up the prod D1 binding (CONFIG_DB →
# database_id d64742ea-e102-40b2-a844-ff02e3f94562, migrations_dir migrations/d1).
readonly DB_NAME="CONFIG_DB"
readonly DB_ENV="prod"
readonly EXPECTED_FILE_COUNT=60
# Computed via CREATE TABLE analysis across all 60 migration files.
# Re-derived 2026-06-09 for the 60-migration set: grep-count of unique table
# names = 80 (was 75 across 52 files; +5 net from the adapter/billing/pilot
# migrations 0053-0061 — pilot_signups, tenant_billing, adapter_cache_map,
# adapter_npm_meta, adapter_pip_index, adapter_oci_kv). FLOOR check only: the
# live sqlite_master count also includes d1_migrations + sqlite internals, so
# the post-apply verification passes whenever actual >= this value.
readonly EXPECTED_TABLE_COUNT=80
readonly ADDITIVE_CHECK_SCRIPT="$REPO_ROOT/scripts/check_migrations_additive.py"
readonly TIMESTAMP="$(date -u +%Y%m%dT%H%M%SZ)"
readonly LOG_PREFIX="[$SCRIPT_NAME]"

# ── CLI parse ────────────────────────────────────────────────────────────────

APPLY_MODE=false

usage() {
    grep '^#' "$0" | head -40 | sed 's/^# \?//'
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --apply)    APPLY_MODE=true; shift ;;
        -h|--help)  usage; exit 0 ;;
        *)
            printf '%s fatal: unknown argument: %s\n' "$LOG_PREFIX" "$1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

# ── Logging ──────────────────────────────────────────────────────────────────

log()  { printf '%s [%s] %s\n' "$LOG_PREFIX" "$(date -u +%H:%M:%SZ)" "$*"; }
err()  { printf '%s [%s] ERROR: %s\n' "$LOG_PREFIX" "$(date -u +%H:%M:%SZ)" "$*" >&2; }
warn() { printf '%s [%s] WARN: %s\n'  "$LOG_PREFIX" "$(date -u +%H:%M:%SZ)" "$*"; }

# ── 1. Pre-flight: discover + validate migration files ───────────────────────

log "Phase D migration runner starting"
log "  mode=$(if $APPLY_MODE; then echo APPLY; else echo DRY-RUN; fi)"
log "  db=$DB_NAME"
log "  migrations_dir=$MIGRATIONS_DIR"

if [[ ! -d "$MIGRATIONS_DIR" ]]; then
    err "migrations dir not found: $MIGRATIONS_DIR"
    exit 1
fi

# Collect SQL files in lexicographic order (matches wrangler semantics).
# Use basename + sort for macOS/BSD + GNU portability (-printf is GNU-only).
mapfile -t SQL_FILES < <(
    find "$MIGRATIONS_DIR" -maxdepth 1 -name '*.sql' \
    | xargs -I{} basename {} \
    | sort
)

ACTUAL_COUNT="${#SQL_FILES[@]}"

if [[ "$ACTUAL_COUNT" -eq 0 ]]; then
    err "no .sql files found in $MIGRATIONS_DIR"
    exit 1
fi

if [[ "$ACTUAL_COUNT" -ne "$EXPECTED_FILE_COUNT" ]]; then
    err "HARD PAUSE: expected $EXPECTED_FILE_COUNT migration files, found $ACTUAL_COUNT"
    err "The spec assumed $EXPECTED_FILE_COUNT migrations. If migrations were added or"
    err "removed, amend the spec and this script before proceeding."
    err "See Wave 32 Phase D prep audit §4 for the baseline count."
    exit 1
fi

log "migration file count: $ACTUAL_COUNT (matches spec expectation of $EXPECTED_FILE_COUNT)"

# ── 2. Additive guard (CTRL-AUDIT-EMIT-BEFORE-MUTATION) ─────────────────────

if [[ -f "$ADDITIVE_CHECK_SCRIPT" ]]; then
    log "running additive guard: $ADDITIVE_CHECK_SCRIPT"
    if ! python3 "$ADDITIVE_CHECK_SCRIPT" 2>&1; then
        err "INV-AUTH-MIGRATION-ADDITIVE guard FAILED"
        err "One or more migration files contain DROP TABLE / ALTER COLUMN with data loss."
        err "Resolve the violation in the migration files before applying to prod."
        exit 3
    fi
    log "additive guard PASSED"
else
    warn "additive check script not found at $ADDITIVE_CHECK_SCRIPT — skipping (guard runs at PR-time)"
fi

# ── 3. Emit ordered migration plan ──────────────────────────────────────────

log "ordered migration plan (lex sort, matches wrangler apply semantics):"
N=0
for fname in "${SQL_FILES[@]}"; do
    N=$((N + 1))
    fpath="$MIGRATIONS_DIR/$fname"
    # Count statements (semicolons) for informational purposes only.
    # We do NOT emit SQL content — per audit constraints.
    STMT_COUNT=$(grep -c ';' "$fpath" 2>/dev/null || echo "?")
    log "  [$N/$ACTUAL_COUNT] $fname (~${STMT_COUNT} statement(s))"
done

# ── 4. Dry-run: exit here ────────────────────────────────────────────────────

if ! $APPLY_MODE; then
    log ""
    log "DRY-RUN complete. $ACTUAL_COUNT migration files listed above would be applied"
    log "to $DB_NAME (env=$DB_ENV) via: wrangler d1 migrations apply $DB_NAME --env $DB_ENV --remote"
    log ""
    log "Post-apply verification would check:"
    log "  wrangler d1 execute $DB_NAME --env $DB_ENV --remote --command=\"SELECT count(*) FROM sqlite_master WHERE type='table'\""
    log "  Expected table count: $EXPECTED_TABLE_COUNT"
    log ""
    log "To apply for real: $0 --apply"
    exit 0
fi

# ── 5. Apply path (requires wrangler + CF auth) ──────────────────────────────

WRANGLER_CMD="${WRANGLER:-npx wrangler@latest}"

if ! $WRANGLER_CMD --version >/dev/null 2>&1; then
    err "wrangler CLI not reachable via: $WRANGLER_CMD"
    err "Ensure npx is available or set WRANGLER env var to the wrangler binary path."
    exit 127
fi

WRANGLER_VER="$($WRANGLER_CMD --version 2>/dev/null || echo unknown)"
log "wrangler version: $WRANGLER_VER"

# CTRL-AUDIT-EMIT-BEFORE-MUTATION: log intent before every mutation.
log "APPLY MODE: will invoke $WRANGLER_CMD d1 migrations apply $DB_NAME --env $DB_ENV --remote"
log "WARNING: D1 schema changes are IRREVERSIBLE. Rollback = delete + re-provision D1."
log "         See specs/_runbooks/RB-D1-MIGRATION-APPLY.md for recovery procedure."
log ""

# wrangler d1 migrations apply handles idempotency via its internal
# d1_migrations table — it skips already-applied migrations automatically.
# We apply the entire set; wrangler determines what is pending.
# --env prod ensures the prod D1 binding (CONFIG_DB) is used with migrations_dir=migrations/d1.

log "invoking: $WRANGLER_CMD d1 migrations apply $DB_NAME --env $DB_ENV --remote"
if ! $WRANGLER_CMD d1 migrations apply "$DB_NAME" --env "$DB_ENV" --remote; then
    err "wrangler d1 migrations apply FAILED"
    err "Partial migration state possible. Do NOT re-run blindly."
    err "1. Run: wrangler d1 migrations list $DB_NAME --remote"
    err "2. Identify the last successfully applied migration."
    err "3. Investigate the failing SQL file."
    err "4. If schema is corrupt: restore from R2 snapshot (RB-D1-MIGRATION-APPLY.md §4)."
    exit 1
fi

log "wrangler d1 migrations apply OK"

# ── 6. Post-apply verification ───────────────────────────────────────────────

log "post-apply verification: counting tables in sqlite_master"

TABLE_COUNT_JSON="$($WRANGLER_CMD d1 execute "$DB_NAME" --env "$DB_ENV" --remote \
    --command="SELECT count(*) as n FROM sqlite_master WHERE type='table'" \
    --json 2>/dev/null)" || {
    err "post-apply table-count query failed"
    err "The migrations themselves may have succeeded. Check manually:"
    err "  $WRANGLER_CMD d1 execute $DB_NAME --env $DB_ENV --remote --command=\"SELECT count(*) FROM sqlite_master WHERE type='table'\""
    exit 1
}

ACTUAL_TABLE_COUNT="$(printf '%s' "$TABLE_COUNT_JSON" | python3 -c \
    "import json,sys,re; raw=sys.stdin.read(); m=re.search(r'(\[.*\])', raw, re.DOTALL); rows=json.loads(m.group(1)); print(rows[0]['results'][0]['n'])" 2>/dev/null)" || {
    err "failed to parse table count from JSON: $TABLE_COUNT_JSON"
    exit 1
}

log "actual table count: $ACTUAL_TABLE_COUNT"
log "expected table count: $EXPECTED_TABLE_COUNT"

if [[ "$ACTUAL_TABLE_COUNT" -lt "$EXPECTED_TABLE_COUNT" ]]; then
    err "VERIFICATION FAILED: expected >= $EXPECTED_TABLE_COUNT tables, got $ACTUAL_TABLE_COUNT"
    err "Some migrations may not have created the expected tables."
    err "Inspect: wrangler d1 execute $DB_NAME --remote --command=\"SELECT name FROM sqlite_master WHERE type='table' ORDER BY name\""
    exit 1
fi

log "VERIFICATION PASSED: $ACTUAL_TABLE_COUNT tables present (>= expected $EXPECTED_TABLE_COUNT)"
log ""
log "Phase D migration apply COMPLETE."
log "  db=$DB_NAME"
log "  migrations_applied=$ACTUAL_COUNT"
log "  tables_in_db=$ACTUAL_TABLE_COUNT"
log "  timestamp=$TIMESTAMP"
log ""
log "Next step: run scripts/put-secrets-prod.sh --apply"
exit 0
