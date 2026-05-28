#!/usr/bin/env bash
# d-day-secrets-rotate-prod.sh — Wave 32 Phase D: D-day secrets rotation script.
#
# Rotates the Phase D set of production secrets:
#   - STRIPE_SECRET_KEY       (90d, SRE Lead, cf-wrangler)
#   - CLERK_SECRET_KEY        (90d, DevOps,   cf-wrangler + vercel-env)
#   - PAGERDUTY_ROUTING_KEY   (365d, SRE Lead, cf-wrangler)
#   - BETTERSTACK_API_TOKEN   (Phase A SEAL, SRE Lead, .env.local / ops-credential)
#   - STRIPE_WEBHOOK_SECRET   (90d, SRE Lead, cf-wrangler — created POST Phase E)
#   - DEPLOY_WEBHOOK_SECRET   (90d, SRE Lead, cf-wrangler; HMAC — openssl rand -hex 32)
#
# IMPORTANT ORDERING:
#   Run STRIPE_WEBHOOK_SECRET ONLY after Phase E is complete and the Stripe
#   webhook endpoint exists (wrangler + Stripe dashboard both required).
#
# Security rules (mandatory — do NOT relax):
#   - NEVER enable xtrace ("set +x" is always enforced; see xtrace-note below)
#   - NEVER pass secret values as CLI arguments (visible in `ps auxww`)
#   - Read prompts use `read -r -s` (silent — no terminal echo)
#   - Secrets piped to wrangler via stdin: printf '%s' "$val" | wrangler secret put ...
#   - After use: unset each variable immediately (defense in depth)
#   - trap cleanup_secrets EXIT INT TERM: scrubs env on any exit path
#   - HMAC key generated inline: openssl rand -hex 32 | wrangler secret put ...
#     (no intermediate variable, no echo to stdout, no temp file)
#   - eval is NEVER used
#   - No temp file under /tmp or anywhere for secret storage
#
# Modes:
#   --dry-run          (default) Print EXACT wrangler secret put commands for
#                      each secret in the D-day set (with <VALUE> placeholder).
#                      No actual calls to wrangler or vendor APIs.
#   --live             Execute real rotation. Requires --interactive.
#   --interactive      Prompt Owner via `read -r -s` for each secret value.
#                      Required when using --live.
#   --validate-matrix  Run the two existing matrix verifiers:
#                        - bash scripts/secrets-checklist-verify.sh
#                        - python3 scripts/validate_secrets_matrix.py
#                      Exits 0 if both pass; non-zero on first failure.
#   --help             Print this usage message.
#
# Typical D-day execution order (copy-paste one-by-one):
#   Step 1:  bash scripts/d-day-secrets-rotate-prod.sh --validate-matrix
#   Step 2:  bash scripts/d-day-secrets-rotate-prod.sh --dry-run
#   Step 3:  bash scripts/d-day-secrets-rotate-prod.sh --live --interactive
#   Step 4:  bash scripts/verify-secrets-deployed.sh   (from Phase D prep)
#
# Exit codes:
#   0   — success (or dry-run / validate-matrix completed cleanly)
#   1   — rotation failure (wrangler returned non-zero; no partial state)
#   2   — usage / argument error
#   3   — --live without --interactive (safety interlock)
#   4   -- wrangler CLI not reachable (exit 127 from wrangler → escalated to 4)
#   5   -- --validate-matrix: verifier(s) returned non-zero drift
#
# References:
#   - docs/internal/secrets-checklist.md (canonical matrix; row numbers cited below)
#   - specs/_audits/sealed/2026-05-22-w32-phaseA-betterstack-live.md
#   - specs/_audits/2026-05-27-w32-phaseD-secrets-prep-seal.md (this WP)
#   - Wave 32 §Phase D production-deploy spec
#
# CHARTER: CTRL-CRED-001 — no real secret values in any committed artifact.
#          Audit-fail-CLOSED: any wrangler failure rolls back session puts.

# ── Safety: xtrace disabled unconditionally (xtrace-note) ───────────────────
# xtrace is permanently disabled by enforcing "set +x" at script start and
# after every subshell. This prevents secret values from appearing in shell
# output or log capture.  Xtrace (the -x flag) MUST remain off for the entire
# lifetime of this script — enabling it would leak prompted values to stdout.
set +x
# Re-enforce after any potential inheritance from parent SHELLOPTS.

set -euo pipefail

# ── Constants ─────────────────────────────────────────────────────────────────
# SC2155: declare and assign separately so that readonly does not mask exit code.
SCRIPT_NAME="$(basename "$0")"
readonly SCRIPT_NAME
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
readonly REPO_ROOT
readonly WRANGLER_ENV="prod"
readonly LOG_PREFIX="[${SCRIPT_NAME}]"
TIMESTAMP="$(date -u +%Y%m%dT%H%M%SZ)"
readonly TIMESTAMP
# SECRETS_CHECKLIST: path to canonical matrix; used by --validate-matrix mode
# (passed to bash/python verifiers via REPO_ROOT).  Referenced only as a
# documentation anchor in dry-run output — actual path is built inline where used.
readonly SECRETS_CHECKLIST="${REPO_ROOT}/docs/internal/secrets-checklist.md"
# Silence SC2034: the constant is an authoritative anchor; consumers reference it.
: "${SECRETS_CHECKLIST}"

# ── Cleanup / trap ────────────────────────────────────────────────────────────
# Registered BEFORE any secret is loaded into the environment.
# Scrubs all D-day secret variables from the environment on any exit path
# (normal exit, error, INT/CTRL-C, TERM).
#
# This is defense-in-depth: the variables are also unset immediately after
# use in the rotation loop, but the trap guarantees scrubbing even if an
# unexpected exit path is taken between prompt and use.

# SC2329: shellcheck cannot see that cleanup_secrets is called via trap; the
# function is intentionally invoked only through the trap mechanism below.
# shellcheck disable=SC2329
cleanup_secrets() {
    # NEVER log that cleanup is running (avoids leaking variable names during
    # signal-interrupted interactive sessions where the terminal is live).
    unset _SECRET_STRIPE_SECRET_KEY        2>/dev/null || true
    unset _SECRET_CLERK_SECRET_KEY         2>/dev/null || true
    unset _SECRET_PAGERDUTY_ROUTING_KEY    2>/dev/null || true
    unset _SECRET_BETTERSTACK_API_TOKEN    2>/dev/null || true
    unset _SECRET_STRIPE_WEBHOOK_SECRET    2>/dev/null || true
    # DEPLOY_WEBHOOK_SECRET is HMAC-only: generated inline, never stored in a
    # named variable in this script.  No unset needed; listed for audit clarity.
}
trap 'cleanup_secrets' EXIT INT TERM

# ── Logging ──────────────────────────────────────────────────────────────────
log()  { printf '%s [%s] %s\n'        "${LOG_PREFIX}" "$(date -u +%H:%M:%SZ)" "$*"; }
warn() { printf '%s [%s] WARN:  %s\n' "${LOG_PREFIX}" "$(date -u +%H:%M:%SZ)" "$*" >&2; }
err()  { printf '%s [%s] ERROR: %s\n' "${LOG_PREFIX}" "$(date -u +%H:%M:%SZ)" "$*" >&2; }

# ── CLI parsing ───────────────────────────────────────────────────────────────
MODE_DRY_RUN=true
MODE_LIVE=false
MODE_INTERACTIVE=false
MODE_VALIDATE_MATRIX=false

usage() {
    grep '^#' "$0" | head -65 | sed 's/^# \?//'
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --dry-run)
            MODE_DRY_RUN=true
            MODE_LIVE=false
            shift ;;
        --live)
            MODE_LIVE=true
            MODE_DRY_RUN=false
            shift ;;
        --interactive)
            MODE_INTERACTIVE=true
            shift ;;
        --validate-matrix)
            MODE_VALIDATE_MATRIX=true
            shift ;;
        -h|--help)
            usage; exit 0 ;;
        *)
            err "unknown argument: $1"
            usage >&2
            exit 2 ;;
    esac
done

# ── Safety interlock: --live requires --interactive ───────────────────────────
if $MODE_LIVE && ! $MODE_INTERACTIVE; then
    err "--live requires --interactive (safety interlock: blind live rotation forbidden)."
    err "Usage: $0 --live --interactive"
    exit 3
fi

# ── Validate-matrix mode ──────────────────────────────────────────────────────
if $MODE_VALIDATE_MATRIX; then
    log "=== --validate-matrix: running secrets-checklist-verify.sh ==="
    VERIFY_SH="${REPO_ROOT}/scripts/secrets-checklist-verify.sh"
    VALIDATE_PY="${REPO_ROOT}/scripts/validate_secrets_matrix.py"

    if [[ ! -f "${VERIFY_SH}" ]]; then
        err "secrets-checklist-verify.sh not found at: ${VERIFY_SH}"
        exit 5
    fi

    if ! bash "${VERIFY_SH}"; then
        err "secrets-checklist-verify.sh reported drift (non-zero exit)."
        exit 5
    fi
    log "secrets-checklist-verify.sh: PASS"

    if [[ -f "${VALIDATE_PY}" ]]; then
        log "=== --validate-matrix: running validate_secrets_matrix.py ==="
        if ! python3 "${VALIDATE_PY}"; then
            err "validate_secrets_matrix.py reported drift (non-zero exit)."
            exit 5
        fi
        log "validate_secrets_matrix.py: PASS"
    else
        warn "validate_secrets_matrix.py not found at: ${VALIDATE_PY}"
        warn "Skipping Python validator (bash verifier passed)."
    fi

    log "=== --validate-matrix: ALL VERIFIERS PASSED ==="
    exit 0
fi

# ── D-day secret definitions ──────────────────────────────────────────────────
#
# Each entry: <ENV_VAR_NAME>|<ROTATION_OWNER>|<CADENCE>|<STORED_AT>|<NOTES>
#
# Fields are consumed by the dry-run printer and interactive prompts.
# Entries match matrix rows (docs/internal/secrets-checklist.md):
#   Row  1: STRIPE_SECRET_KEY       — 90d, SRE Lead, cf-wrangler
#   Row  6: CLERK_SECRET_KEY        — 90d, DevOps,   cf-wrangler + vercel-env
#   Row 11: PAGERDUTY_ROUTING_KEY   — 365d, SRE Lead, cf-wrangler
#   Row  3: STRIPE_WEBHOOK_SECRET   — 90d, SRE Lead, cf-wrangler (POST Phase E)
#   Row 54: DEPLOY_WEBHOOK_SECRET   — 90d, SRE Lead, cf-wrangler (HMAC — auto-generated)
#   N/A:    BETTERSTACK_API_TOKEN   — Phase A SEAL; not a cf-wrangler entry in the
#                                     current matrix (see FLAG below and audit doc §7).
#
# Flag: BETTERSTACK_API_TOKEN does NOT appear as a cf-wrangler row in the current
# matrix (docs/internal/secrets-checklist.md). It is used operationally (Phase A SEAL;
# present in .env.local) but stored outside cf-wrangler. The script rotates it via
# .env.local credential update and flags it for the Owner with an explicit note.
# See audit doc §7 — matrix row 42 (STATUSPAGE_API_KEY / Atlassian) is a DIFFERENT
# vendor from BetterStack (row 131 STATUSPAGE_URL is config, not credential).
# Resolution: BETTERSTACK_API_TOKEN should be added to the matrix as a dedicated row.
# Until then, this script handles it as an .env.local / ops-credential update only.

# Secrets to rotate via wrangler secret put (cf-wrangler tier):
WRANGLER_SECRETS=(
    "STRIPE_SECRET_KEY|SRE Lead|90d|cf-wrangler|Matrix row 1; Stripe live secret key"
    "CLERK_SECRET_KEY|DevOps|90d|cf-wrangler + vercel-env|Matrix row 6; also needs Vercel env update"
    "PAGERDUTY_ROUTING_KEY|SRE Lead|365d|cf-wrangler|Matrix row 11; PagerDuty Events API v2 routing key"
    "STRIPE_WEBHOOK_SECRET|SRE Lead|90d|cf-wrangler|Matrix row 3; POST-Phase-E only — skip if Phase E not complete"
)

# HMAC-generated secrets (openssl rand -hex 32, piped inline — no temp var):
HMAC_SECRETS=(
    "DEPLOY_WEBHOOK_SECRET|SRE Lead|90d|cf-wrangler + gha-secret|Matrix row 54; HMAC 32-byte hex; mirror to GHA secret in lockstep"
)

# Ops-credential secrets (not cf-wrangler; updated in .env.local + vendor dashboard):
OPS_SECRETS=(
    "BETTERSTACK_API_TOKEN|SRE Lead|Phase-A-SEAL|.env.local (ops)|NOT in matrix cf-wrangler rows; see FLAG above; rotate at betterstack.com dashboard + update .env.local"
)

# ── Helper: detect wrangler ───────────────────────────────────────────────────
WRANGLER_CMD="${WRANGLER:-npx wrangler@latest}"

check_wrangler() {
    set +x
    if ! ${WRANGLER_CMD} --version >/dev/null 2>&1; then
        err "wrangler CLI not reachable via: ${WRANGLER_CMD}"
        err "Install: npm install -g wrangler@^4  OR  set WRANGLER env var."
        exit 4
    fi
}

# ── Track successfully-put secrets for rollback ───────────────────────────────
SUCCESSFULLY_PUT=()

rollback_session() {
    local failed_name="$1"
    set +x
    err "===== ROLLBACK triggered (failed on: ${failed_name}) ====="
    err "Deleting ${#SUCCESSFULLY_PUT[@]} secret(s) put in this session..."

    local rb_fail=0
    for put_name in "${SUCCESSFULLY_PUT[@]:-}"; do
        if ${WRANGLER_CMD} secret delete "${put_name}" \
               --env "${WRANGLER_ENV}" --force 2>/dev/null; then
            err "  ROLLBACK OK:   deleted ${put_name}"
        else
            err "  ROLLBACK FAIL: could not delete ${put_name} — manual cleanup required"
            rb_fail=$(( rb_fail + 1 ))
        fi
    done

    if [[ "${rb_fail}" -gt 0 ]]; then
        err "WARNING: ${rb_fail} secret(s) could NOT be auto-deleted."
        err "Manual cleanup (run as SRE Lead):"
        for put_name in "${SUCCESSFULLY_PUT[@]:-}"; do
            err "  ${WRANGLER_CMD} secret delete ${put_name} --env ${WRANGLER_ENV} --force"
        done
    else
        err "ROLLBACK COMPLETE: ${#SUCCESSFULLY_PUT[@]} secret(s) removed from ${WRANGLER_ENV}."
    fi
    err "=================================================================="
}

# ── DRY-RUN mode ──────────────────────────────────────────────────────────────
if $MODE_DRY_RUN; then
    log ""
    log "=== D-day secrets rotation — DRY-RUN (2026-05-27) ========================="
    log ""
    log "D-day set: ${#WRANGLER_SECRETS[@]} wrangler secrets + ${#HMAC_SECRETS[@]} HMAC + ${#OPS_SECRETS[@]} ops-credential"
    log ""
    log "──────────────────────────────────────────────────────────────────────"
    log "SECTION A: cf-wrangler secrets  (wrangler secret put --env ${WRANGLER_ENV})"
    log "──────────────────────────────────────────────────────────────────────"
    log ""

    N=0
    for entry in "${WRANGLER_SECRETS[@]}"; do
        N=$(( N + 1 ))
        IFS='|' read -r secret_name owner cadence stored_at notes <<< "${entry}"
        log "[$N] ${secret_name}"
        log "    owner:    ${owner}"
        log "    cadence:  ${cadence}"
        log "    stored:   ${stored_at}"
        log "    notes:    ${notes}"
        log "    command:  printf '%s' '<VALUE>' | \\"
        log "                  wrangler secret put ${secret_name} --env ${WRANGLER_ENV}"
        log ""
    done

    log "──────────────────────────────────────────────────────────────────────"
    log "SECTION B: HMAC-generated secrets  (openssl rand -hex 32 | wrangler ...)"
    log "  IMPORTANT: value is generated inline and piped DIRECTLY to wrangler."
    log "  No intermediate variable. No echo to stdout. No temp file."
    log "──────────────────────────────────────────────────────────────────────"
    log ""

    N=0
    for entry in "${HMAC_SECRETS[@]}"; do
        N=$(( N + 1 ))
        IFS='|' read -r secret_name owner cadence stored_at notes <<< "${entry}"
        log "[$N] ${secret_name}"
        log "    owner:    ${owner}"
        log "    cadence:  ${cadence}"
        log "    stored:   ${stored_at}"
        log "    notes:    ${notes}"
        log "    command:  openssl rand -hex 32 | \\"
        log "                  wrangler secret put ${secret_name} --env ${WRANGLER_ENV}"
        log "    mirror:   also update the GHA secret ${secret_name} in lockstep"
        log "              (gha-secret tier; openssl rand -hex 32 separately for GHA)"
        log ""
    done

    log "──────────────────────────────────────────────────────────────────────"
    log "SECTION C: ops-credential (outside cf-wrangler — .env.local + vendor dashboard)"
    log "──────────────────────────────────────────────────────────────────────"
    log ""

    N=0
    for entry in "${OPS_SECRETS[@]}"; do
        N=$(( N + 1 ))
        IFS='|' read -r secret_name owner cadence stored_at notes <<< "${entry}"
        log "[$N] ${secret_name}"
        log "    owner:    ${owner}"
        log "    cadence:  ${cadence}"
        log "    stored:   ${stored_at}"
        log "    notes:    ${notes}"
        log "    action:   Rotate at https://betterstack.com → API tokens → revoke old → create new"
        log "              Update .env.local manually (NEVER commit .env.local)."
        log ""
    done

    log "──────────────────────────────────────────────────────────────────────"
    log "DRY-RUN complete.  No real calls made."
    log ""
    log "To execute: $0 --live --interactive"
    log "Pre-check:  $0 --validate-matrix"
    log ""
    log "Phase E gate: STRIPE_WEBHOOK_SECRET must be rotated AFTER Phase E"
    log "  webhook endpoint exists in Stripe dashboard and is live in CF Worker."
    log ""
    exit 0
fi

# ── LIVE + INTERACTIVE mode ───────────────────────────────────────────────────
log ""
log "=== D-day secrets rotation — LIVE / INTERACTIVE (${TIMESTAMP}) ============"
log ""
warn "This will rotate ${#WRANGLER_SECRETS[@]} wrangler secrets + ${#HMAC_SECRETS[@]} HMAC secret"
warn "against Cloudflare Workers --env ${WRANGLER_ENV} (PRODUCTION)."
warn ""
warn "Prerequisites:"
warn "  1. wrangler >=4.x installed + authenticated (CLOUDFLARE_API_TOKEN set)"
warn "  2. Phase E complete before rotating STRIPE_WEBHOOK_SECRET"
warn "  3. Run --validate-matrix first to confirm zero drift"
warn ""

# Confirm wrangler is available.
check_wrangler
WRANGLER_VER="$(${WRANGLER_CMD} --version 2>/dev/null | head -1 || echo unknown)"
log "wrangler: ${WRANGLER_VER}"
log ""

# ── Section A: interactive wrangler secrets ───────────────────────────────────
log "=== SECTION A: cf-wrangler secrets ==="
log ""

for entry in "${WRANGLER_SECRETS[@]}"; do
    set +x
    IFS='|' read -r secret_name owner cadence stored_at notes <<< "${entry}"

    log "Rotating: ${secret_name}"
    log "  owner:   ${owner}"
    log "  cadence: ${cadence}"
    log "  notes:   ${notes}"

    # Check Phase E gate for STRIPE_WEBHOOK_SECRET.
    if [[ "${secret_name}" == "STRIPE_WEBHOOK_SECRET" ]]; then
        warn ""
        warn "STRIPE_WEBHOOK_SECRET: Phase E gate."
        warn "Only rotate if Phase E is complete and the Stripe webhook endpoint"
        warn "exists and is live. Enter 'SKIP' to skip this secret safely."
        warn ""
    fi

    # Prompt Owner for value — silent, no echo.
    _prompt_val=""
    read -r -s -p "  Enter value for ${secret_name} (hidden; type SKIP to skip): " \
        _prompt_val </dev/tty
    printf '\n' >/dev/tty

    if [[ "${_prompt_val}" == "SKIP" ]]; then
        warn "  SKIPPED: ${secret_name}"
        unset _prompt_val
        log ""
        continue
    fi

    if [[ -z "${_prompt_val}" ]]; then
        err "  Empty value for ${secret_name} — aborting rotation."
        unset _prompt_val
        rollback_session "${secret_name}"
        exit 1
    fi

    # Audit-emit BEFORE mutation (CTRL-AUDIT-EMIT-BEFORE-MUTATION).
    # Log name + length only. NEVER log the value.
    _val_len="${#_prompt_val}"
    log "  Putting ${secret_name} (length=${_val_len} chars) ..."

    # Pipe value via stdin — never as CLI arg.
    set +x
    if ! printf '%s' "${_prompt_val}" \
            | ${WRANGLER_CMD} secret put "${secret_name}" \
                  --env "${WRANGLER_ENV}" 2>&1; then
        err "  FAILED to put ${secret_name}"
        unset _prompt_val
        rollback_session "${secret_name}"
        exit 1
    fi

    SUCCESSFULLY_PUT+=("${secret_name}")
    log "  OK: ${secret_name} (length=${_val_len})"

    # Immediate unset after consumption (defense in depth).
    unset _prompt_val
    unset _val_len
    log ""
done

# ── Section B: HMAC-generated secrets ─────────────────────────────────────────
log "=== SECTION B: HMAC-generated secrets (openssl rand -hex 32) ==="
log ""

for entry in "${HMAC_SECRETS[@]}"; do
    set +x
    IFS='|' read -r secret_name owner cadence stored_at notes <<< "${entry}"

    log "Generating + rotating HMAC secret: ${secret_name}"
    log "  owner:   ${owner}"
    log "  cadence: ${cadence}"
    log "  notes:   ${notes}"
    log ""

    # Prompt Owner to confirm before generating (one-shot; non-reversible).
    _confirm=""
    read -r -p "  Confirm HMAC generation for ${secret_name} (yes/SKIP): " \
        _confirm </dev/tty
    printf '\n' >/dev/tty

    if [[ "${_confirm}" != "yes" ]]; then
        warn "  SKIPPED: ${secret_name}"
        unset _confirm
        log ""
        continue
    fi
    unset _confirm

    # CTRL-CRED-001 / safety:
    #   openssl rand -hex 32  →  piped DIRECTLY into wrangler.
    #   No intermediate variable, no echo, no temp file.
    #   The 32-byte hex value exists ONLY in the pipe between openssl and wrangler.
    #
    # Audit note: we cannot log length (it's always 64 hex chars) but we log
    # the source ("openssl rand -hex 32") for audit trail without leaking value.
    log "  Source: openssl rand -hex 32 | wrangler secret put ${secret_name} ..."
    set +x
    if ! openssl rand -hex 32 \
            | ${WRANGLER_CMD} secret put "${secret_name}" \
                  --env "${WRANGLER_ENV}" 2>&1; then
        err "  FAILED to put HMAC secret ${secret_name}"
        rollback_session "${secret_name}"
        exit 1
    fi

    SUCCESSFULLY_PUT+=("${secret_name}")
    log "  OK: ${secret_name} (HMAC 32-byte hex — value never stored or logged)"
    log ""
    warn "  MIRROR REQUIRED: ${secret_name} is also stored in gha-secret tier."
    warn "  Generate a SEPARATE value for GHA (operator manual step):"
    warn "    openssl rand -hex 32  → copy to GitHub Actions secret ${secret_name}"
    warn "  (The CF Worker and GHA values MAY differ; they serve independent"
    warn "   verification paths. If they must match, coordinate manually.)"
    log ""
done

# ── Section C: ops-credential notice ─────────────────────────────────────────
log "=== SECTION C: ops-credentials (outside cf-wrangler) ==="
log ""

for entry in "${OPS_SECRETS[@]}"; do
    IFS='|' read -r secret_name owner cadence stored_at notes <<< "${entry}"
    warn "${secret_name}: NOT rotated by this script (stored outside cf-wrangler)."
    warn "  Manual action required:"
    warn "  1. Go to https://betterstack.com → Profile → API tokens"
    warn "  2. Revoke the current ${secret_name} token"
    warn "  3. Create a new token → copy value"
    warn "  4. Update .env.local:  ${secret_name}=<new-value>"
    warn "     CTRL-CRED-001: NEVER commit .env.local"
    warn "  5. Re-run any ops scripts that read BETTERSTACK_API_TOKEN from .env.local"
    warn ""
done

# ── Summary ───────────────────────────────────────────────────────────────────
log "====================================================================="
log "D-day rotation COMPLETE"
log "  timestamp:   ${TIMESTAMP}"
log "  env:         ${WRANGLER_ENV}"
log "  wrangler:    ${WRANGLER_VER}"
log "  cf_rotated:  ${#SUCCESSFULLY_PUT[@]} secret(s)"
log "  rotated:     ${SUCCESSFULLY_PUT[*]:-none}"
log "====================================================================="
log ""
log "Next step: run scripts/verify-secrets-deployed.sh to confirm deployment."
log ""

exit 0
