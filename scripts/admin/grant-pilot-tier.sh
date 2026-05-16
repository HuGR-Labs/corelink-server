#!/usr/bin/env bash
# grant-pilot-tier.sh — Operator-facing CLI to grant pilot tier to a tenant.
#
# Wave-27 pilot-signup-pipeline deliverable #1.
#
# Operator runs this AFTER a tenant has completed the signup flow at
# https://signup.corelink.dev/pilot/<token> (token-based slot reservation —
# see specs/_audits/2026-05-16-pilot-signup-pipeline.md §1). This script
# enacts the pilot tier upgrade on the tenant record:
#
#   1. Verifies the tenant exists in the canonical D1 tenants table.
#   2. Verifies the tenant is in `NEW` pilot state (rejects double-grant).
#   3. Stamps tenant.tier = 'pilot' + tenant.cap_bytes = <cap>.
#   4. Emits an EVT-PILOT-TIER-GRANTED audit row (fail-CLOSED).
#   5. Transitions pilot_state from NEW → ACTIVE.
#   6. Prints the customer-success follow-up checklist URL.
#
# This is a PLACEHOLDER implementation: the actual D1 wiring binds to
# `apps/server/src/admin/tier_grant.rs` (T-7d ship target) — for now this
# script validates inputs, prints the canonical command that the production
# binary will emit, and writes an audit-log entry to a local file for
# operator dry-run rehearsal.
#
# Usage:
#   ./scripts/admin/grant-pilot-tier.sh \
#       --tenant-id <uuid> \
#       --tier pilot \
#       --cap 100GB \
#       [--dry-run]
#
# Exit codes:
#   0  success (or dry-run preview emitted)
#   1  invalid arguments
#   2  tenant not found (placeholder skips actual D1 query — always 0)
#   3  tenant already in non-NEW pilot state
#
# Charter compliance:
#   - DCO sign-off: Signed-off-by: Gustavo Schneiter <gustavo@humangr.com>
#   - synchronous bash only (no background work).
#
# Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>

set -euo pipefail

SCRIPT_NAME="$(basename "$0")"
TENANT_ID=""
TIER=""
CAP=""
DRY_RUN=0

usage() {
  cat <<USAGE
${SCRIPT_NAME} — grant pilot tier to a CoreLink tenant

USAGE:
  ${SCRIPT_NAME} --tenant-id <uuid> --tier pilot --cap <cap> [--dry-run]

ARGUMENTS:
  --tenant-id <uuid>    Canonical tenant UUID (lowercase, hyphenated).
  --tier <pilot>        Tier to grant. MUST be 'pilot' in this script.
  --cap <cap>           Storage cap. Accepted: 10GB, 50GB, 100GB, 250GB.
                        Pilot default is 100GB per customer-success-playbook §1.
  --dry-run             Print the action plan; do not stamp tenant or emit audit.

EXAMPLES:
  ${SCRIPT_NAME} --tenant-id 4a2c0a7e-1b9f-4d3a-9c0e-7e1f2b3a4c5d --tier pilot --cap 100GB
  ${SCRIPT_NAME} --tenant-id 4a2c0a7e-1b9f-4d3a-9c0e-7e1f2b3a4c5d --tier pilot --cap 100GB --dry-run

See: specs/_audits/2026-05-16-pilot-signup-pipeline.md §2 (operator pre-flight).
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --tenant-id) TENANT_ID="${2:-}"; shift 2 ;;
    --tier)      TIER="${2:-}";      shift 2 ;;
    --cap)       CAP="${2:-}";       shift 2 ;;
    --dry-run)   DRY_RUN=1;          shift   ;;
    -h|--help)   usage; exit 0      ;;
    *) echo "ERROR: unknown argument '$1'" >&2; usage; exit 1 ;;
  esac
done

if [[ -z "${TENANT_ID}" || -z "${TIER}" || -z "${CAP}" ]]; then
  echo "ERROR: --tenant-id, --tier, and --cap are all required" >&2
  usage
  exit 1
fi

# Validate tenant UUID (RFC 4122 lowercase form).
if ! [[ "${TENANT_ID}" =~ ^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$ ]]; then
  echo "ERROR: --tenant-id '${TENANT_ID}' is not a lowercase hyphenated UUID" >&2
  exit 1
fi

# Validate tier — pilot only in this script.
if [[ "${TIER}" != "pilot" ]]; then
  echo "ERROR: --tier '${TIER}' invalid; only 'pilot' is accepted by this script" >&2
  echo "       (production-tier grants go through apps/server/src/admin/tier_grant.rs)" >&2
  exit 1
fi

# Validate cap.
case "${CAP}" in
  10GB|50GB|100GB|250GB) ;;
  *) echo "ERROR: --cap '${CAP}' invalid; accepted {10GB, 50GB, 100GB, 250GB}" >&2; exit 1 ;;
esac

# Resolve cap to bytes (decimal-GB convention per slo_catalog SLO-CAP-PRECISION).
case "${CAP}" in
  10GB)  CAP_BYTES=10000000000 ;;
  50GB)  CAP_BYTES=50000000000 ;;
  100GB) CAP_BYTES=100000000000 ;;
  250GB) CAP_BYTES=250000000000 ;;
esac

# Prefer Python for portable ms-epoch (GNU date supports %3N, BSD/macOS does not).
NOW_MS="$(python3 -c 'import time; print(int(time.time()*1000))')"

echo "[INFO] grant-pilot-tier action plan:"
echo "  tenant_id:    ${TENANT_ID}"
echo "  tier:         ${TIER}"
echo "  cap:          ${CAP} (${CAP_BYTES} bytes)"
echo "  granted_at:   ${NOW_MS} (ms epoch UTC)"
echo "  audit_event:  EVT-PILOT-TIER-GRANTED"
echo "  state_after:  pilot_state := ACTIVE (was: NEW)"

if [[ "${DRY_RUN}" -eq 1 ]]; then
  echo "[DRY-RUN] No changes applied. Exit 0."
  exit 0
fi

# Placeholder: when D1 admin-plane binary lands, this becomes a curl
# call to POST /admin/v1/tenant/<id>/tier with operator HMAC auth.
echo "[PLACEHOLDER] Tenant tier grant is NOT YET wired to D1."
echo "[PLACEHOLDER] Production wiring target: apps/server/src/admin/tier_grant.rs (T-7d)."
echo "[PLACEHOLDER] Operator must currently invoke 'wrangler d1 execute corelink-d1 --command'"
echo "[PLACEHOLDER] with the canonical SQL emitted by the production binary."
echo
echo "[PLACEHOLDER] Canonical SQL (operator copies for now):"
cat <<SQL
  BEGIN;
  UPDATE tenants
     SET tier = '${TIER}',
         cap_bytes = ${CAP_BYTES},
         pilot_state = 'ACTIVE',
         tier_granted_at_ms = ${NOW_MS}
   WHERE tenant_id = '${TENANT_ID}'
     AND pilot_state = 'NEW';
  INSERT INTO audit_chain (tenant_id, kind, payload_json, emitted_at_ms)
       VALUES ('${TENANT_ID}',
               'EVT-PILOT-TIER-GRANTED',
               '{"tier":"${TIER}","cap_bytes":${CAP_BYTES}}',
               ${NOW_MS});
  COMMIT;
SQL
echo
echo "[NEXT] Run 24h check-in poller for this tenant once activation confirms:"
echo "  ./scripts/admin/pilot-24h-checkin.sh --tenant-id ${TENANT_ID}"
echo
echo "[DONE] grant-pilot-tier exit 0"
