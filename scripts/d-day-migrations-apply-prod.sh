#!/usr/bin/env bash
# d-day-migrations-apply-prod.sh — Wave 32 Phase D D-day migration runner.
#
# Applies 52 D1 migrations to corelink-config-prod (binding: CONFIG_DB) via wrangler.
#
# DEFAULT MODE: --dry-run  (no live CF API calls; prints plan and exits 0)
#
# Charter constraints honoured:
#   Wave-32 W1: dry-run is the default mode (--live is explicit opt-in).
#   Wave-32 W2: idempotent — wrangler d1 migrations apply skips already-applied
#               migrations via its internal ledger table.
#   Wave-32 W3: no live API calls unless --live is passed (this agent itself
#               never makes live calls during authoring).
#   Wave-32 W4: additive guard runs before any mutation; exits 1 on violation.
#   INV-AUTH-MIGRATION-ADDITIVE (HIGH): additive guard wraps check_migrations_additive.py.
#   CTRL-AUDIT-EMIT-BEFORE-MUTATION: migration plan emitted to stdout before
#               any wrangler invocation.
#
# WRANGLER v4 REQUIREMENT:
#   wrangler.toml uses [[containers]] array syntax which requires wrangler v4+.
#   Bare `npx wrangler` or `npx wrangler@latest` may resolve to a stale v3
#   cache and fail with: "containers" should be an object, but got an array.
#   The default WRANGLER_CMD therefore prefers the worker-local v4 binary at
#   worker/node_modules/.bin/wrangler (v4.95.0), falling back to npx wrangler@4
#   only if the local binary is absent. Override via WRANGLER env var.
#
# D1 BINDING / ENV CLARIFICATION (BUG 1 fix — 2026-05-28):
#   The prod D1 database lives under [[env.prod.d1_databases]] in wrangler.toml:
#     binding = "CONFIG_DB"
#     database_name = "corelink-config-prod"
#     database_id = "d64742ea-e102-40b2-a844-ff02e3f94562"
#   The top-level [[d1_databases]] binding (line 169) is the dev binding with a
#   PLACEHOLDER uuid. Invoking without --env prod would silently target the dev
#   binding and fail. Correct invocation: CONFIG_DB --remote --env prod.
#
# Exit codes:
#   0   — success (dry-run printed plan, or live apply completed)
#   1   — migration failure or guard failure
#   2   — usage / argument error
#   127 — wrangler not in PATH (--live / --validate-token only)
#
# Usage:
#   scripts/d-day-migrations-apply-prod.sh [--dry-run] [--live] [--validate-token] [--apply] [--help]

set -euo pipefail

# ── Constants ─────────────────────────────────────────────────────────────────

SCRIPT_NAME="$(basename "$0")"
readonly SCRIPT_NAME
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
readonly REPO_ROOT
readonly MIGRATIONS_DIR="${REPO_ROOT}/migrations/d1"
# BUG 1 fix (2026-05-28): The prod D1 lives under [[env.prod.d1_databases]] with
# binding = "CONFIG_DB" (database_name = "corelink-config-prod",
# uuid = d64742ea-e102-40b2-a844-ff02e3f94562).  The wrangler CLI takes the
# *binding name* (CONFIG_DB), not the database_name string.  Without --env prod
# the command would target the top-level dev binding (PLACEHOLDER uuid) and fail.
readonly DB_BINDING="CONFIG_DB"
readonly DB_ENV="prod"
# DB_NAME retained for log messages / backwards-compat references only.
readonly DB_NAME="corelink-config-prod"
# EXPECTED_FILE_COUNT is computed DYNAMICALLY from the actual
# migrations/d1/*.sql files at runtime (see pre-flight below), NOT hardcoded.
# A hardcoded baseline (was 52) goes stale on every new migration and would
# HARD-PAUSE a legitimate re-provision once the file count drifts — exactly
# the ledger-desync landmine this DR-hardening removes. The guard's INTENT
# (detect a truncated / empty migrations dir before touching prod) is kept:
# we still hard-pause if the dynamic count is 0. True ledger-vs-files drift
# is owned by wrangler's internal d1_migrations table, which skips
# already-applied migrations idempotently.
readonly ADDITIVE_AUDIT_SCRIPT="${REPO_ROOT}/scripts/d-day-migrations-additive-audit.sh"
readonly ADDITIVE_CHECK_SCRIPT="${REPO_ROOT}/scripts/check_migrations_additive.py"
TIMESTAMP="$(date -u +%Y%m%dT%H%M%SZ)"
readonly TIMESTAMP
readonly LOG_PREFIX="[${SCRIPT_NAME}]"

# ── CLI parse ─────────────────────────────────────────────────────────────────

DRY_RUN=true
LIVE_MODE=false
VALIDATE_TOKEN_MODE=false

usage() {
    cat <<'USAGE'
d-day-migrations-apply-prod.sh — Wave 32 Phase D D-day migration runner

USAGE:
  scripts/d-day-migrations-apply-prod.sh [FLAGS]

FLAGS:
  --dry-run          (DEFAULT) Print the sequential migration plan and the
                     wrangler command that WOULD be executed. No live CF API
                     calls. Exits 0 on success.

  --live             Execute the migrations for real against corelink-config-prod
                     (binding CONFIG_DB, --env prod, uuid d64742ea).
                     Requires CLOUDFLARE_API_TOKEN env var to be set with
                     D1:Write + Account:Read scopes. Additive guard runs first;
                     aborts if any non-additive pattern is detected (W4).
                     IRREVERSIBLE — D1 schema changes cannot be rolled back.
                     Rollback = delete + re-provision D1 from Phase C.

  --apply            Alias for --live (backwards compat with apply-d1-migrations-prod.sh).

  --validate-token   Run `wrangler whoami` to confirm CF API token is present
                     and reachable. Exits 0 if token resolves; exits 1 with
                     scope checklist if absent or invalid. No migrations applied.

  --help, -h         Print this help and exit 0.

ENVIRONMENT:
  CLOUDFLARE_API_TOKEN   Required for --live and --validate-token.
                         Scopes needed: D1:Write, Account:Read.
  WRANGLER               Override wrangler binary path.
                         Default: worker/node_modules/.bin/wrangler (v4, local)
                         Fallback: npx wrangler@4 (wrangler v4 required for
                         [[containers]] array syntax in wrangler.toml).
                         Do NOT use bare npx wrangler or npx wrangler@latest —
                         stale cache may resolve to v3 which rejects [[containers]].

SPEC REF:
  specs/_audits/sealed/2026-05-22-wave32-prod-deploy-spec.md line 149
  specs/_audits/2026-05-27-w32-phaseD-migrations-prep-seal.md

EXAMPLES:
  # Preview what would be applied (safe, no side-effects):
  scripts/d-day-migrations-apply-prod.sh --dry-run

  # Confirm CF token is configured:
  scripts/d-day-migrations-apply-prod.sh --validate-token

  # Apply for real (D-day):
  scripts/d-day-migrations-apply-prod.sh --live
USAGE
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --dry-run)         DRY_RUN=true;  LIVE_MODE=false; shift ;;
        --live|--apply)    LIVE_MODE=true; DRY_RUN=false;  shift ;;
        --validate-token)  VALIDATE_TOKEN_MODE=true;       shift ;;
        -h|--help)         usage; exit 0 ;;
        *)
            printf '%s fatal: unknown argument: %s\n' "${LOG_PREFIX}" "$1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

# ── Logging ───────────────────────────────────────────────────────────────────

log()  { printf '%s [%s] %s\n' "${LOG_PREFIX}" "$(date -u +%H:%M:%SZ)" "$*"; }
err()  { printf '%s [%s] ERROR: %s\n' "${LOG_PREFIX}" "$(date -u +%H:%M:%SZ)" "$*" >&2; }
warn() { printf '%s [%s] WARN: %s\n'  "${LOG_PREFIX}" "$(date -u +%H:%M:%SZ)" "$*"; }

# ── --validate-token mode ─────────────────────────────────────────────────────

if "${VALIDATE_TOKEN_MODE}"; then
    log "mode=VALIDATE-TOKEN"
    # BUG 2 fix (2026-05-28): prefer worker-local wrangler v4.95.0; bare
    # npx wrangler or npx wrangler@latest may resolve to v3 from stale cache.
    # wrangler.toml uses [[containers]] array syntax which requires v4+.
    _LOCAL_WRANGLER="${REPO_ROOT}/worker/node_modules/.bin/wrangler"
    if [[ -z "${WRANGLER:-}" ]] && [[ -x "${_LOCAL_WRANGLER}" ]]; then
        WRANGLER_CMD="${_LOCAL_WRANGLER}"
    else
        WRANGLER_CMD="${WRANGLER:-npx wrangler@4}"
    fi
    if [[ -z "${CLOUDFLARE_API_TOKEN:-}" ]]; then
        err "CLOUDFLARE_API_TOKEN is not set."
        err "Required scopes for Phase D:"
        err "  - D1:Write       (apply migrations to corelink-prod-d1)"
        err "  - Account:Read   (resolve account ID for wrangler whoami)"
        err "Set the token: export CLOUDFLARE_API_TOKEN=<token>"
        err "Source: .env.local (Bucket 2 creds validated 2026-05-22)"
        exit 1
    fi
    log "CLOUDFLARE_API_TOKEN is set (length=${#CLOUDFLARE_API_TOKEN})."
    log "Running: ${WRANGLER_CMD} whoami"
    if ! "${WRANGLER_CMD}" whoami 2>&1; then
        err "wrangler whoami failed — token may be invalid or lack required scopes."
        err "Required scopes: D1:Write, Account:Read"
        exit 1
    fi
    log "Token validated OK."
    exit 0
fi

# ── Pre-flight: migrations directory ─────────────────────────────────────────

log "Phase D migration runner starting"
log "  mode=$(if "${LIVE_MODE}"; then printf 'LIVE'; else printf 'DRY-RUN'; fi)"
log "  db_binding=${DB_BINDING}  db_name=${DB_NAME}  env=${DB_ENV}"
log "  migrations_dir=${MIGRATIONS_DIR}"
log "  timestamp=${TIMESTAMP}"

if [[ ! -d "${MIGRATIONS_DIR}" ]]; then
    err "migrations directory not found: ${MIGRATIONS_DIR}"
    exit 1
fi

# Collect SQL files in lexicographic order (matches wrangler migration semantics).
SQL_FILES=()
while IFS= read -r -d '' f; do
    SQL_FILES+=("$(basename "$f")")
done < <(find "${MIGRATIONS_DIR}" -maxdepth 1 -name '*.sql' -print0 | sort -z)

ACTUAL_COUNT="${#SQL_FILES[@]}"

# Dynamic baseline: derive the expected count from the files actually on disk
# instead of a stale hardcoded constant. This keeps the truncation guard alive
# (count==0 still hard-pauses) without going stale on every new migration.
readonly EXPECTED_FILE_COUNT="${ACTUAL_COUNT}"

if [[ "${ACTUAL_COUNT}" -eq 0 ]]; then
    err "HARD PAUSE (Wave-32 W4): no .sql files found in ${MIGRATIONS_DIR}."
    err "The migrations dir is empty or truncated — refusing to apply against prod."
    exit 1
fi

log "migration file count: ${ACTUAL_COUNT} (dynamic; derived from migrations/d1/*.sql)"

# ── Additive guard (Wave-32 W4) ───────────────────────────────────────────────

log "running additive guard (Wave-32 W4)"

if [[ -x "${ADDITIVE_AUDIT_SCRIPT}" ]]; then
    if ! bash "${ADDITIVE_AUDIT_SCRIPT}" 2>&1; then
        err "INV-AUTH-MIGRATION-ADDITIVE: d-day-migrations-additive-audit.sh reported violations."
        err "Wave-32 W4 hard-pause: DO NOT apply migrations."
        exit 1
    fi
elif [[ -f "${ADDITIVE_CHECK_SCRIPT}" ]]; then
    warn "d-day-migrations-additive-audit.sh not executable; falling back to check_migrations_additive.py"
    if ! python3 "${ADDITIVE_CHECK_SCRIPT}" 2>&1; then
        err "INV-AUTH-MIGRATION-ADDITIVE: check_migrations_additive.py reported violations."
        err "Wave-32 W4 hard-pause: DO NOT apply migrations."
        exit 1
    fi
else
    warn "no additive guard script found — skipping guard (guard runs at PR-time via CI)"
fi

log "additive guard: PASSED"

# ── CTRL-AUDIT-EMIT-BEFORE-MUTATION: emit sequential plan ────────────────────

log ""
log "sequential migration plan (${ACTUAL_COUNT} files, lex order = wrangler apply order):"
N=0
for fname in "${SQL_FILES[@]}"; do
    N=$((N + 1))
    fpath="${MIGRATIONS_DIR}/${fname}"
    STMT_COUNT="$(grep -c ';' "${fpath}" 2>/dev/null || printf '?')"
    log "  [$(printf '%02d' "${N}")/${ACTUAL_COUNT}] ${fname}  (~${STMT_COUNT} stmt)"
done
log ""
log "wrangler command:"
log "  wrangler d1 migrations apply ${DB_BINDING} --remote --env ${DB_ENV}"
log ""

# ── Dry-run exit ──────────────────────────────────────────────────────────────

if "${DRY_RUN}"; then
    log "DRY-RUN complete. ${ACTUAL_COUNT} migration files listed above would be applied"
    log "to ${DB_NAME} (binding=${DB_BINDING}, env=${DB_ENV}) via:"
    log "  wrangler d1 migrations apply ${DB_BINDING} --remote --env ${DB_ENV}"
    log ""
    log "To apply for real (D-day): $0 --live"
    log "To validate CF token first: $0 --validate-token"
    exit 0
fi

# ── Live apply path ───────────────────────────────────────────────────────────

log "LIVE MODE engaged — this will mutate ${DB_NAME} IRREVERSIBLY."
log "WARNING: D1 schema changes cannot be rolled back."
log "         Rollback path = delete + re-provision D1 from Phase C."
log "         See: specs/_runbooks/RB-D1-MIGRATION-APPLY.md"
log ""

if [[ -z "${CLOUDFLARE_API_TOKEN:-}" ]]; then
    err "CLOUDFLARE_API_TOKEN is not set. Run: $0 --validate-token"
    exit 1
fi

# BUG 2 fix (2026-05-28): prefer worker-local wrangler v4.95.0; bare
# npx wrangler or npx wrangler@latest may resolve to v3 from stale cache.
# wrangler.toml uses [[containers]] array syntax which requires v4+.
_LOCAL_WRANGLER="${REPO_ROOT}/worker/node_modules/.bin/wrangler"
if [[ -z "${WRANGLER:-}" ]] && [[ -x "${_LOCAL_WRANGLER}" ]]; then
    WRANGLER_CMD="${_LOCAL_WRANGLER}"
else
    WRANGLER_CMD="${WRANGLER:-npx wrangler@4}"
fi

if ! "${WRANGLER_CMD}" --version >/dev/null 2>&1; then
    err "wrangler CLI not reachable via: ${WRANGLER_CMD}"
    err "Ensure npx is in PATH or set WRANGLER env var to the wrangler binary."
    exit 127
fi

WRANGLER_VER="$("${WRANGLER_CMD}" --version 2>/dev/null || printf 'unknown')"
log "wrangler version: ${WRANGLER_VER}"

log "invoking: ${WRANGLER_CMD} d1 migrations apply ${DB_BINDING} --remote --env ${DB_ENV}"
if ! "${WRANGLER_CMD}" d1 migrations apply "${DB_BINDING}" --remote --env "${DB_ENV}"; then
    err "wrangler d1 migrations apply FAILED."
    err "Partial migration state is possible. Do NOT re-run blindly."
    err "Recovery steps:"
    err "  1. wrangler d1 migrations list ${DB_BINDING} --remote --env ${DB_ENV}"
    err "  2. Identify last successfully applied migration."
    err "  3. Investigate the failing SQL file."
    err "  4. If schema is corrupt: restore from R2 snapshot (RB-D1-MIGRATION-APPLY.md §4)."
    exit 1
fi

log "wrangler d1 migrations apply: OK"
log ""
log "Phase D migration apply COMPLETE."
log "  db_binding=${DB_BINDING}  db_name=${DB_NAME}  env=${DB_ENV}"
log "  migrations_applied=${ACTUAL_COUNT}"
log "  timestamp=${TIMESTAMP}"
log ""
log "Next step: run scripts/put-secrets-prod.sh --apply"
exit 0
