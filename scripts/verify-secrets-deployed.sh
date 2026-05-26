#!/usr/bin/env bash
# verify-secrets-deployed.sh — Phase D verifier: diff the canonical secrets
# matrix (docs/internal/secrets-checklist.md) against wrangler secret list
# output to confirm all cf-wrangler secrets are deployed to prod.
#
# Outputs diff in +/- format:
#   +NAME  = in matrix but NOT deployed to Cloudflare (missing)
#   -NAME  = deployed to Cloudflare but NOT in matrix (undocumented)
#
# Exit codes:
#   0   — no diff; all canonical secrets are deployed, no undocumented extras
#   1   — diff present (see output for details)
#   2   — usage / argument error
#   127 — wrangler not in PATH
#
# Usage:
#   scripts/verify-secrets-deployed.sh [--env <env>] [--help]
#
#   --env <env>   Cloudflare Workers environment (default: prod).
#   --help        Print this message.

set -euo pipefail

# ── Constants ────────────────────────────────────────────────────────────────

readonly SCRIPT_NAME="$(basename "$0")"
readonly REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
readonly SECRETS_CHECKLIST="$REPO_ROOT/docs/internal/secrets-checklist.md"
readonly LOG_PREFIX="[$SCRIPT_NAME]"

# ── CLI parse ────────────────────────────────────────────────────────────────

WRANGLER_ENV="prod"

usage() {
    grep '^#' "$0" | head -30 | sed 's/^# \?//'
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --env)
            WRANGLER_ENV="${2:-}"
            if [[ -z "$WRANGLER_ENV" ]]; then
                printf '%s fatal: --env requires an argument\n' "$LOG_PREFIX" >&2
                exit 2
            fi
            shift 2
            ;;
        -h|--help)
            usage; exit 0 ;;
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

# ── 1. Check prerequisites ───────────────────────────────────────────────────

WRANGLER_CMD="${WRANGLER:-npx wrangler@latest}"

if ! $WRANGLER_CMD --version >/dev/null 2>&1; then
    err "wrangler CLI not reachable via: $WRANGLER_CMD"
    err "Ensure npx is available or set WRANGLER env var."
    exit 127
fi

if ! command -v jq >/dev/null 2>&1; then
    err "jq not found in PATH (required to parse wrangler secret list output)"
    exit 127
fi

if [[ ! -f "$SECRETS_CHECKLIST" ]]; then
    err "secrets checklist not found: $SECRETS_CHECKLIST"
    exit 1
fi

# ── 2. Parse canonical list from secrets-checklist.md ───────────────────────

log "parsing canonical cf-wrangler secrets from $SECRETS_CHECKLIST"

mapfile -t CANONICAL_NAMES < <(CHECKLIST_PATH="$SECRETS_CHECKLIST" python3 - <<'PYEOF'
import re, sys, os

checklist = os.environ.get('CHECKLIST_PATH', '')

with open(checklist) as f:
    content = f.read()

lines = content.split('\n')
in_matrix = False
results = []

for line in lines:
    stripped = line.strip()
    if stripped.startswith('| #') and 'Env var name' in stripped:
        in_matrix = True
        continue
    if in_matrix and re.match(r'^\|[-| ]+\|', stripped):
        continue
    if in_matrix and stripped.startswith('|') and not stripped.startswith('| #'):
        parts = [c.strip() for c in stripped.split('|')]
        parts = [p for p in parts if p != '']
        if len(parts) < 11:
            continue
        row_num = parts[0]
        if not row_num.isdigit():
            continue
        env_var = parts[2].strip('`')
        stored_at = parts[10]
        if 'cf-wrangler' in stored_at:
            results.append(env_var)

for r in results:
    print(r)
PYEOF
)

CANONICAL_COUNT="${#CANONICAL_NAMES[@]}"

if [[ "$CANONICAL_COUNT" -eq 0 ]]; then
    err "secrets parser returned 0 results — check $SECRETS_CHECKLIST format"
    exit 1
fi

log "canonical cf-wrangler secret count: $CANONICAL_COUNT"

# ── 3. Query deployed secrets from Cloudflare ────────────────────────────────

log "querying wrangler secret list --env $WRANGLER_ENV"

# NOTE: wrangler secret list outputs JSON by default (no --json flag needed;
# --json is not a supported flag for this subcommand). Strip the Cloudflare
# skills banner from stdout before parsing.
DEPLOYED_JSON="$($WRANGLER_CMD secret list --env "$WRANGLER_ENV" 2>/dev/null | \
    python3 -c 'import sys,re; raw=sys.stdin.read(); m=re.search(r"(\[.*\])", raw, re.DOTALL); print(m.group(1) if m else "[]")')" || {
    err "wrangler secret list failed"
    err "Check: wrangler auth, --env value, and that the Worker is deployed."
    exit 1
}

mapfile -t DEPLOYED_NAMES < <(printf '%s' "$DEPLOYED_JSON" | jq -r '.[].name' | sort)
DEPLOYED_COUNT="${#DEPLOYED_NAMES[@]}"

log "deployed secret count (env=$WRANGLER_ENV): $DEPLOYED_COUNT"

# ── 4. Build sorted sets and diff ────────────────────────────────────────────

CANONICAL_SORTED="$(printf '%s\n' "${CANONICAL_NAMES[@]}" | sort)"
DEPLOYED_SORTED="$(printf '%s\n' "${DEPLOYED_NAMES[@]}" | sort)"

# Secrets in canonical but NOT deployed.
MISSING=()
while IFS= read -r name; do
    if [[ -n "$name" ]]; then
        MISSING+=("$name")
    fi
done < <(comm -23 \
    <(printf '%s\n' "${CANONICAL_NAMES[@]}" | sort) \
    <(printf '%s\n' "${DEPLOYED_NAMES[@]}" | sort))

# Secrets deployed but NOT in canonical.
EXTRA=()
while IFS= read -r name; do
    if [[ -n "$name" ]]; then
        EXTRA+=("$name")
    fi
done < <(comm -13 \
    <(printf '%s\n' "${CANONICAL_NAMES[@]}" | sort) \
    <(printf '%s\n' "${DEPLOYED_NAMES[@]}" | sort))

# ── 5. Report ─────────────────────────────────────────────────────────────────

log ""
log "== SECRETS VERIFICATION REPORT (env=$WRANGLER_ENV) =="
log "canonical (matrix): $CANONICAL_COUNT"
log "deployed:           $DEPLOYED_COUNT"
log "missing (in matrix, NOT deployed): ${#MISSING[@]}"
log "extra   (deployed, NOT in matrix): ${#EXTRA[@]}"
log ""

DIFF_FOUND=false

if [[ "${#MISSING[@]}" -gt 0 ]]; then
    DIFF_FOUND=true
    log "MISSING (must be deployed before Phase D is complete):"
    for name in "${MISSING[@]}"; do
        log "  +$name"
    done
    log ""
fi

if [[ "${#EXTRA[@]}" -gt 0 ]]; then
    DIFF_FOUND=true
    log "EXTRA (deployed but not in matrix — investigate; may be stale or undocumented):"
    for name in "${EXTRA[@]}"; do
        log "  -$name"
    done
    log ""
fi

if ! $DIFF_FOUND; then
    log "PASS: no diff. All $CANONICAL_COUNT canonical secrets are deployed."
    log "      No undocumented extras detected."
    log ""
    log "Phase D secrets verification COMPLETE."
    exit 0
else
    err "FAIL: diff detected between canonical matrix and deployed secrets."
    err "Resolve all +/- lines above before Phase D can be signed off."
    exit 1
fi
