#!/usr/bin/env bash
# put-secrets-regional.sh — push container-storage secrets to the 4 regional
# Worker envs (prod-sam, prod-lhr, prod-nrt, prod-syd).
#
# The regional Workers were deployed by Wave 32 multi-region phase 2 but do
# NOT have the secrets the container sidecar needs to reach R2 (S3 API) +
# CF API (R2_S3_ACCESS_KEY_ID, R2_S3_SECRET_ACCESS_KEY, CF_API_TOKEN,
# CLOUDFLARE_ACCOUNT_ID). Without these, any container-driven path (audit export, scrub,
# advanced storage ops) fails. Worker-only paths (CAS/AC binding read/write,
# auth, signup forwarding) work without them.
#
# Default mode: DRY-RUN. Pass --apply to actually call wrangler.
#
# Charter constraints (mirrors put-secrets-prod.sh):
#   CTRL-CRED-001: NO secret values hard-coded. Sourced from .env.local OR
#     prompted interactively.
#   CTRL-AUDIT-EMIT-BEFORE-MUTATION: each put is logged (name + length +
#     SHA256[:8]) BEFORE wrangler runs.
#   Rollback on failure: any wrangler put failure triggers `wrangler secret
#     delete` for every secret successfully put in this session.
#
# Usage:
#   scripts/put-secrets-regional.sh                              # dry-run
#   scripts/put-secrets-regional.sh --apply                      # all 4 regions
#   scripts/put-secrets-regional.sh --apply --region sam         # one region
#   scripts/put-secrets-regional.sh --apply --region sam,lhr     # subset
#
# Exit codes:
#   0   success / dry-run completed cleanly
#   1   wrangler put failure (rollback executed)
#   2   usage / argument error
#   127 wrangler not in PATH

set -euo pipefail

readonly SCRIPT_NAME="$(basename "$0")"
readonly REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
readonly DEFAULT_ENV_FILE="$REPO_ROOT/.env.local"
readonly DEFAULT_WRANGLER="$REPO_ROOT/worker/node_modules/.bin/wrangler"

readonly ALL_REGIONS=(sam lhr nrt syd)
# CLOUDFLARE_ACCOUNT_ID added 2026-06-08 (#33): StorageEnv::from_env (storage.rs:84)
# requires it; without it the regional container falls back to InMemory storage =
# DATA LOSS. wrangler.toml:551 already documents it in the per-region secret set,
# but it was missing from this push list.
readonly SECRETS=(R2_S3_ACCESS_KEY_ID R2_S3_SECRET_ACCESS_KEY CF_API_TOKEN CLOUDFLARE_ACCOUNT_ID)

# Logging helpers (no secret values ever).
LOG_PREFIX="[$SCRIPT_NAME]"
log()  { printf '%s %s\n' "$LOG_PREFIX" "$*"; }
warn() { printf '%s WARN: %s\n' "$LOG_PREFIX" "$*" >&2; }
err()  { printf '%s ERROR: %s\n' "$LOG_PREFIX" "$*" >&2; }

# ── 1. Args ──────────────────────────────────────────────────────────────────

APPLY_MODE=false
ENV_FILE="$DEFAULT_ENV_FILE"
REGIONS=("${ALL_REGIONS[@]}")

while [[ $# -gt 0 ]]; do
    case "$1" in
        --apply)        APPLY_MODE=true; shift ;;
        --env-file)     ENV_FILE="${2:-}"; shift 2 ;;
        --region)
            IFS=',' read -ra REGIONS <<<"${2:-}"
            shift 2
            ;;
        --help|-h)
            sed -n '1,30p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *) err "unknown arg: $1"; exit 2 ;;
    esac
done

# Validate regions.
for r in "${REGIONS[@]}"; do
    valid=false
    for a in "${ALL_REGIONS[@]}"; do [[ "$a" == "$r" ]] && valid=true; done
    if ! $valid; then
        err "unknown region: $r (valid: ${ALL_REGIONS[*]})"
        exit 2
    fi
done

# ── 2. Source env file ───────────────────────────────────────────────────────

if [[ -f "$ENV_FILE" ]]; then
    log "sourcing env file: $ENV_FILE"
    set +u
    # shellcheck disable=SC1090
    source "$ENV_FILE" || { err "failed to source $ENV_FILE"; exit 1; }
    set -u
else
    warn "env file not found: $ENV_FILE"
    warn "will prompt interactively for each secret value (if --apply)"
fi

# ── 3. Helpers ───────────────────────────────────────────────────────────────

SUCCESSFULLY_PUT=()  # entries: "env:NAME"

get_secret() {
    local name="$1"
    local val=""
    if [[ -n "${!name:-}" ]]; then
        val="${!name}"
    else
        if $APPLY_MODE; then
            read -r -s -p "Enter value for $name (input hidden): " val </dev/tty
            printf '\n' >/dev/tty
            if [[ -z "$val" ]]; then
                err "empty value provided for $name — aborting"
                return 1
            fi
        else
            val="<NOT_SET_IN_ENV>"
        fi
    fi
    printf '%s' "$val"
}

value_hash() {
    local val="$1"
    printf '%s' "$val" | shasum -a 256 2>/dev/null | cut -c1-8 \
        || printf '%s' "$val" | sha256sum 2>/dev/null | cut -c1-8 \
        || echo "hash-unavailable"
}

rollback() {
    local failed="$1"
    err "========================================="
    err "ROLLBACK triggered (failed on: $failed)"
    err "Deleting ${#SUCCESSFULLY_PUT[@]} already-put secret(s) in this session..."
    for entry in "${SUCCESSFULLY_PUT[@]}"; do
        local env="${entry%%:*}"
        local name="${entry#*:}"
        if $WRANGLER_CMD secret delete "$name" --env "$env" --force 2>/dev/null; then
            err "  ROLLBACK OK: deleted $name from $env"
        else
            err "  ROLLBACK FAIL: $name in $env — manual cleanup required"
            err "  $WRANGLER_CMD secret delete $name --env $env --force"
        fi
    done
    err "========================================="
}

# ── 4. Plan ──────────────────────────────────────────────────────────────────

TOTAL=$(( ${#REGIONS[@]} * ${#SECRETS[@]} ))
log ""
log "Secret put plan:"
log "  regions: ${REGIONS[*]}"
log "  secrets per region: ${SECRETS[*]}"
log "  total puts: $TOTAL"
log ""
log "Pre-check — values resolvable from current env:"
for s in "${SECRETS[@]}"; do
    if [[ -n "${!s:-}" ]]; then
        log "  $s: present (length=${#:-0}; SHA256[:8]=$(value_hash "${!s}"))"
    else
        warn "  $s: NOT IN ENV — will prompt interactively (--apply) or skip (dry-run)"
    fi
done
log ""

if ! $APPLY_MODE; then
    log "DRY-RUN complete. No secrets were put."
    log ""
    log "To apply: $0 --apply"
    log "Single region:  $0 --apply --region sam"
    exit 0
fi

# ── 5. Apply ─────────────────────────────────────────────────────────────────

# Same resolver bug as scripts/deploy-container-prod.sh (fixed 2026-08-04):
# `-x` is a PATH test, so a bare command name (e.g. WRANGLER=wrangler after a
# global npm install) was silently discarded and replaced by an unpinned
# `npx wrangler@latest`. Honour both forms; keep the fallback PINNED.
WRANGLER_CMD="${WRANGLER:-$DEFAULT_WRANGLER}"
if [[ ! -x "$WRANGLER_CMD" ]] && ! command -v "$WRANGLER_CMD" >/dev/null 2>&1; then
    # shellcheck source=scripts/_wrangler-pin.sh
    . "$REPO_ROOT/scripts/_wrangler-pin.sh"
    warn "wrangler not found at '$WRANGLER_CMD' and not on \$PATH; falling back to 'npx wrangler@${WRANGLER_PINNED_VERSION}'"
    WRANGLER_CMD="npx wrangler@${WRANGLER_PINNED_VERSION}"
fi

if ! $WRANGLER_CMD --version >/dev/null 2>&1; then
    err "wrangler CLI not reachable via: $WRANGLER_CMD"
    exit 127
fi

WRANGLER_VER="$($WRANGLER_CMD --version 2>/dev/null | head -1 || echo unknown)"
log "wrangler: $WRANGLER_VER"
log ""
log "APPLY MODE: pushing $TOTAL secrets across ${#REGIONS[@]} regions"
log ""

N=0
for region in "${REGIONS[@]}"; do
    env="prod-$region"
    for name in "${SECRETS[@]}"; do
        N=$((N + 1))

        VAL="$(get_secret "$name")" || { rollback "$env:$name"; exit 1; }
        VAL_LEN="${#VAL}"
        VAL_HASH="$(value_hash "$VAL")"

        log "[$N/$TOTAL] $env :: $name ... (length=$VAL_LEN; SHA256[:8]=$VAL_HASH)"

        if ! printf '%s' "$VAL" | $WRANGLER_CMD secret put "$name" --env "$env" 2>&1 | tail -3; then
            err "[$N/$TOTAL] FAILED on $env :: $name"
            rollback "$env:$name"
            exit 1
        fi

        SUCCESSFULLY_PUT+=("$env:$name")
        log "[$N/$TOTAL] $env :: $name OK"
    done
done

log ""
log "Regional secret put COMPLETE."
log "  regions: ${REGIONS[*]}"
log "  secrets_put: $TOTAL"
log ""
log "Next step:"
log "  bash scripts/verify-regional-secrets.sh    # confirms presence per region"
log "  bash scripts/smoke-regional-data-plane.sh  # exercises container path"
exit 0
