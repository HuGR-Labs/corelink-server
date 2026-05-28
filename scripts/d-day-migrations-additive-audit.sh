#!/usr/bin/env bash
# d-day-migrations-additive-audit.sh — Wave 32 Phase D pre-stage additive audit.
#
# Scans all 52 D1 migration files for non-additive SQL patterns (DROP TABLE,
# DROP COLUMN, ALTER TABLE ... DROP, RENAME TABLE, RENAME COLUMN).
#
# Lines that are pure SQL comments (-- ...) are excluded before pattern
# matching so documented roll-back stubs in comment blocks do not trigger.
#
# Output: tab-separated columns per violation:
#   migration_file<TAB>classification<TAB>match_line
#
# Exit codes:
#   0 — all migrations are strictly additive; no violations found.
#   1 — at least one non-additive pattern detected (migrations unsafe to apply).
#
# Canonical spec ref:
#   specs/_audits/sealed/2026-05-22-wave32-prod-deploy-spec.md line 149
#   INV-AUTH-MIGRATION-ADDITIVE (HIGH)
#
# No external deps beyond grep -E, awk, wc (POSIX).

set -euo pipefail

SCRIPT_NAME="$(basename "$0")"
readonly SCRIPT_NAME
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
readonly REPO_ROOT
readonly MIGRATIONS_DIR="${REPO_ROOT}/migrations/d1"

# ── Non-additive patterns (contract §INPUTS) ─────────────────────────────────
# Pattern label → ERE pattern (case-insensitive via grep -i)
# Evaluated against SQL lines AFTER comment stripping.

readonly PATTERNS_LABELS=(
    "DROP_TABLE"
    "DROP_COLUMN"
    "ALTER_TABLE_DROP"
    "RENAME_TABLE"
    "RENAME_COLUMN"
)

readonly PATTERNS_RE=(
    '^\s*DROP\s+TABLE\b'
    '\bDROP\s+COLUMN\b'
    '\bALTER\s+TABLE\b.*\bDROP\b'
    '\bRENAME\s+TABLE\b'
    '\bRENAME\s+COLUMN\b'
)

# ── Helpers ───────────────────────────────────────────────────────────────────

log()  { printf '[%s] %s\n' "$SCRIPT_NAME" "$*"; }
err()  { printf '[%s] ERROR: %s\n' "$SCRIPT_NAME" "$*" >&2; }

# ── Validate migrations directory ─────────────────────────────────────────────

if [[ ! -d "${MIGRATIONS_DIR}" ]]; then
    err "migrations directory not found: ${MIGRATIONS_DIR}"
    exit 1
fi

# ── Collect SQL files (lex order) ─────────────────────────────────────────────

SQL_FILES=()
while IFS= read -r -d '' f; do
    SQL_FILES+=("$f")
done < <(find "${MIGRATIONS_DIR}" -maxdepth 1 -name '*.sql' -print0 | sort -z)

FILE_COUNT="${#SQL_FILES[@]}"
if [[ "${FILE_COUNT}" -eq 0 ]]; then
    err "no .sql files found under ${MIGRATIONS_DIR}"
    exit 1
fi

log "scanning ${FILE_COUNT} migration file(s) in ${MIGRATIONS_DIR}"
log "patterns: DROP TABLE | DROP COLUMN | ALTER TABLE ... DROP | RENAME TABLE | RENAME COLUMN"
log ""

# ── Per-file scan ─────────────────────────────────────────────────────────────

VIOLATION_COUNT=0

for sql_path in "${SQL_FILES[@]}"; do
    fname="$(basename "${sql_path}")"

    # Strip comment-only lines (lines starting with optional whitespace + '--')
    # so documented roll-back stubs (commented DROP / ALTER) don't trigger.
    # (stripping is done inline in the awk pipeline below)

    for i in "${!PATTERNS_LABELS[@]}"; do
        label="${PATTERNS_LABELS[$i]}"
        pattern="${PATTERNS_RE[$i]}"

        # grep against the stripped content; -n for line numbers in original file.
        # We need original line numbers: grep on the original file but only on
        # non-comment lines.  Strategy: filter out comment lines via awk,
        # preserving original line numbers, then grep.
        while IFS= read -r match_line; do
            VIOLATION_COUNT=$((VIOLATION_COUNT + 1))
            printf '%s\t%s\t%s\n' "${fname}" "${label}" "${match_line}"
        done < <(
            awk '!/^\s*--/' "${sql_path}" \
            | grep -Ei "${pattern}" \
            || true
        )
    done
done

log ""

# ── Summary ───────────────────────────────────────────────────────────────────

if [[ "${VIOLATION_COUNT}" -gt 0 ]]; then
    err "FAIL: ${VIOLATION_COUNT} non-additive token(s) detected across ${FILE_COUNT} migration file(s)."
    err "INV-AUTH-MIGRATION-ADDITIVE (HIGH) violated."
    err "Wave 32 W4 hard-pause triggered: DO NOT apply migrations to corelink-prod-d1."
    exit 1
fi

log "PASS: ${FILE_COUNT} migration file(s) scanned — all strictly additive."
log "Wave 32 W4 gate: CLEAR. Safe to proceed with d-day-migrations-apply-prod.sh --live."
exit 0
