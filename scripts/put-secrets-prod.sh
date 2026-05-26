#!/usr/bin/env bash
# put-secrets-prod.sh — Phase D runner: push all cf-wrangler secrets from the
# canonical secrets-checklist.md matrix to Cloudflare Workers (env=prod).
#
# Default mode: DRY-RUN (lists secrets that would be pushed; no mutations).
# Pass --apply to actually execute the wrangler secret put commands.
#
# Charter constraints honoured:
#   CTRL-CRED-001: NO secret values are hard-coded in this script.
#     Values are sourced from .env.local (if present) or prompted interactively.
#   CTRL-AUDIT-EMIT-BEFORE-MUTATION: each secret put is logged (name + value
#     length + SHA256[:8]) BEFORE wrangler is invoked. The value itself is
#     NEVER logged.
#   Audit-fail-CLOSED on secret put: any failed wrangler secret put triggers
#     rollback — wrangler secret delete is called for every secret successfully
#     put in the same session. Partial-deploy state is avoided.
#
# Exit codes:
#   0   — success (all secrets put, or dry-run completed cleanly)
#   1   — wrangler secret put failure (rollback executed; see stderr)
#   2   — usage / argument error
#   127 — wrangler not in PATH
#
# Usage:
#   scripts/put-secrets-prod.sh [--apply] [--env-file <path>] [--help]
#
#   --apply              Actually push secrets via wrangler (opt-in; NOT default).
#   --env-file <path>    Path to env file to source (default: .env.local in repo root).
#                        Pass --env-file /dev/null to force interactive prompts.
#   --help               Print this message.

set -euo pipefail

# ── Constants ────────────────────────────────────────────────────────────────

readonly SCRIPT_NAME="$(basename "$0")"
readonly REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
readonly SECRETS_CHECKLIST="$REPO_ROOT/docs/internal/secrets-checklist.md"
readonly DEFAULT_ENV_FILE="$REPO_ROOT/.env.local"
readonly WRANGLER_ENV="prod"
readonly LOG_PREFIX="[$SCRIPT_NAME]"
readonly TIMESTAMP="$(date -u +%Y%m%dT%H%M%SZ)"

# ── CLI parse ────────────────────────────────────────────────────────────────

APPLY_MODE=false
MVP_ONLY=false
ENV_FILE="$DEFAULT_ENV_FILE"
readonly MVP_ALLOWLIST="$REPO_ROOT/scripts/secrets-mvp-allowlist.txt"

usage() {
    grep '^#' "$0" | head -45 | sed 's/^# \?//'
    cat <<'USAGE_EXTRA'

  --mvp-only      Only put secrets present in scripts/secrets-mvp-allowlist.txt.
                  Secrets in secrets-checklist.md but NOT in the allowlist are
                  SKIPPED with a WARN (not failed). Per Wave 32 Phase D MVP
                  decision: solopreneur deploy doesn't need Slack/Twilio/etc
                  at provision-time; per-feature lazy enrollment.
USAGE_EXTRA
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --apply)
            APPLY_MODE=true; shift ;;
        --mvp-only)
            MVP_ONLY=true; shift ;;
        --env-file)
            ENV_FILE="${2:-}"
            if [[ -z "$ENV_FILE" ]]; then
                printf '%s fatal: --env-file requires a path argument\n' "$LOG_PREFIX" >&2
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
warn() { printf '%s [%s] WARN: %s\n'  "$LOG_PREFIX" "$(date -u +%H:%M:%SZ)" "$*"; }

# ── 1. Source .env.local (CTRL-CRED-001: values from file, never hard-coded) ─

if [[ -f "$ENV_FILE" ]]; then
    log "sourcing env file: $ENV_FILE"
    # Source without set -a (don't export everything — only what's accessed).
    # shellcheck disable=SC1090
    set +u  # env file may reference undefined vars
    # We use 'source' but deliberately do NOT 'set -a' — only needed vars get
    # read via get_secret() below which checks the current environment.
    source "$ENV_FILE" || {
        err "failed to source $ENV_FILE"
        exit 1
    }
    set -u
else
    warn "env file not found: $ENV_FILE"
    warn "will prompt interactively for each secret value (if --apply)"
fi

# ── 2. Parse secrets list from secrets-checklist.md ─────────────────────────

# Extract env var names where Stored-at column contains 'cf-wrangler'.
# Uses the same logic as the Python-verified parser in the prep audit.

if [[ ! -f "$SECRETS_CHECKLIST" ]]; then
    err "secrets checklist not found: $SECRETS_CHECKLIST"
    exit 1
fi

# Parse with Python (awk cannot handle multi-cell pipe-split reliably for
# the complex cell content in this matrix).
# Pass the checklist path via CHECKLIST_PATH env var (heredoc single-quotes
# prevent shell variable expansion inside the script body).
mapfile -t SECRET_NAMES < <(CHECKLIST_PATH="$SECRETS_CHECKLIST" python3 - <<'PYEOF'
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

SECRET_COUNT="${#SECRET_NAMES[@]}"

if [[ "$SECRET_COUNT" -eq 0 ]]; then
    err "secrets parser returned 0 results — check $SECRETS_CHECKLIST format"
    exit 1
fi

log "parsed $SECRET_COUNT cf-wrangler secrets from secrets-checklist.md"

# ── 3. Helper: get secret value (CTRL-CRED-001 compliant) ────────────────────

# Array to track secrets successfully put (for rollback on failure).
SUCCESSFULLY_PUT=()

get_secret() {
    local name="$1"
    local val=""

    # Try environment (sourced from .env.local or already set).
    if [[ -n "${!name:-}" ]]; then
        val="${!name}"
    else
        if $APPLY_MODE; then
            # Interactive prompt — value is never echoed or logged.
            read -r -s -p "Enter value for $name (input hidden): " val </dev/tty
            printf '\n' >/dev/tty
            if [[ -z "$val" ]]; then
                err "empty value provided for $name — aborting"
                return 1
            fi
        else
            # Dry-run: just note the var is not set.
            val="<NOT_SET_IN_ENV>"
        fi
    fi
    printf '%s' "$val"
}

value_hash() {
    local val="$1"
    printf '%s' "$val" | sha256sum 2>/dev/null | cut -c1-8 \
        || printf '%s' "$val" | shasum -a 256 2>/dev/null | cut -c1-8 \
        || echo "hash-unavailable"
}

# ── 4. Rollback handler ──────────────────────────────────────────────────────

rollback() {
    local failed_name="$1"
    err "========================================="
    err "ROLLBACK triggered (failed on: $failed_name)"
    err "Deleting ${#SUCCESSFULLY_PUT[@]} already-put secret(s) in this session..."

    local rb_failed=0
    for name in "${SUCCESSFULLY_PUT[@]}"; do
        if $WRANGLER_CMD secret delete "$name" --env "$WRANGLER_ENV" --force 2>/dev/null; then
            err "  ROLLBACK OK: deleted $name"
        else
            err "  ROLLBACK FAIL: could not delete $name — manual cleanup required"
            rb_failed=$((rb_failed + 1))
        fi
    done

    if [[ "$rb_failed" -gt 0 ]]; then
        err "WARNING: $rb_failed secret(s) could NOT be deleted during rollback."
        err "Manual cleanup required. Run:"
        for name in "${SUCCESSFULLY_PUT[@]}"; do
            err "  $WRANGLER_CMD secret delete $name --env $WRANGLER_ENV --force"
        done
    else
        err "ROLLBACK COMPLETE: all $((${#SUCCESSFULLY_PUT[@]})) secrets removed from prod."
    fi
    err "========================================="
}

# ── 4.5 MVP allowlist filter (if --mvp-only) ────────────────────────────────

if $MVP_ONLY; then
    if [[ ! -f "$MVP_ALLOWLIST" ]]; then
        err "MVP allowlist file not found: $MVP_ALLOWLIST"
        err "Run without --mvp-only OR create the allowlist file first."
        exit 1
    fi
    log "MVP-ONLY mode: filtering against $MVP_ALLOWLIST"

    # Read allowlist (strip comments, whitespace, empty lines).
    mapfile -t MVP_NAMES < <(grep -v '^[[:space:]]*#' "$MVP_ALLOWLIST" | grep -v '^[[:space:]]*$' | awk '{$1=$1};1')

    # Filter SECRET_NAMES to intersection with MVP_NAMES; track deferred ones.
    FILTERED=()
    DEFERRED=()
    for name in "${SECRET_NAMES[@]}"; do
        match=false
        for mvp in "${MVP_NAMES[@]}"; do
            if [[ "$name" == "$mvp" ]]; then match=true; break; fi
        done
        if $match; then
            FILTERED+=("$name")
        else
            DEFERRED+=("$name")
        fi
    done

    # Also include MVP-allowlist entries that aren't in checklist (defensive —
    # e.g. internal HMAC keys generated outside the vendor matrix).
    for mvp in "${MVP_NAMES[@]}"; do
        in_filtered=false
        for f in "${FILTERED[@]:-}"; do
            if [[ "$f" == "$mvp" ]]; then in_filtered=true; break; fi
        done
        if ! $in_filtered; then
            FILTERED+=("$mvp")
        fi
    done

    SECRET_NAMES=("${FILTERED[@]}")
    SECRET_COUNT=${#SECRET_NAMES[@]}

    if [[ ${#DEFERRED[@]} -gt 0 ]]; then
        warn ""
        warn "Deferred (NOT in MVP allowlist; will be skipped):"
        for d in "${DEFERRED[@]}"; do
            warn "  - $d"
        done
        warn ""
        warn "These will need to be put when the corresponding feature ships."
        warn ""
    fi
    log "MVP-mode active: $SECRET_COUNT secrets will be put (${#DEFERRED[@]} deferred to WARN)."
fi

# ── 5. Emit plan ─────────────────────────────────────────────────────────────

log ""
log "Secret put plan (env=$WRANGLER_ENV):"
N=0
for name in "${SECRET_NAMES[@]}"; do
    N=$((N + 1))
    log "  [$N/$SECRET_COUNT] $name"
done
log ""

# ── 6. Dry-run: exit here ─────────────────────────────────────────────────────

if ! $APPLY_MODE; then
    log "DRY-RUN complete. $SECRET_COUNT secrets listed above would be put to"
    log "Cloudflare Workers (--env $WRANGLER_ENV) via: wrangler secret put <NAME> --env $WRANGLER_ENV"
    log ""
    log "Values sourced from: ${ENV_FILE} (if present) or interactive prompt."
    log "CTRL-CRED-001: no values are logged or hard-coded."
    log ""
    log "To apply for real: $0 --apply"
    exit 0
fi

# ── 7. Apply path (requires wrangler + CF auth) ───────────────────────────────

WRANGLER_CMD="${WRANGLER:-npx wrangler@latest}"

if ! $WRANGLER_CMD --version >/dev/null 2>&1; then
    err "wrangler CLI not reachable via: $WRANGLER_CMD"
    err "Ensure npx is available or set WRANGLER env var to the wrangler binary path."
    exit 127
fi

WRANGLER_VER="$($WRANGLER_CMD --version 2>/dev/null || echo unknown)"
log "wrangler version: $WRANGLER_VER"
log ""
log "APPLY MODE: pushing $SECRET_COUNT secrets to $WRANGLER_ENV"
log ""

N=0
for name in "${SECRET_NAMES[@]}"; do
    N=$((N + 1))

    # Get value (CTRL-CRED-001: from env or prompt; never hard-coded).
    VAL="$(get_secret "$name")" || {
        rollback "$name"
        exit 1
    }

    VAL_LEN="${#VAL}"
    VAL_HASH="$(value_hash "$VAL")"

    # CTRL-AUDIT-EMIT-BEFORE-MUTATION: log intent before mutation.
    log "[$N/$SECRET_COUNT] putting $name ... (length=${VAL_LEN} chars; SHA256[:8]=${VAL_HASH})"

    # Push secret. Value piped via stdin — never passed as CLI arg or logged.
    if ! printf '%s' "$VAL" | $WRANGLER_CMD secret put "$name" --env "$WRANGLER_ENV" 2>&1; then
        err "[$N/$SECRET_COUNT] FAILED to put $name"
        rollback "$name"
        exit 1
    fi

    SUCCESSFULLY_PUT+=("$name")
    log "[$N/$SECRET_COUNT] $name ... OK (length=${VAL_LEN}; SHA256[:8]=${VAL_HASH})"
done

log ""
log "Phase D secret put COMPLETE."
log "  env=$WRANGLER_ENV"
log "  secrets_put=$SECRET_COUNT"
log "  timestamp=$TIMESTAMP"
log ""
log "Next step: run scripts/verify-secrets-deployed.sh to confirm deployment."
exit 0
