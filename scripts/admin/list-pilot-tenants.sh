#!/usr/bin/env bash
# list-pilot-tenants.sh — Operator-facing CLI to list pilot tenants by state.
#
# Wave-27 pilot-signup-pipeline deliverable #2.
#
# Lists pilot tenants whose `pilot_state` matches the provided filter.
# States are tracked on the D1 `tenants` row:
#
#   NEW        — signup token redeemed; tier not yet granted (operator action pending)
#   ACTIVE     — pilot tier granted; tenant within first 90 days of pilot window
#   GRADUATED  — pilot succeeded; transitioned to a paid plan (subscription Active)
#   TERMINATED — pilot offboarded (RB-PILOT-ONBOARDING-E2E §5 fully applied)
#
# This is a PLACEHOLDER implementation: it prints the canonical SQL the
# production binary will issue. When apps/server/src/admin/tier_list.rs
# lands (T-7d), this script will be replaced with a real HTTPS call.
#
# Usage:
#   ./scripts/admin/list-pilot-tenants.sh --state ACTIVE
#   ./scripts/admin/list-pilot-tenants.sh --state NEW --format json
#
# Exit codes:
#   0  success
#   1  invalid arguments
#
# Charter compliance:
#   - DCO sign-off: Signed-off-by: Gustavo Schneiter <gustavo@humangr.com>
#
# Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>

set -euo pipefail

SCRIPT_NAME="$(basename "$0")"
STATE=""
FORMAT="table"

usage() {
  cat <<USAGE
${SCRIPT_NAME} — list pilot tenants filtered by state

USAGE:
  ${SCRIPT_NAME} --state <STATE> [--format {table,json}]

ARGUMENTS:
  --state <STATE>   Pilot lifecycle state. One of:
                      NEW         — signup token redeemed, tier not yet granted
                      ACTIVE      — pilot tier granted, within 90-day pilot window
                      GRADUATED   — pilot converted to paid plan
                      TERMINATED  — pilot offboarded
  --format <fmt>    Output format. Default 'table'. Options: table, json.

EXAMPLES:
  ${SCRIPT_NAME} --state NEW
  ${SCRIPT_NAME} --state ACTIVE --format json
  ${SCRIPT_NAME} --state GRADUATED

See: specs/_audits/sealed/2026-05-16-pilot-signup-pipeline.md §2 (operator pre-flight)
     docs/internal/customer-success-playbook.md §1 (pilot lifecycle)
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --state)  STATE="${2:-}";  shift 2 ;;
    --format) FORMAT="${2:-}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "ERROR: unknown argument '$1'" >&2; usage; exit 1 ;;
  esac
done

if [[ -z "${STATE}" ]]; then
  echo "ERROR: --state is required" >&2
  usage
  exit 1
fi

case "${STATE}" in
  NEW|ACTIVE|GRADUATED|TERMINATED) ;;
  *) echo "ERROR: --state '${STATE}' invalid; accepted {NEW, ACTIVE, GRADUATED, TERMINATED}" >&2
     exit 1 ;;
esac

case "${FORMAT}" in
  table|json) ;;
  *) echo "ERROR: --format '${FORMAT}' invalid; accepted {table, json}" >&2
     exit 1 ;;
esac

echo "[INFO] list-pilot-tenants filter: pilot_state = '${STATE}'"

echo "[PLACEHOLDER] D1 admin-plane list-binding not yet wired."
echo "[PLACEHOLDER] Production wiring target: apps/server/src/admin/tier_list.rs (T-7d)."
echo
echo "[PLACEHOLDER] Canonical SQL (operator copies for now):"
cat <<SQL
  SELECT tenant_id,
         slug,
         tier,
         cap_bytes,
         pilot_state,
         signup_at_ms,
         tier_granted_at_ms,
         first_blob_at_ms
    FROM tenants
   WHERE pilot_state = '${STATE}'
ORDER BY signup_at_ms ASC;
SQL

echo
if [[ "${FORMAT}" == "json" ]]; then
  echo "[PLACEHOLDER] Production binary will emit JSON array of TenantSummary rows."
  echo "  Schema: see apps/server/src/admin/tier_list.rs::TenantSummary."
  echo
  echo "  Example shape:"
  echo '  ['
  echo '    {'
  echo '      "tenant_id": "4a2c0a7e-1b9f-4d3a-9c0e-7e1f2b3a4c5d",'
  echo '      "slug": "acme-builds",'
  echo "      \"tier\": \"pilot\","
  echo '      "cap_bytes": 100000000000,'
  echo "      \"pilot_state\": \"${STATE}\","
  echo '      "signup_at_ms": 1747353600000,'
  echo '      "tier_granted_at_ms": 1747440000000,'
  echo '      "first_blob_at_ms": 1747526400000'
  echo '    }'
  echo '  ]'
else
  echo "[PLACEHOLDER] Production binary will emit table:"
  printf '  %-38s %-20s %-7s %-12s %-10s %-15s\n' \
    TENANT_ID SLUG TIER CAP_BYTES STATE SIGNUP_AT
  printf '  %-38s %-20s %-7s %-12s %-10s %-15s\n' \
    "(no rows — wire to D1 first)" "—" "—" "—" "—" "—"
fi
echo
echo "[NEXT] If state = NEW, operator runs grant-pilot-tier.sh for each row:"
echo "  ./scripts/admin/grant-pilot-tier.sh --tenant-id <id> --tier pilot --cap 100GB"
echo
echo "[DONE] list-pilot-tenants exit 0"
