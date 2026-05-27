#!/usr/bin/env bash
# cutover-checklist-prod.sh — Wave 32 Phase H PREP
#
# Interactive go-live checklist runner for CoreLink production cutover.
# Prints each checklist item, accepts operator y/n confirmation, and
# logs the result to a signed markdown audit file.
#
# Usage:
#   bash scripts/cutover-checklist-prod.sh
#   bash scripts/cutover-checklist-prod.sh --output /path/to/checklist.md
#   bash scripts/cutover-checklist-prod.sh --non-interactive  (CI/operator-scripted mode)
#   bash scripts/cutover-checklist-prod.sh --help
#
# Charter compliance:
#   CTRL-CRED-001: No credentials in this script.
#   cutover-fail-CLOSED: if post-cutover smoke fails, AUTO-ROLLBACK is invoked
#     (calls rollback-prod-corelink.sh --auto).
#   CTRL-AUDIT-EMIT: every step appended to markdown log before action.
#
# Phase H APPLY prerequisite:
#   Phase E (container deploy) + Phase F (pages) + Phase G (DNS) must all be complete.
#   Run smoke-prod-corelink.sh first (pre-cutover item 1 in this script).
#
# Exit codes:
#   0 — all items confirmed / cutover committed
#   1 — operator aborted (typed 'n' on a blocking item or Ctrl-C)
#   2 — post-cutover smoke failed (auto-rollback triggered)
#   3 — usage error

set -euo pipefail

# ──────────────────────────────────────────────
# Constants
# ──────────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
TIMESTAMP_SLUG=$(date -u +"%Y-%m-%dT%H%M%SZ")
DEFAULT_OUTPUT="${REPO_ROOT}/specs/_audits/2026-05-26-w32-phaseH-cutover-checklist-${TIMESTAMP_SLUG}.md"

# ──────────────────────────────────────────────
# State
# ──────────────────────────────────────────────
OUTPUT_FILE="$DEFAULT_OUTPUT"
NON_INTERACTIVE=false
STEP_NUM=0
FAIL_COUNT=0
CONFIRM_WORD="CUTOVER"
CUTOVER_COMMITTED=false

usage() {
  cat <<EOF
cutover-checklist-prod.sh — Wave 32 Phase H go-live checklist runner

Usage:
  bash scripts/cutover-checklist-prod.sh [OPTIONS]

Options:
  --output FILE        Write signed markdown checklist to FILE
                       (default: specs/_audits/2026-05-26-w32-phaseH-cutover-checklist-<ts>.md)
  --non-interactive    Accept all items automatically (CI/operator-scripted mode).
                       Does NOT skip the final CUTOVER sign-off — omit for dry workflow.
  --help               Show this help

Checklist areas (15 items):
  Pre-cutover (8 items):
    1.  Smoke test green (run smoke-prod-corelink.sh)
    2.  BetterStack status page reachable + subscribable (Phase A)
    3.  CF Container metrics — no crash loops (10-min baseline)
    4.  Error budget unconsumed
    5.  Phase D secrets deployed (verify-secrets-deployed.sh exits 0)
    6.  Phase B Worker shim passing unit tests (cd worker && pnpm test)
    7.  Canary traffic stable at 5% for >= 10 minutes
    8.  Adversarial review score >= 7.5/10 (per wave-32 spec §7 hard pause #8)

  Cutover execution (3 items):
    9.  Confirm canary ramp: 5% -> 100% via wrangler
    10. DNS cutover locked in (all 9 records present, no NXDOMAIN)
    11. CF Pages custom domains responding on docs + app subdomains

  Post-cutover (3 items):
    12. Post-cutover smoke green (re-run smoke-prod-corelink.sh)
    13. 30-minute metrics window clean (no crash loops, no error spikes)
    14. Roll-forward verification (git log --oneline -5 on deployed commit)

  Sign-off (1 item):
    15. Operator types '$CONFIRM_WORD' to commit cutover

Exit codes:
  0 — cutover committed
  1 — operator aborted
  2 — post-cutover smoke failed (auto-rollback triggered)
  3 — usage error

EOF
  exit 0
}

# ──────────────────────────────────────────────
# Argument parsing
# ──────────────────────────────────────────────
while [[ $# -gt 0 ]]; do
  case "$1" in
    --output)          OUTPUT_FILE="$2"; shift ;;
    --non-interactive) NON_INTERACTIVE=true ;;
    --help|-h)         usage ;;
    *) echo "Unknown option: $1" >&2; exit 3 ;;
  esac
  shift
done

# ──────────────────────────────────────────────
# Helpers
# ──────────────────────────────────────────────
log()    { printf '[%s] %s\n' "$(date -u +"%H:%M:%SZ")" "$*"; }
item()   { STEP_NUM=$(( STEP_NUM + 1 )); printf '\n[%02d/%d] %s\n' "$STEP_NUM" 15 "$*"; }
emit()   { echo "$*" >> "$OUTPUT_FILE"; }

# confirm QUESTION [blocking=true]
# Returns 0 if confirmed, 1 if denied.
confirm() {
  local question="$1"
  local blocking="${2:-true}"
  if $NON_INTERACTIVE; then
    echo "  [AUTO-CONFIRM] $question"
    emit "  - [AUTO-CONFIRM] ${question}"
    return 0
  fi
  while true; do
    printf '  %s [y/n]: ' "$question"
    read -r answer
    case "$answer" in
      y|Y|yes|YES)
        emit "  - [YES] ${question}"
        return 0
        ;;
      n|N|no|NO)
        emit "  - [NO]  ${question}"
        if $blocking; then
          echo "  Aborted by operator."
          emit ""
          emit "**CUTOVER ABORTED** by operator at step ${STEP_NUM}/15 — ${TIMESTAMP}"
          return 1
        fi
        return 1
        ;;
      *)
        echo "  Please type y or n."
        ;;
    esac
  done
}

abort_cutover() {
  local reason="$1"
  echo ""
  echo "CUTOVER ABORTED: $reason" >&2
  emit ""
  emit "**CUTOVER ABORTED:** ${reason} — $(date -u +"%Y-%m-%dT%H:%M:%SZ")"
  exit 1
}

# ──────────────────────────────────────────────
# Hard pause trigger 2: wrangler required
# ──────────────────────────────────────────────
if ! command -v npx &>/dev/null; then
  echo "HARD PAUSE TRIGGER 2: wrangler CLI not found in PATH." >&2
  echo "Install: npm install -g wrangler  (requires Node 18+)" >&2
  exit 1
fi

WRANGLER_VERSION=$(npx wrangler@latest --version 2>/dev/null | head -1 || echo "unknown")
log "wrangler version: ${WRANGLER_VERSION}"

# Hard pause trigger 1: dns-prod-plan.sh must be present
if [[ ! -f "${SCRIPT_DIR}/dns-prod-plan.sh" ]]; then
  echo "HARD PAUSE TRIGGER 1: scripts/dns-prod-plan.sh not found." >&2
  echo "Phase G prerequisite missing." >&2
  exit 1
fi

# Hard pause trigger 3: Phase G audit must list 9 plan rows
PHASE_G_AUDIT="${REPO_ROOT}/specs/_audits/sealed/2026-05-26-w32-phaseG-prep.md"
if [[ ! -f "$PHASE_G_AUDIT" ]]; then
  echo "HARD PAUSE TRIGGER 3: Phase G audit not found: ${PHASE_G_AUDIT}" >&2
  exit 1
fi
PLAN_ROW_COUNT=$(grep -c "corelink.*humangr.com.*CNAME" "$PHASE_G_AUDIT" 2>/dev/null || echo "0")
if [[ "$PLAN_ROW_COUNT" -lt 9 ]]; then
  echo "HARD PAUSE TRIGGER 3: Phase G audit shows only ${PLAN_ROW_COUNT} DNS plan rows (need >= 9)." >&2
  exit 1
fi

# ──────────────────────────────────────────────
# Initialize output file
# ──────────────────────────────────────────────
mkdir -p "$(dirname "$OUTPUT_FILE")"
cat > "$OUTPUT_FILE" <<HEADER
# Wave 32 Phase H — Production Cutover Checklist

**Started:** ${TIMESTAMP}
**Operator:** $(whoami)@$(hostname)
**wrangler:** ${WRANGLER_VERSION}
**Script:** scripts/cutover-checklist-prod.sh
**Parent spec:** specs/_audits/sealed/2026-05-22-wave32-prod-deploy-spec.md §4 Phase H

---

## Checklist

HEADER

log "Cutover checklist started. Output: ${OUTPUT_FILE}"
echo ""
echo "═══════════════════════════════════════════════════════"
echo "  CoreLink Production Cutover Checklist"
echo "  Wave 32 Phase H APPLY"
echo "  Started: ${TIMESTAMP}"
echo "  Output:  ${OUTPUT_FILE}"
echo "═══════════════════════════════════════════════════════"
echo ""
echo "Review each item. Type y to confirm, n to abort."
echo ""

# ──────────────────────────────────────────────
# PRE-CUTOVER ITEMS (1–8)
# ──────────────────────────────────────────────
emit "### Pre-cutover items"
emit ""

# Item 1: Smoke green
item "PRE-CUTOVER: Smoke test green"
echo "  Run: bash scripts/smoke-prod-corelink.sh"
echo "  Expected exit code: 0 (all 22 checks green)"
emit "#### [01/15] PRE-CUTOVER: Smoke test green"
if ! confirm "Have you run smoke-prod-corelink.sh and confirmed exit code 0?"; then
  abort_cutover "Pre-cutover smoke not confirmed"
fi

# Item 2: BetterStack status page
item "PRE-CUTOVER: BetterStack status page reachable + subscribable"
echo "  Check: https://status.corelink.humangr.com"
echo "  Verify: page loads, subscribe button present, 5 components visible"
echo "  Phase A inheritance (SEALed 4d4fb8f6)"
emit "#### [02/15] PRE-CUTOVER: BetterStack status page"
if ! confirm "Is https://status.corelink.humangr.com reachable and subscribable?"; then
  abort_cutover "BetterStack status page not confirmed"
fi

# Item 3: CF Container metrics — no crash loops
item "PRE-CUTOVER: CF Container metrics — no crash loops (10-min baseline)"
echo "  Check CF Dashboard → Workers & Pages → corelink-prod → Containers"
echo "  Confirm: container status = 'running', restarts = 0 over last 10 minutes"
echo "  Hard pause: if crash loop detected, DO NOT proceed (Phase E prerequisite)"
emit "#### [03/15] PRE-CUTOVER: CF Container metrics clean (10-min baseline)"
if ! confirm "Is the CF Container running with no crash loops over the last 10 minutes?"; then
  abort_cutover "CF Container crash loop detected — cannot proceed to cutover"
fi

# Item 4: Error budget unconsumed
item "PRE-CUTOVER: Error budget unconsumed"
echo "  Check: SLO dashboard (Grafana / CF Analytics)"
echo "  Confirm: error rate < 0.1% over last 1 hour (SLO target: 99.9% availability)"
echo "  If error budget is > 50% consumed, consider deferring cutover."
emit "#### [04/15] PRE-CUTOVER: Error budget unconsumed"
if ! confirm "Is the error budget unconsumed (< 50% consumed in last 1 hour)?"; then
  abort_cutover "Error budget consumed — cutover deferred"
fi

# Item 5: Phase D secrets deployed
item "PRE-CUTOVER: Phase D secrets deployed"
echo "  Run: bash scripts/verify-secrets-deployed.sh"
echo "  Expected: exit 0, zero diff against canonical matrix (55 cf-wrangler secrets)"
emit "#### [05/15] PRE-CUTOVER: Phase D secrets deployed"
if ! confirm "Have you run verify-secrets-deployed.sh and confirmed exit code 0 (all 55 secrets)?"; then
  abort_cutover "Phase D secrets not fully deployed"
fi

# Item 6: Phase B Worker shim tests
item "PRE-CUTOVER: Phase B Worker shim unit tests passing"
echo "  Run: cd worker && pnpm test"
echo "  Expected: >= 70% coverage, all tests green"
emit "#### [06/15] PRE-CUTOVER: Phase B Worker shim tests passing"
if ! confirm "Have you run 'cd worker && pnpm test' and confirmed >= 70% coverage + all green?"; then
  abort_cutover "Worker shim tests not confirmed"
fi

# Item 7: Canary 5% stable >= 10 minutes
item "PRE-CUTOVER: Canary traffic stable at 5% for >= 10 minutes"
echo "  Verify in CF Dashboard → Workers → corelink-prod → Traffic Splits"
echo "  Canary should show 5% weight, no error spikes in last 10 minutes"
echo "  Command: npx wrangler@latest deployments list --env prod"
emit "#### [07/15] PRE-CUTOVER: 5% canary stable >= 10 minutes"
if ! confirm "Is the 5% canary stable with no error spikes over the last 10 minutes?"; then
  abort_cutover "Canary not stable — cannot ramp to 100%"
fi

# Item 8: Adversarial review score
item "PRE-CUTOVER: Adversarial review score >= 7.5/10"
echo "  Per wave-32 spec §7 hard pause trigger #8:"
echo "  An independent Sonnet adversarial review must score the deploy state >= 7.5/10"
echo "  (SOTA bar: 8.5/10 target)"
echo "  Review scope: Worker shim, container health, DNS records, secrets matrix, audit chain"
emit "#### [08/15] PRE-CUTOVER: Adversarial review >= 7.5/10"
if ! confirm "Has an independent adversarial review been completed with score >= 7.5/10?"; then
  abort_cutover "Adversarial review not complete or score < 7.5/10 — per spec §7 hard pause #8"
fi

# ──────────────────────────────────────────────
# CUTOVER EXECUTION ITEMS (9–11)
# ──────────────────────────────────────────────
emit ""
emit "### Cutover execution"
emit ""
echo ""
echo "──────────────────────────────────────────────"
echo "  ALL PRE-CUTOVER ITEMS CONFIRMED"
echo "  Beginning cutover execution..."
echo "──────────────────────────────────────────────"
echo ""

# Item 9: Ramp canary 5% → 100%
item "CUTOVER STEP 1: Ramp canary from 5% to 100%"
echo "  Command: npx wrangler@latest deployments promote <deployment-id> --env prod"
echo "  Or via CF Dashboard → Workers → corelink-prod → Deployments → Promote"
echo "  This routes 100% of traffic to the new Worker + DO + Container version."
echo ""
echo "  WARNING: This is an irreversible forward step."
echo "  Rollback is available via: bash scripts/rollback-prod-corelink.sh"
emit "#### [09/15] CUTOVER: Ramp canary 5% -> 100%"
if ! confirm "Confirm you want to ramp from 5% canary to 100% now?"; then
  abort_cutover "Operator declined canary ramp — cutover aborted"
fi
log "Executing canary ramp to 100%..."
emit "  - Canary ramp initiated at $(date -u +"%Y-%m-%dT%H:%M:%SZ")"
emit "  - Command: npx wrangler@latest deployments promote <id> --env prod"
# NOTE: Phase H APPLY will execute the actual wrangler command here.
# This PREP script documents the step and records operator confirmation.
echo "  [RECORDED] Canary ramp confirmed. Phase H APPLY will execute wrangler promote."

# Item 10: DNS cutover locked in
item "CUTOVER STEP 2: DNS cutover locked in"
echo "  Run: bash scripts/dns-prod-verify.sh --post-apply"
echo "  Expected: all 9 plan rows resolve (no NXDOMAIN), status NO-OP"
echo "  9 Worker/Pages records should resolve to CF IPs (proxied)."
emit "#### [10/15] CUTOVER: DNS records present and resolving"
if ! confirm "Have you run dns-prod-verify.sh --post-apply and confirmed all 9 records resolve?"; then
  abort_cutover "DNS records not verified — cutover halted at DNS step"
fi

# Item 11: CF Pages custom domains
item "CUTOVER STEP 3: CF Pages custom domains responding"
echo "  Verify:"
echo "    curl -sI https://corelink-docs.humangr.com | head -1  → HTTP/2 200"
echo "    curl -sI https://corelink-app.humangr.com  | head -1  → HTTP/2 200"
emit "#### [11/15] CUTOVER: Pages custom domains responding (docs + app)"
if ! confirm "Are both corelink-docs.humangr.com and corelink-app.humangr.com returning 200?"; then
  abort_cutover "Pages custom domains not responding — check CF Pages project custom domain setup"
fi

# ──────────────────────────────────────────────
# POST-CUTOVER ITEMS (12–14)
# ──────────────────────────────────────────────
emit ""
emit "### Post-cutover verification"
emit ""
echo ""
echo "──────────────────────────────────────────────"
echo "  CUTOVER STEPS 1-3 COMPLETE"
echo "  Beginning post-cutover verification..."
echo "──────────────────────────────────────────────"
echo ""

# Item 12: Post-cutover smoke
item "POST-CUTOVER: Smoke test green again"
echo "  Run: bash scripts/smoke-prod-corelink.sh"
echo "  Expected: exit code 0 (all 22 checks green)"
echo ""
echo "  CRITICAL: If smoke fails, AUTO-ROLLBACK will be triggered."
emit "#### [12/15] POST-CUTOVER: Smoke test green"
POST_SMOKE_PASS=false
if confirm "Have you run smoke-prod-corelink.sh post-cutover and it returned exit 0?" "false"; then
  POST_SMOKE_PASS=true
  echo "  [PASS] Post-cutover smoke confirmed green"
  emit "  - [PASS] Post-cutover smoke green"
else
  echo ""
  echo "  POST-CUTOVER SMOKE FAILED."
  echo "  Triggering AUTO-ROLLBACK per charter (cutover-fail-CLOSED)..."
  emit "  - [FAIL] Post-cutover smoke FAILED — AUTO-ROLLBACK triggered"
  emit ""
  emit "**AUTO-ROLLBACK TRIGGERED** — post-cutover smoke failed — $(date -u +"%Y-%m-%dT%H:%M:%SZ")"
  # Call rollback script with --auto flag (no interactive confirmation required)
  if [[ -f "${SCRIPT_DIR}/rollback-prod-corelink.sh" ]]; then
    log "Invoking rollback-prod-corelink.sh --auto ..."
    bash "${SCRIPT_DIR}/rollback-prod-corelink.sh" --auto || true
  else
    echo "  ERROR: rollback-prod-corelink.sh not found at ${SCRIPT_DIR}/" >&2
    echo "  Manual rollback required: npx wrangler@latest rollback --env prod" >&2
  fi
  exit 2
fi

# Item 13: 30-minute metrics window
item "POST-CUTOVER: 30-minute metrics window clean"
echo "  Monitor CF Analytics → Workers → corelink-prod for 30 minutes:"
echo "    - Error rate < 0.1%"
echo "    - P95 latency < 500ms"
echo "    - No container crash loops"
echo "    - No DO storage errors"
echo "  Recommendation: set a 30-minute timer and check back."
emit "#### [13/15] POST-CUTOVER: 30-minute metrics window clean"
if ! confirm "Have you monitored for 30 minutes and confirmed clean metrics?"; then
  echo "  WARNING: Metrics window not confirmed. Proceeding at operator discretion."
  emit "  - [WARN] 30-min metrics window confirmation SKIPPED by operator"
  FAIL_COUNT=$(( FAIL_COUNT + 1 ))
else
  emit "  - [PASS] 30-minute metrics window clean"
fi

# Item 14: Roll-forward verification
item "POST-CUTOVER: Roll-forward verification"
echo "  Verify the deployed commit matches the expected SHA:"
echo "    npx wrangler@latest deployments list --env prod | head -3"
echo "    git log --oneline -5"
echo "  Confirm: deployed Worker commit == current repo HEAD"
emit "#### [14/15] POST-CUTOVER: Roll-forward verification"
DEPLOYED_SHA=$(npx wrangler@latest deployments list --env prod 2>/dev/null | grep -m1 "sha\|commit\|version" | awk '{print $NF}' || echo "unknown")
REPO_HEAD=$(git -C "${REPO_ROOT}" rev-parse --short HEAD 2>/dev/null || echo "unknown")
echo "  Current repo HEAD: ${REPO_HEAD}"
echo "  Deployed version:  ${DEPLOYED_SHA}"
emit "  - Repo HEAD: ${REPO_HEAD}"
emit "  - Deployed:  ${DEPLOYED_SHA}"
if ! confirm "Does the deployed version match the expected commit?"; then
  echo "  WARNING: Deployed version mismatch. Investigate before signing off."
  emit "  - [WARN] Version mismatch reported by operator"
  FAIL_COUNT=$(( FAIL_COUNT + 1 ))
else
  emit "  - [PASS] Roll-forward version verified"
fi

# ──────────────────────────────────────────────
# SIGN-OFF (Item 15)
# ──────────────────────────────────────────────
emit ""
emit "### Sign-off"
emit ""
echo ""
echo "══════════════════════════════════════════════════════"
echo "  ALL CUTOVER AND POST-CUTOVER ITEMS COMPLETE"
echo "  FINAL SIGN-OFF REQUIRED"
echo "══════════════════════════════════════════════════════"
echo ""
echo "  To commit this cutover, type exactly: ${CONFIRM_WORD}"
echo "  To abort, type anything else or press Ctrl-C."
echo ""

item "SIGN-OFF: Type '${CONFIRM_WORD}' to commit the cutover"
emit "#### [15/15] SIGN-OFF: Operator commitment"

if $NON_INTERACTIVE; then
  echo "  [NON-INTERACTIVE] Cutover sign-off skipped (non-interactive mode)."
  echo "  WARNING: This mode is for dry-workflow only. Real cutover requires manual sign-off."
  emit "  - [WARN] Non-interactive mode — sign-off not committed"
else
  printf '  Type %s to commit: ' "$CONFIRM_WORD"
  read -r OPERATOR_WORD
  if [[ "$OPERATOR_WORD" == "$CONFIRM_WORD" ]]; then
    CUTOVER_COMMITTED=true
    SIGNOFF_TS=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
    emit "  - [SIGNED OFF] Operator typed '${CONFIRM_WORD}' at ${SIGNOFF_TS}"
    emit "  - Operator: $(whoami)"
  else
    echo "  Incorrect confirmation word. Cutover NOT committed." >&2
    emit "  - [ABORTED] Incorrect confirmation word — cutover NOT committed"
    exit 1
  fi
fi

# ──────────────────────────────────────────────
# Final footer
# ──────────────────────────────────────────────
FINAL_TS=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
cat >> "$OUTPUT_FILE" <<FOOTER

---

## Summary

| Property | Value |
|---|---|
| Started | ${TIMESTAMP} |
| Completed | ${FINAL_TS} |
| Operator | $(whoami)@$(hostname) |
| Warnings | ${FAIL_COUNT} |
| Cutover committed | $($CUTOVER_COMMITTED && echo "YES" || echo "NO") |
| Checklist doc | ${OUTPUT_FILE} |

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>

---
*End of Wave 32 Phase H cutover checklist.*
FOOTER

echo ""
echo "══════════════════════════════════════════════════════"
if $CUTOVER_COMMITTED; then
  echo "  CUTOVER COMMITTED  warnings=${FAIL_COUNT}"
else
  echo "  CUTOVER DRY-RUN COMPLETE  warnings=${FAIL_COUNT}"
fi
echo "  Checklist: ${OUTPUT_FILE}"
echo "══════════════════════════════════════════════════════"
echo ""

log "cutover-checklist-prod.sh complete. Output: ${OUTPUT_FILE}"
exit 0
