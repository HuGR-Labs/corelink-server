#!/usr/bin/env bash
# pilot-24h-checkin.sh — Auto check-in poller for newly-granted pilot tenants.
#
# Wave-27 pilot-signup-pipeline deliverable #3.
#
# Queries a pilot tenant's activation signal 24h after tier-grant. If the
# tenant has not yet committed a single blob (`first_blob_at_ms IS NULL`)
# 24h after `tier_granted_at_ms`, the script emits a Slack alert to the
# customer-success rotation so a human can reach out before the pilot
# loses momentum.
#
# Customer-success playbook §3.3 calls this the "no-blob silent-fail"
# anti-pattern: the single strongest leading indicator of pilot churn is
# a tenant whose first 24 hours pass without ANY upload activity.
#
# This is a PLACEHOLDER implementation: it prints the canonical SQL
# query + the canonical Slack webhook payload. Production wiring target
# is apps/server/src/admin/pilot_checkin.rs (T-7d) plus a cron schedule
# in .github/workflows/pilot-checkin-cron.yml (also T-7d).
#
# Usage:
#   ./scripts/admin/pilot-24h-checkin.sh --tenant-id <uuid>
#   ./scripts/admin/pilot-24h-checkin.sh --all-active        # check every ACTIVE pilot
#   ./scripts/admin/pilot-24h-checkin.sh --tenant-id <uuid> --dry-run
#
# Environment variables:
#   PILOT_CHECKIN_SLACK_WEBHOOK   — Slack incoming-webhook URL (set on operator host;
#                                   placeholder mode prints body only if unset)
#
# Exit codes:
#   0  success (tenant healthy, or alert emitted)
#   1  invalid arguments
#   2  tenant has no `tier_granted_at_ms` (not yet granted; nothing to check)
#
# Charter compliance:
#   - DCO sign-off: Signed-off-by: Gustavo Schneiter <gustavo@humangr.com>
#   - synchronous bash only (no background polling loop).
#
# Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>

set -euo pipefail

SCRIPT_NAME="$(basename "$0")"
TENANT_ID=""
ALL_ACTIVE=0
DRY_RUN=0

usage() {
  cat <<USAGE
${SCRIPT_NAME} — 24h pilot tenant activation check-in

USAGE:
  ${SCRIPT_NAME} --tenant-id <uuid> [--dry-run]
  ${SCRIPT_NAME} --all-active        [--dry-run]

ARGUMENTS:
  --tenant-id <uuid>   Single tenant UUID to check.
  --all-active         Check every tenant with pilot_state = 'ACTIVE'.
  --dry-run            Print what would be alerted; do not POST to Slack.

ALERT RULE:
  Emit Slack alert to #pilot-onboarding (channel) when:
    pilot_state = 'ACTIVE'
    AND tier_granted_at_ms IS NOT NULL
    AND (now_ms - tier_granted_at_ms) >= 86_400_000  (24h)
    AND first_blob_at_ms IS NULL

EXAMPLES:
  ${SCRIPT_NAME} --tenant-id 4a2c0a7e-1b9f-4d3a-9c0e-7e1f2b3a4c5d
  ${SCRIPT_NAME} --all-active --dry-run

See: specs/_audits/sealed/2026-05-16-pilot-signup-pipeline.md §3 (24h auto-checkin)
     docs/internal/customer-success-playbook.md §3.3 (no-blob silent-fail)
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --tenant-id)   TENANT_ID="${2:-}"; shift 2 ;;
    --all-active)  ALL_ACTIVE=1;       shift   ;;
    --dry-run)     DRY_RUN=1;          shift   ;;
    -h|--help)     usage; exit 0 ;;
    *) echo "ERROR: unknown argument '$1'" >&2; usage; exit 1 ;;
  esac
done

if [[ -z "${TENANT_ID}" && "${ALL_ACTIVE}" -eq 0 ]]; then
  echo "ERROR: provide --tenant-id <uuid> or --all-active" >&2
  usage
  exit 1
fi

if [[ -n "${TENANT_ID}" && "${ALL_ACTIVE}" -eq 1 ]]; then
  echo "ERROR: --tenant-id and --all-active are mutually exclusive" >&2
  exit 1
fi

if [[ -n "${TENANT_ID}" ]]; then
  if ! [[ "${TENANT_ID}" =~ ^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$ ]]; then
    echo "ERROR: --tenant-id '${TENANT_ID}' is not a lowercase hyphenated UUID" >&2
    exit 1
  fi
fi

# Prefer Python for portable ms-epoch (GNU date supports %3N, BSD/macOS does not).
NOW_MS="$(python3 -c 'import time; print(int(time.time()*1000))')"
WINDOW_MS=86400000  # 24h
SLACK_URL="${PILOT_CHECKIN_SLACK_WEBHOOK:-}"

echo "[INFO] pilot-24h-checkin"
echo "  now_ms:        ${NOW_MS}"
echo "  window_ms:     ${WINDOW_MS} (24h)"
if [[ -n "${TENANT_ID}" ]]; then
  echo "  scope:         single tenant ${TENANT_ID}"
else
  echo "  scope:         all ACTIVE pilots"
fi

if [[ -z "${SLACK_URL}" ]]; then
  echo "  slack_webhook: <unset; placeholder payload only>"
else
  # Redact webhook in log (last 8 chars).
  echo "  slack_webhook: <set; suffix=...${SLACK_URL: -8}>"
fi

echo "[PLACEHOLDER] D1 admin-plane query binding not yet wired."
echo "[PLACEHOLDER] Production wiring target: apps/server/src/admin/pilot_checkin.rs (T-7d)."
echo
echo "[PLACEHOLDER] Canonical SQL (operator copies for now):"
if [[ -n "${TENANT_ID}" ]]; then
  cat <<SQL
  SELECT tenant_id, slug, tier_granted_at_ms, first_blob_at_ms
    FROM tenants
   WHERE tenant_id = '${TENANT_ID}'
     AND pilot_state = 'ACTIVE'
     AND tier_granted_at_ms IS NOT NULL
     AND (${NOW_MS} - tier_granted_at_ms) >= ${WINDOW_MS}
     AND first_blob_at_ms IS NULL;
SQL
else
  cat <<SQL
  SELECT tenant_id, slug, tier_granted_at_ms, first_blob_at_ms
    FROM tenants
   WHERE pilot_state = 'ACTIVE'
     AND tier_granted_at_ms IS NOT NULL
     AND (${NOW_MS} - tier_granted_at_ms) >= ${WINDOW_MS}
     AND first_blob_at_ms IS NULL;
SQL
fi

echo
echo "[PLACEHOLDER] Canonical Slack alert payload (one per matching row):"
cat <<'PAYLOAD'
  {
    "channel": "#pilot-onboarding",
    "username": "pilot-24h-checkin",
    "icon_emoji": ":warning:",
    "text": "Pilot tenant <SLUG> (<TENANT_ID>) has not uploaded a single blob 24h after tier grant. Reach out before momentum is lost. Playbook: docs/internal/customer-success-playbook.md §3.3."
  }
PAYLOAD

if [[ "${DRY_RUN}" -eq 1 ]]; then
  echo "[DRY-RUN] No Slack POST issued. Exit 0."
  exit 0
fi

if [[ -z "${SLACK_URL}" ]]; then
  echo "[INFO] PILOT_CHECKIN_SLACK_WEBHOOK unset; skipping POST (placeholder mode)."
  echo "[DONE] pilot-24h-checkin exit 0"
  exit 0
fi

echo "[INFO] curl POST to Slack webhook is NOT yet wired in placeholder mode."
echo "[INFO] Once apps/server/src/admin/pilot_checkin.rs lands, this script delegates."
echo "[DONE] pilot-24h-checkin exit 0"
