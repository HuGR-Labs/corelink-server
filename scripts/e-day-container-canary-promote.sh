#!/usr/bin/env bash
# scripts/e-day-container-canary-promote.sh — Wave 32 Phase E
#
# Gradual canary promote script for CoreLink container deploy.
# Stages: 5% (10-min wait) → 25% (10-min wait) → 100%.
#
# Modes:
#   --dry-run   List every stage + wrangler command; no mutations. (DEFAULT)
#   --apply     Execute the sequence. Requires Workers Paid plan (≥$5/mo).
#               Enforces 10-min sleep between stages + Owner ack at each stage.
#
# Optional flags (only valid with --apply):
#   --accept-data-loss   Required alongside --force-do-purge. Acknowledges
#                        irreversible Durable Object state purge at 100% rollback.
#   --force-do-purge     Triggers `wrangler delete --env prod --force` instead of
#                        `wrangler rollback` during the 100% rollback path.
#                        REQUIRES --accept-data-loss. DATA LOSS IS IRREVERSIBLE.
#
# Usage:
#   bash scripts/e-day-container-canary-promote.sh                # dry-run
#   bash scripts/e-day-container-canary-promote.sh --dry-run      # dry-run (explicit)
#   bash scripts/e-day-container-canary-promote.sh --apply        # live canary sequence
#   bash scripts/e-day-container-canary-promote.sh --help
#
# Observability reminders printed at each stage:
#   - CF Workers Analytics URL
#   - Grafana DASH-SLO-API.json (uid DASH-SLO-API)
#   - Grafana DASH-SLO-AUDIT.json (uid DASH-SLO-AUDIT)
#   - BetterStack page 247652
#
# Charter compliance:
#   - Bash strict mode: set -euo pipefail
#   - CTRL-CRED-001: No credentials embedded in this script
#   - Owner ack via `read -r -p` before EVERY stage promotion (--apply mode)
#   - --force-do-purge REQUIRES BOTH --apply AND --accept-data-loss (defense in depth)
#   - OBSERVATION_SECONDS=600 enforced between stages; not skippable
#
# Exit codes:
#   0  — dry-run printed / all stages promoted successfully
#   1  — stage failed or Owner aborted (n/N response at ack prompt)
#   2  — usage error (bad flags)
#   3  — pre-flight check failed (missing wrangler, missing credentials)
#
# Companion runbook: specs/_runbooks/RB-W32-CONTAINER-CANARY.md
# Signed-off-by: Gustavo Schneiter <gustavo@humangr.com>
# Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>

set -euo pipefail

# ── Constants ──────────────────────────────────────────────────────────────

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

WORKER_ENV="prod"
OBSERVATION_SECONDS=600  # 10 minutes — NOT skippable

# BetterStack page ID (Phase A SEAL: specs/_audits/sealed/2026-05-22-w32-phaseA-betterstack-live.md)
BETTERSTACK_PAGE_ID="247652"

# Grafana dashboard paths (relative to REPO_ROOT)
DASH_SLO_API="dashboards/grafana/DASH-SLO-API.json"
DASH_SLO_AUDIT="dashboards/grafana/DASH-SLO-AUDIT.json"

# CF Workers Analytics URL pattern (replace <account_id> with actual value)
CF_ANALYTICS_URL="https://dash.cloudflare.com/\${CF_ACCOUNT_ID}/workers/services/view/corelink/production"

# ── State ──────────────────────────────────────────────────────────────────

DRY_RUN=true
APPLY=false
ACCEPT_DATA_LOSS=false
FORCE_DO_PURGE=false

# ── Helpers ────────────────────────────────────────────────────────────────

log()  { echo "[$(date -u +"%H:%M:%SZ")] $*"; }
# info() removed — unused function (shellcheck SC2329)
rule() { echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"; }

usage() {
  cat <<'EOF'
e-day-container-canary-promote.sh — Wave 32 Phase E canary promote

Modes:
  (default / --dry-run)   List every stage + wrangler command; no mutations.
  --apply                 Execute the gradual canary sequence live.

Flags (only valid with --apply):
  --accept-data-loss      Required for --force-do-purge. Acknowledges permanent DO loss.
  --force-do-purge        At rollback, use `wrangler delete --force` instead of rollback.
                          REQUIRES --accept-data-loss. THIS DESTROYS ALL DO STATE.

Other:
  --help                  Show this help and exit.

Canary stages:
  Stage 1 → 5%   deploy → 10-min observation → Owner ack → continue or rollback
  Stage 2 → 25%  deploy → 10-min observation → Owner ack → continue or rollback
  Stage 3 → 100% deploy → 10-min observation → Owner ack → sign-off or rollback

PASS criteria (all 4 required per stage):
  C1  Error rate within +0.5pp of pre-deploy baseline (CF Workers Analytics + Grafana DASH-SLO-API)
  C2  P95 latency within +20ms of pre-deploy baseline (CF Workers Analytics + Grafana DASH-SLO-API)
  C3  Zero CRITICAL Sentry errors if DSN provisioned (deferred/auto-PASS if DSN absent)
  C4  Zero BetterStack probe failures over 10-min window (BetterStack page 247652)

Rollback:
  Stages 5%/25%: wrangler rollback --env prod
  Stage 100%:    wrangler rollback --env prod
  Stage 100% DO corrupted: wrangler delete --env prod --force
                           (requires --apply --accept-data-loss --force-do-purge)

Example:
  bash scripts/e-day-container-canary-promote.sh --dry-run
  bash scripts/e-day-container-canary-promote.sh --apply

EOF
  exit 0
}

# ── Argument parsing ───────────────────────────────────────────────────────

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run)           DRY_RUN=true;  APPLY=false ;;
    --apply)             APPLY=true;    DRY_RUN=false ;;
    --accept-data-loss)  ACCEPT_DATA_LOSS=true ;;
    --force-do-purge)    FORCE_DO_PURGE=true ;;
    --help|-h)           usage ;;
    *)
      echo "ERROR: Unknown flag: $1" >&2
      echo "Run with --help for usage." >&2
      exit 2
      ;;
  esac
  shift
done

# Validate flag combinations
if $FORCE_DO_PURGE && ! $ACCEPT_DATA_LOSS; then
  echo "ERROR: --force-do-purge requires --accept-data-loss (defense in depth)." >&2
  echo "       This operation DESTROYS ALL Durable Object state permanently." >&2
  exit 2
fi

if $FORCE_DO_PURGE && ! $APPLY; then
  echo "ERROR: --force-do-purge requires --apply (cannot run in dry-run mode)." >&2
  exit 2
fi

if $ACCEPT_DATA_LOSS && ! $FORCE_DO_PURGE; then
  echo "WARNING: --accept-data-loss has no effect without --force-do-purge." >&2
fi

# ── Dry-run mode ───────────────────────────────────────────────────────────

if $DRY_RUN; then
  cat <<EOF

e-day-container-canary-promote.sh — DRY-RUN
Date: $(date -u +"%Y-%m-%dT%H:%M:%SZ")
Worker env: ${WORKER_ENV}
Observation window per stage: ${OBSERVATION_SECONDS}s (10 min)

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
STAGE 1 — 5% canary
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

  Promote command:
    npx wrangler@latest deploy \\
      --env ${WORKER_ENV} \\
      --percent 5 \\
      --message "Wave 32 Phase E — canary stage 1 (5%)"

  Observe for ${OBSERVATION_SECONDS}s (10 min). Monitor:
    - CF Workers Analytics: ${CF_ANALYTICS_URL}
    - Grafana DASH-SLO-API: ${REPO_ROOT}/${DASH_SLO_API} (uid: DASH-SLO-API)
    - Grafana DASH-SLO-AUDIT: ${REPO_ROOT}/${DASH_SLO_AUDIT} (uid: DASH-SLO-AUDIT)
    - BetterStack page ${BETTERSTACK_PAGE_ID}: https://status.corelink.humangr.com

  PASS criteria:
    C1 Error rate within +0.5pp of baseline (CF Analytics + Grafana DASH-SLO-API)
    C2 P95 latency within +20ms of baseline (CF Analytics + Grafana DASH-SLO-API)
    C3 Zero CRITICAL Sentry errors (auto-PASS if DSN not provisioned)
    C4 Zero BetterStack probe failures over 10-min window (page ${BETTERSTACK_PAGE_ID})

  On FAIL → rollback:
    npx wrangler@latest rollback --env ${WORKER_ENV}

  Owner ack required before proceeding to Stage 2.

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
STAGE 2 — 25% canary
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

  Promote command:
    npx wrangler@latest deploy \\
      --env ${WORKER_ENV} \\
      --percent 25 \\
      --message "Wave 32 Phase E — canary stage 2 (25%)"

  Observe for ${OBSERVATION_SECONDS}s (10 min). Same monitoring sources as Stage 1.

  PASS criteria: C1, C2, C3, C4 (same thresholds as Stage 1)

  On FAIL → rollback:
    npx wrangler@latest rollback --env ${WORKER_ENV}

  Owner ack required before proceeding to Stage 3.

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
STAGE 3 — 100% full promote
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

  Promote command:
    npx wrangler@latest deploy \\
      --env ${WORKER_ENV} \\
      --message "Wave 32 Phase E — full promote (100%)"

  Observe for ${OBSERVATION_SECONDS}s (10 min). Same monitoring sources.

  PASS criteria: C1, C2, C3, C4 (same thresholds)

  On FAIL (standard rollback):
    npx wrangler@latest rollback --env ${WORKER_ENV}

  On FAIL (DO state corrupted — REQUIRES --apply --accept-data-loss --force-do-purge):
    npx wrangler@latest delete --env ${WORKER_ENV} --force
    *** THIS DESTROYS ALL DURABLE OBJECT STATE PERMANENTLY ***

  Owner ack required for Phase E SEAL sign-off.

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
DRY-RUN COMPLETE — no changes made.
To execute: bash scripts/e-day-container-canary-promote.sh --apply
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

EOF
  exit 0
fi

# ── Apply mode — pre-flight checks ─────────────────────────────────────────

rule
log "e-day-container-canary-promote.sh — APPLY MODE"
log "Repo root: ${REPO_ROOT}"
rule

# Check wrangler is available
if ! command -v npx &>/dev/null; then
  log "ERROR: npx not found. Install Node.js ≥18." >&2
  exit 3
fi

WRANGLER_VERSION="$(npx wrangler@latest --version 2>/dev/null || true)"
if [[ -z "$WRANGLER_VERSION" ]]; then
  log "ERROR: wrangler not available via npx." >&2
  exit 3
fi
log "Wrangler: ${WRANGLER_VERSION}"

# Check credentials
if [[ -z "${CF_API_TOKEN:-}" ]]; then
  log "ERROR: CF_API_TOKEN is not set. Export before running." >&2
  exit 3
fi
if [[ -z "${CF_ACCOUNT_ID:-}" ]]; then
  log "ERROR: CF_ACCOUNT_ID is not set. Export before running." >&2
  exit 3
fi
log "Credentials: CF_API_TOKEN set (${#CF_API_TOKEN} chars), CF_ACCOUNT_ID=${CF_ACCOUNT_ID}"

# Check wrangler.toml present
if [[ ! -f "${REPO_ROOT}/wrangler.toml" ]]; then
  log "ERROR: wrangler.toml not found at repo root." >&2
  exit 3
fi
log "wrangler.toml: found"

rule

# ── Helper: print monitoring reminder ──────────────────────────────────────

print_monitoring_reminder() {
  local stage="$1"
  local pct="$2"
  cat <<EOF

  MONITORING REMINDER — Stage ${stage} (${pct}%)
  ─────────────────────────────────────────────────────────────────────────
  Watch ALL of the following during the ${OBSERVATION_SECONDS}s (10-min) window:

  1. CF Workers Analytics:
     https://dash.cloudflare.com/${CF_ACCOUNT_ID}/workers/services/view/corelink/production
     → Error rate (must stay within +0.5pp of baseline)
     → P95 latency (must stay within +20ms of baseline)

  2. Grafana DASH-SLO-API (uid: DASH-SLO-API):
     Source: ${REPO_ROOT}/${DASH_SLO_API}
     → Error rate time-series panel
     → P95 latency per-endpoint panel

  3. Grafana DASH-SLO-AUDIT (uid: DASH-SLO-AUDIT):
     Source: ${REPO_ROOT}/${DASH_SLO_AUDIT}
     → Audit chain write latency + error rate

  4. BetterStack page ${BETTERSTACK_PAGE_ID}:
     https://status.corelink.humangr.com
     → All 7 synthetic probes must remain GREEN

  5. Sentry: DEFERRED (auto-PASS until DSN is provisioned — Phase H)

  PASS criteria (all required):
    C1 Error rate Δ ≤ +0.5pp vs baseline
    C2 P95 latency Δ ≤ +20ms vs baseline
    C3 Zero CRITICAL Sentry errors (auto-PASS if DSN absent)
    C4 Zero BetterStack probe failures in 10-min window
  ─────────────────────────────────────────────────────────────────────────

EOF
}

# ── Helper: Owner ack gate ─────────────────────────────────────────────────

owner_ack() {
  local stage="$1"
  local next_action="$2"
  local response
  echo ""
  read -r -p "  Stage ${stage} looks clean? Proceed to ${next_action}? (y/n): " response
  echo ""
  case "$response" in
    [Yy]|[Yy][Ee][Ss]) return 0 ;;
    *)
      log "Owner declined to proceed at Stage ${stage}. Initiating rollback."
      return 1
      ;;
  esac
}

# ── Helper: rollback ───────────────────────────────────────────────────────

do_rollback() {
  local stage="$1"
  local do_purge="${2:-false}"

  if [[ "$do_purge" == "true" ]]; then
    # This path requires both --apply AND --accept-data-loss (validated at startup)
    log "INITIATING DO PURGE at Stage ${stage} (wrangler delete --force)"
    log "WARNING: ALL Durable Object state will be permanently destroyed."
    rule
    npx wrangler@latest delete --env "${WORKER_ENV}" --force
    log "DO purge complete. Phase E SEALED in FAILED state. File incident report."
    exit 1
  else
    log "ROLLBACK: Stage ${stage} — wrangler rollback --env ${WORKER_ENV}"
    npx wrangler@latest rollback --env "${WORKER_ENV}"
    log "Rollback complete. Verify:"
    npx wrangler@latest deployments list --env "${WORKER_ENV}" 2>/dev/null | head -5 || true
    log "Phase E FAILED at Stage ${stage}. Document in specs/_audits/ before retrying."
    exit 1
  fi
}

# ── STAGE 1 — 5% ──────────────────────────────────────────────────────────

rule
log "STAGE 1 — Deploying at 5% canary"
rule

log "Running: npx wrangler@latest deploy --env ${WORKER_ENV} --percent 5"
npx wrangler@latest deploy \
  --env "${WORKER_ENV}" \
  --percent 5 \
  --message "Wave 32 Phase E — canary stage 1 (5%)"

log "Stage 1 deploy complete. Starting ${OBSERVATION_SECONDS}s (10-min) observation window."
print_monitoring_reminder "1" "5"

log "Sleeping ${OBSERVATION_SECONDS}s — DO NOT skip this wait..."
sleep "${OBSERVATION_SECONDS}"
log "Observation window complete."

if ! owner_ack "1 (5%)" "Stage 2 (25%)"; then
  do_rollback "1 (5%)"
fi

# ── STAGE 2 — 25% ─────────────────────────────────────────────────────────

rule
log "STAGE 2 — Deploying at 25% canary"
rule

log "Running: npx wrangler@latest deploy --env ${WORKER_ENV} --percent 25"
npx wrangler@latest deploy \
  --env "${WORKER_ENV}" \
  --percent 25 \
  --message "Wave 32 Phase E — canary stage 2 (25%)"

log "Stage 2 deploy complete. Starting ${OBSERVATION_SECONDS}s (10-min) observation window."
print_monitoring_reminder "2" "25"

log "Sleeping ${OBSERVATION_SECONDS}s — DO NOT skip this wait..."
sleep "${OBSERVATION_SECONDS}"
log "Observation window complete."

if ! owner_ack "2 (25%)" "Stage 3 (100% full promote)"; then
  do_rollback "2 (25%)"
fi

# ── STAGE 3 — 100% ────────────────────────────────────────────────────────

rule
log "STAGE 3 — Full promote at 100%"
rule

log "Running: npx wrangler@latest deploy --env ${WORKER_ENV}"
npx wrangler@latest deploy \
  --env "${WORKER_ENV}" \
  --message "Wave 32 Phase E — full promote (100%)"

log "Stage 3 deploy complete. Starting ${OBSERVATION_SECONDS}s (10-min) observation window."
print_monitoring_reminder "3" "100"

log "Sleeping ${OBSERVATION_SECONDS}s — DO NOT skip this wait..."
sleep "${OBSERVATION_SECONDS}"
log "Observation window complete."

# At 100%, ack drives Phase E SEAL sign-off
echo ""
read -r -p "  Stage 3 (100%) looks clean? Sign off Phase E SEAL? (y/n): " final_response
echo ""
case "$final_response" in
  [Yy]|[Yy][Ee][Ss])
    : # proceed to success path
    ;;
  *)
    log "Owner declined sign-off at 100%. Initiating rollback."
    if $FORCE_DO_PURGE && $ACCEPT_DATA_LOSS; then
      do_rollback "3 (100%)" "true"
    else
      do_rollback "3 (100%)"
    fi
    ;;
esac

# ── Success ────────────────────────────────────────────────────────────────

rule
log "Phase E COMPLETE — all 3 stages promoted and signed off."
rule
cat <<EOF

  Next steps:
    1. Commit Phase E SEAL audit: specs/_audits/2026-05-27-w32-phaseE-canary-prep-seal.md
    2. Notify Phase F owner (DNS cutover) — gate is OPEN.
    3. Notify Phase G owner (smoke test) — ready to execute post-DNS.
    4. Grafana + BetterStack screenshots attached to SEAL doc.

  Companion runbook: specs/_runbooks/RB-W32-CONTAINER-CANARY.md

EOF
exit 0
