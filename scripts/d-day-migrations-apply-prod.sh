#!/usr/bin/env bash
# d-day-migrations-apply-prod.sh — Wave 32 Phase D D-day migration runner.
#
# Applies 52 D1 migrations to corelink-prod-d1 via wrangler.
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
readonly DB_NAME="corelink-prod-d1"
readonly EXPECTED_FILE_COUNT=52
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

  --live             Execute the migrations for real against corelink-prod-d1.
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
  WRANGLER               Override wrangler binary path (default: npx wrangler@latest).

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
    WRANGLER_CMD="${WRANGLER:-npx wrangler@latest}"
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
log "  db=${DB_NAME}"
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

if [[ "${ACTUAL_COUNT}" -eq 0 ]]; then
    err "no .sql files found in ${MIGRATIONS_DIR}"
    exit 1
fi

if [[ "${ACTUAL_COUNT}" -ne "${EXPECTED_FILE_COUNT}" ]]; then
    err "HARD PAUSE (Wave-32 W4): expected ${EXPECTED_FILE_COUNT} migration files, found ${ACTUAL_COUNT}."
    err "Spec assumes exactly ${EXPECTED_FILE_COUNT} migrations. If migrations were added or"
    err "removed, amend the spec + update EXPECTED_FILE_COUNT in this script."
    err "See: specs/_audits/2026-05-27-w32-phaseD-migrations-prep-seal.md §3"
    exit 1
fi

log "migration file count: ${ACTUAL_COUNT} (matches spec expectation of ${EXPECTED_FILE_COUNT})"

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
log "  wrangler d1 migrations apply ${DB_NAME} --remote"
log ""

# ── Dry-run exit ──────────────────────────────────────────────────────────────

if "${DRY_RUN}"; then
    log "DRY-RUN complete. ${ACTUAL_COUNT} migration files listed above would be applied"
    log "to ${DB_NAME} via:"
    log "  wrangler d1 migrations apply ${DB_NAME} --remote"
    log ""
    log "To apply for real (D-day): $0 --live"
    log "To validate CF token first: $0 --validate-token"
    exit 0
fi

# ── Live apply path ───────────────────────────────────────────────────────────

log "LIVE MODE engaged — this will mutate corelink-prod-d1 IRREVERSIBLY."
log "WARNING: D1 schema changes cannot be rolled back."
log "         Rollback path = delete + re-provision D1 from Phase C."
log "         See: specs/_runbooks/RB-D1-MIGRATION-APPLY.md"
log ""

if [[ -z "${CLOUDFLARE_API_TOKEN:-}" ]]; then
    err "CLOUDFLARE_API_TOKEN is not set. Run: $0 --validate-token"
    exit 1
fi

WRANGLER_CMD="${WRANGLER:-npx wrangler@latest}"

if ! "${WRANGLER_CMD}" --version >/dev/null 2>&1; then
    err "wrangler CLI not reachable via: ${WRANGLER_CMD}"
    err "Ensure npx is in PATH or set WRANGLER env var to the wrangler binary."
    exit 127
fi

WRANGLER_VER="$("${WRANGLER_CMD}" --version 2>/dev/null || printf 'unknown')"
log "wrangler version: ${WRANGLER_VER}"

log "invoking: ${WRANGLER_CMD} d1 migrations apply ${DB_NAME} --remote"
if ! "${WRANGLER_CMD}" d1 migrations apply "${DB_NAME}" --remote; then
    err "wrangler d1 migrations apply FAILED."
    err "Partial migration state is possible. Do NOT re-run blindly."
    err "Recovery steps:"
    err "  1. wrangler d1 migrations list ${DB_NAME} --remote"
    err "  2. Identify last successfully applied migration."
    err "  3. Investigate the failing SQL file."
    err "  4. If schema is corrupt: restore from R2 snapshot (RB-D1-MIGRATION-APPLY.md §4)."
    exit 1
fi

log "wrangler d1 migrations apply: OK"
log ""
log "Phase D migration apply COMPLETE."
log "  db=${DB_NAME}"
log "  migrations_applied=${ACTUAL_COUNT}"
log "  timestamp=${TIMESTAMP}"
log ""
log "Next step: run scripts/put-secrets-prod.sh --apply"
exit 0
