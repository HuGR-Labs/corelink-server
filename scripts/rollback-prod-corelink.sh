#!/usr/bin/env bash
# rollback-prod-corelink.sh — Wave 32 Phase H PREP
#
# Fast rollback of CoreLink production deploy.
# Reverts Worker+DO (npx wrangler@latest rollback), Container (npx wrangler@latest containers rollback),
# and DNS records (from Phase G snapshot) to their pre-cutover state.
#
# Usage:
#   bash scripts/rollback-prod-corelink.sh              # Interactive (requires typing 'ROLLBACK')
#   bash scripts/rollback-prod-corelink.sh --auto       # Non-interactive (auto-rollback path)
#   bash scripts/rollback-prod-corelink.sh --dry-run    # Print what would happen; no mutations
#   bash scripts/rollback-prod-corelink.sh --dns-only   # Rollback DNS only
#   bash scripts/rollback-prod-corelink.sh --help
#
# Charter compliance:
#   - Bash strict mode: set -euo pipefail
#   - Confirmation prompt ('ROLLBACK') before any destructive step (interactive mode)
#   - --auto mode skips the prompt (called by cutover-checklist-prod.sh on smoke failure)
#   - CTRL-CRED-001: No credentials in this script
#   - CTRL-AUDIT-EMIT-BEFORE-MUTATION: each step logged before wrangler call
#
# Exit codes:
#   0 — rollback complete (all steps succeeded)
#   1 — rollback failed or aborted by operator
#   2 — partial rollback (some steps failed; manual action required)
#   3 — usage error

set -euo pipefail

# ──────────────────────────────────────────────
# Constants
# ──────────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
CONFIRM_WORD="ROLLBACK"

# DNS snapshot: Phase G produces a rollback snapshot at apply time.
# Default snapshot path (Phase G APPLY writes this file):
DNS_SNAPSHOT_DEFAULT="/tmp/dns-rollback-latest.json"

# Worker env name for wrangler calls
WORKER_ENV="prod"

# ──────────────────────────────────────────────
# State
# ──────────────────────────────────────────────
AUTO=false
DRY_RUN=false
DNS_ONLY=false
DNS_SNAPSHOT=""
STEP_PASS=0
STEP_FAIL=0
ROLLBACK_LOG="${REPO_ROOT}/specs/_audits/rollback-log-$(date -u +"%Y-%m-%dT%H%M%SZ").md"

usage() {
  cat <<EOF
rollback-prod-corelink.sh — Wave 32 Phase H fast rollback

Usage:
  bash scripts/rollback-prod-corelink.sh [OPTIONS]

Options:
  --auto              Non-interactive mode (used by cutover-checklist-prod.sh on smoke failure)
  --dry-run           Print rollback plan; no mutations
  --dns-only          Rollback DNS only (skip Worker/Container rollback)
  --dns-snapshot FILE Path to Phase G DNS rollback snapshot JSON
                      (default: /tmp/dns-rollback-latest.json)
  --help              Show this help

Rollback steps:
  1. Worker rollback:    npx wrangler@latest rollback --env prod
  2. Container rollback: npx wrangler@latest containers rollback corelink-server:prod
  3. DNS rollback:       scripts/dns-prod-apply.sh --rollback <snapshot>
  4. Smoke verify:       bash scripts/smoke-prod-corelink.sh (against pre-cutover baseline)

Decision tree:
  - If Worker rollback fails: HALT, manual action required (see §4 rollback decision tree)
  - If Container rollback fails: WARN, Worker is rolled back; DNS rollback still proceeds
  - If DNS rollback fails: ERROR, document manually which records need deletion
  - If post-rollback smoke fails: WARN, system is in degraded state, escalate to Owner

Exit codes:
  0 — all rollback steps succeeded
  1 — operator aborted (interactive mode only)
  2 — rollback partially failed (check FAIL steps in output)
  3 — usage error

EOF
  exit 0
}

# ──────────────────────────────────────────────
# Argument parsing
# ──────────────────────────────────────────────
while [[ $# -gt 0 ]]; do
  case "$1" in
    --auto)         AUTO=true ;;
    --dry-run)      DRY_RUN=true ;;
    --dns-only)     DNS_ONLY=true ;;
    --dns-snapshot) DNS_SNAPSHOT="$2"; shift ;;
    --help|-h)      usage ;;
    *) echo "Unknown option: $1" >&2; exit 3 ;;
  esac
  shift
done

[[ -z "$DNS_SNAPSHOT" ]] && DNS_SNAPSHOT="$DNS_SNAPSHOT_DEFAULT"

# ──────────────────────────────────────────────
# Dry-run mode (early exit — no wrangler prereq needed)
# ──────────────────────────────────────────────
if $DRY_RUN; then
  cat <<EOF

rollback-prod-corelink.sh — DRY-RUN
Date: $(date -u +"%Y-%m-%dT%H:%M:%SZ")

Rollback plan:

Step 1: Worker + DO rollback
  Command: npx wrangler@latest rollback --env ${WORKER_ENV}
  Effect:  Reverts corelink-prod Worker and CoreLinkServer DO to previous deployment.
  Note:    DO storage state survives rollback. Only code rolls back.
  Risk:    None for data. Workers.dev subdomain returns to previous Worker version.

Step 2: Container rollback
  Command: npx wrangler@latest containers rollback corelink-server:prod
  Effect:  Reverts the CF Container image to the previous pushed version.
  Note:    Container must be re-started by DO after Worker rollback.

Step 3: DNS rollback (from Phase G snapshot)
  Snapshot: ${DNS_SNAPSHOT}
  Command:  bash scripts/dns-prod-apply.sh --rollback ${DNS_SNAPSHOT}
  Effect:   Deletes all 9 newly-created CNAME records from Phase G APPLY.
            status.corelink.humangr.com (Phase A) is NOT touched.
  Note:     Only runs if snapshot file exists.

Step 4: Post-rollback smoke verification
  Command: bash scripts/smoke-prod-corelink.sh
  Expected: Worker on workers.dev subdomain returns 200 (pre-DNS cutover baseline).
            DNS checks may fail if DNS was not yet applied — that is OK.

Decision tree:
  - Worker rollback fails     → HALT + manual action (npx wrangler@latest deployments promote <id>)
  - Container rollback fails  → WARN + continue to DNS rollback (Worker already safe)
  - DNS rollback fails        → ERR + manual deletion of 9 CNAME records
  - Post-rollback smoke fails → WARN + escalate to Owner

Exit code (live run): 0 = complete, 2 = partial, 1 = aborted
EOF
  exit 0
fi

# ──────────────────────────────────────────────
# Helpers
# ──────────────────────────────────────────────
log()     { printf '[%s] %s\n' "$(date -u +"%H:%M:%SZ")" "$*"; }
step()    { printf '\n── Step %s ──\n' "$*"; }
ok()      { printf '[OK]   %s\n' "$*"; STEP_PASS=$(( STEP_PASS + 1 )); }
err()     { printf '[ERR]  %s\n' "$*" >&2; STEP_FAIL=$(( STEP_FAIL + 1 )); }
warn()    { printf '[WARN] %s\n' "$*"; }
emit_log(){ echo "$*" >> "$ROLLBACK_LOG"; }

# ──────────────────────────────────────────────
# Initialize rollback log
# ──────────────────────────────────────────────
mkdir -p "$(dirname "$ROLLBACK_LOG")"
cat > "$ROLLBACK_LOG" <<HEADER
# Wave 32 Phase H — Rollback Log

**Initiated:** ${TIMESTAMP}
**Mode:** $($AUTO && echo "AUTO" || echo "INTERACTIVE")
**Operator:** $(whoami)@$(hostname)
**Script:** scripts/rollback-prod-corelink.sh

---

HEADER

# ──────────────────────────────────────────────
# Prerequisite checks
# ──────────────────────────────────────────────
log "rollback-prod-corelink.sh — ${TIMESTAMP}"
emit_log "## Prerequisite checks"

if ! command -v npx &>/dev/null; then
  err "wrangler CLI not found in PATH"
  emit_log "- [ERR] wrangler not found"
  echo "Install: npm install -g wrangler" >&2
  exit 1
fi
WRANGLER_VER=$(npx wrangler@latest --version 2>/dev/null | head -1 || echo "unknown")
log "wrangler: ${WRANGLER_VER}"
emit_log "- wrangler: ${WRANGLER_VER}"

# ──────────────────────────────────────────────
# Confirmation prompt (interactive mode only)
# ──────────────────────────────────────────────
if ! $AUTO; then
  echo ""
  echo "═══════════════════════════════════════════════════════"
  echo "  CoreLink Production ROLLBACK"
  echo "  Wave 32 Phase H"
  echo "  Time: ${TIMESTAMP}"
  echo "═══════════════════════════════════════════════════════"
  echo ""
  echo "  This will rollback:"
  echo "    - Worker + DO (npx wrangler@latest rollback)"
  echo "    - Container (npx wrangler@latest containers rollback)"
  if [[ -f "$DNS_SNAPSHOT" ]]; then
    echo "    - DNS records (from snapshot: ${DNS_SNAPSHOT})"
  else
    echo "    - DNS records: snapshot NOT found at ${DNS_SNAPSHOT}"
    echo "      DNS rollback will be SKIPPED (manual action required)"
  fi
  echo ""
  echo "  Type '${CONFIRM_WORD}' to proceed. Ctrl-C to abort."
  echo ""
  printf '  Confirmation: '
  read -r OPERATOR_WORD
  if [[ "$OPERATOR_WORD" != "$CONFIRM_WORD" ]]; then
    echo "  Incorrect confirmation word. Rollback ABORTED." >&2
    emit_log "- [ABORTED] Incorrect confirmation word"
    exit 1
  fi
  emit_log "- [CONFIRMED] Operator typed '${CONFIRM_WORD}'"
  log "Rollback confirmed by operator."
else
  log "Auto-rollback mode — no confirmation required"
  emit_log "- [AUTO] Auto-rollback invoked (post-cutover smoke failure)"
fi

# ──────────────────────────────────────────────
# Step 1: Worker + DO rollback
# ──────────────────────────────────────────────
if ! $DNS_ONLY; then
  step "1/4: Worker + DO rollback"
  emit_log ""
  emit_log "## Step 1: Worker + DO rollback"
  emit_log "**Command:** \`npx wrangler@latest rollback --env ${WORKER_ENV}\`"
  emit_log "**Time:** $(date -u +"%Y-%m-%dT%H:%M:%SZ")"
  log "CTRL-AUDIT-EMIT: about to invoke npx wrangler@latest rollback --env ${WORKER_ENV}"

  if npx wrangler@latest rollback --env "${WORKER_ENV}" 2>&1 | tee -a "$ROLLBACK_LOG"; then
    ok "Worker + DO rollback complete"
    emit_log "**Result:** OK"
  else
    err "Worker + DO rollback FAILED"
    emit_log "**Result:** FAILED"
    echo ""
    echo "  CRITICAL: Worker rollback failed. Manual action required:" >&2
    echo "    1. Check: npx wrangler@latest deployments list --env ${WORKER_ENV}" >&2
    echo "    2. Identify previous stable deployment ID" >&2
    echo "    3. Manually: npx wrangler@latest deployments promote <ID> --env ${WORKER_ENV}" >&2
    echo ""
    # Worker rollback failure is critical — halt full rollback
    emit_log ""
    emit_log "**HALT:** Worker rollback failed. Remaining steps skipped. Manual action required."
    exit 2
  fi

  # Step 2: Container rollback
  step "2/4: Container rollback"
  emit_log ""
  emit_log "## Step 2: Container rollback"
  emit_log "**Command:** \`npx wrangler@latest containers rollback corelink-server:prod\`"
  emit_log "**Time:** $(date -u +"%Y-%m-%dT%H:%M:%SZ")"
  log "CTRL-AUDIT-EMIT: about to invoke npx wrangler@latest containers rollback corelink-server:prod"

  if npx wrangler@latest containers rollback corelink-server:prod 2>&1 | tee -a "$ROLLBACK_LOG"; then
    ok "Container rollback complete"
    emit_log "**Result:** OK"
  else
    warn "Container rollback FAILED — Worker is rolled back; DNS rollback will still proceed"
    emit_log "**Result:** FAILED (non-fatal — Worker already rolled back)"
    echo ""
    echo "  WARNING: Container rollback failed. Worker is already rolled back." >&2
    echo "  Manual action: npx wrangler@latest containers rollback corelink-server:prod" >&2
    echo "  Continuing with DNS rollback..." >&2
    echo ""
    STEP_FAIL=$(( STEP_FAIL + 1 ))
  fi
fi

# ──────────────────────────────────────────────
# Step 3: DNS rollback
# ──────────────────────────────────────────────
step "3/4: DNS rollback (Phase G snapshot)"
emit_log ""
emit_log "## Step 3: DNS rollback"
emit_log "**Snapshot:** ${DNS_SNAPSHOT}"
emit_log "**Time:** $(date -u +"%Y-%m-%dT%H:%M:%SZ")"

DNS_APPLY="${SCRIPT_DIR}/dns-prod-apply.sh"
if [[ ! -f "$DNS_APPLY" ]]; then
  err "DNS rollback script not found: ${DNS_APPLY}"
  emit_log "**Result:** FAILED — dns-prod-apply.sh not found"
  echo "  Manual DNS rollback required:" >&2
  echo "  Delete these 9 CNAME records via CF Dashboard or API:" >&2
  echo "    corelink-api.humangr.com" >&2
  echo "    corelink-app.humangr.com" >&2
  echo "    corelink-docs.humangr.com" >&2
  echo "    corelink-signup.humangr.com" >&2
  echo "    humangr.com" >&2
  echo "    acme-dev.corelink.humangr.com" >&2
  echo "    staging.corelink.humangr.com" >&2
  echo "    sandbox.corelink.humangr.com" >&2
  echo "    go.corelink.humangr.com" >&2
  STEP_FAIL=$(( STEP_FAIL + 1 ))
elif [[ ! -f "$DNS_SNAPSHOT" ]]; then
  warn "DNS rollback snapshot not found at ${DNS_SNAPSHOT}"
  emit_log "**Result:** SKIPPED — snapshot not found (expected: ${DNS_SNAPSHOT})"
  echo "  DNS rollback skipped — no snapshot file." >&2
  echo "  Manual DNS rollback required:" >&2
  echo "  Delete the 9 CNAME records created by Phase G APPLY." >&2
  echo "  Use: bash scripts/dns-prod-apply.sh --rollback <snapshot-file>" >&2
  STEP_FAIL=$(( STEP_FAIL + 1 ))
else
  log "CTRL-AUDIT-EMIT: about to invoke dns-prod-apply.sh --rollback ${DNS_SNAPSHOT}"
  if bash "${DNS_APPLY}" --rollback "${DNS_SNAPSHOT}" 2>&1 | tee -a "$ROLLBACK_LOG"; then
    ok "DNS rollback complete (snapshot: ${DNS_SNAPSHOT})"
    emit_log "**Result:** OK"
  else
    err "DNS rollback FAILED"
    emit_log "**Result:** FAILED"
    echo "  Manual DNS rollback required. Check CF Dashboard." >&2
    STEP_FAIL=$(( STEP_FAIL + 1 ))
  fi
fi

# ──────────────────────────────────────────────
# Step 4: Post-rollback smoke verification
# ──────────────────────────────────────────────
step "4/4: Post-rollback smoke verification"
emit_log ""
emit_log "## Step 4: Post-rollback smoke"
emit_log "**Time:** $(date -u +"%Y-%m-%dT%H:%M:%SZ")"
log "Running post-rollback smoke (workers.dev baseline)..."

SMOKE_SCRIPT="${SCRIPT_DIR}/smoke-prod-corelink.sh"
if [[ ! -f "$SMOKE_SCRIPT" ]]; then
  warn "smoke-prod-corelink.sh not found — skipping post-rollback smoke"
  emit_log "**Result:** SKIPPED — smoke script not found"
else
  # Post-rollback: the workers.dev subdomain should still respond.
  # DNS may not be active yet (if Phase G APPLY hadn't run).
  # We run smoke and treat DNS failures as non-fatal in this context.
  SMOKE_RESULT=0
  bash "${SMOKE_SCRIPT}" 2>&1 | tee -a "$ROLLBACK_LOG" || SMOKE_RESULT=$?
  if [[ "$SMOKE_RESULT" -eq 0 ]]; then
    ok "Post-rollback smoke: all checks passed"
    emit_log "**Result:** OK (exit 0)"
  else
    warn "Post-rollback smoke: ${SMOKE_RESULT} failure(s). System may be in degraded state."
    warn "Escalate to Owner if Worker /health check fails."
    emit_log "**Result:** DEGRADED — ${SMOKE_RESULT} smoke failures"
    STEP_FAIL=$(( STEP_FAIL + 1 ))
  fi
fi

# ──────────────────────────────────────────────
# Summary
# ──────────────────────────────────────────────
ROLLBACK_END=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
printf '\n══════════════════════════════════════════════\n'
printf 'ROLLBACK SUMMARY  %s\n' "$ROLLBACK_END"
printf '  Steps passed:  %d\n' "$STEP_PASS"
printf '  Steps failed:  %d\n' "$STEP_FAIL"
if [[ "$STEP_FAIL" -eq 0 ]]; then
  printf '  Result:        ROLLBACK COMPLETE\n'
else
  printf '  Result:        PARTIAL ROLLBACK (%d failure(s))\n' "$STEP_FAIL"
  printf '  Action:        Review [ERR]/[WARN] lines above + escalate to Owner\n'
fi
printf '  Log:           %s\n' "$ROLLBACK_LOG"
printf '══════════════════════════════════════════════\n'

cat >> "$ROLLBACK_LOG" <<FOOTER

---

## Summary

| Property | Value |
|---|---|
| Started | ${TIMESTAMP} |
| Completed | ${ROLLBACK_END} |
| Steps passed | ${STEP_PASS} |
| Steps failed | ${STEP_FAIL} |
| Mode | $($AUTO && echo "AUTO" || echo "INTERACTIVE") |
| Operator | $(whoami)@$(hostname) |

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>

---
*End of Wave 32 Phase H rollback log.*
FOOTER

log "Rollback log written: ${ROLLBACK_LOG}"

if [[ "$STEP_FAIL" -gt 0 ]]; then
  exit 2
fi
exit 0
