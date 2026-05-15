#!/bin/bash
# check_proptest_density_gate.sh — CI gate enforcing proptest density.
#
# A crate FAILS the gate when:
#   - it has >= 1 INV reference AND ratio < 1.0
#   - AND it is NOT listed in scripts/proptest-density-allowlist.txt
#
# The allowlist documents pre-existing gaps (with closing WI IDs) so the
# gate does not flap on baseline regressions while follow-up WIs land.
#
# Exit codes:
#   0  → gate passed
#   1  → gate failed (new or regressed gap below 1.0 ratio)
#   2  → gate misconfiguration (script can't read inputs)
#
# Source audit: specs/_audits/2026-05-15-proptest-density.md
# Followup:    specs/_audits/proptest-followup-tickets.md (WI-PROPTEST-FU-005)

set -euo pipefail
LC_NUMERIC=C
export LC_NUMERIC

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/.." && pwd)
cd "$REPO_ROOT" || exit 2

AUDIT_SCRIPT="$SCRIPT_DIR/audit_proptest_density.sh"
ALLOWLIST="$SCRIPT_DIR/proptest-density-allowlist.txt"

if [ ! -x "$AUDIT_SCRIPT" ] && [ ! -r "$AUDIT_SCRIPT" ]; then
  echo "ERROR: audit script not readable at $AUDIT_SCRIPT" >&2
  exit 2
fi
if [ ! -r "$ALLOWLIST" ]; then
  echo "ERROR: allowlist not readable at $ALLOWLIST" >&2
  exit 2
fi

# Build allowed-crate set from allowlist (strip comments + inline comments).
ALLOWED=$(grep -vE '^[[:space:]]*(#|$)' "$ALLOWLIST" | awk '{print $1}' | sort -u)

REPORT=$(bash "$AUDIT_SCRIPT")

echo "$REPORT"
echo "---"

VIOLATIONS=0
while IFS='|' read -r crate prop_macros prop_tests inv_refs ratio; do
  # Skip header.
  [ "$crate" = "crate" ] && continue
  # Skip crates with no INV references.
  [ "$ratio" = "N/A" ] && continue
  # Numeric compare.
  if awk -v r="$ratio" 'BEGIN { exit !(r+0 < 1.0) }'; then
    if echo "$ALLOWED" | grep -qx "$crate"; then
      echo "ALLOWED gap: $crate (ratio=$ratio, inv_refs=$inv_refs) — see proptest-density-allowlist.txt"
    else
      echo "FAIL: $crate ratio=$ratio inv_refs=$inv_refs prop_tests=$prop_tests (below 1.0; not in allowlist)" >&2
      VIOLATIONS=$((VIOLATIONS + 1))
    fi
  fi
done <<< "$REPORT"

if [ "$VIOLATIONS" -gt 0 ]; then
  echo "" >&2
  echo "proptest density gate: $VIOLATIONS crate(s) below 1.0 ratio AND not allowlisted." >&2
  echo "Fix: add proptests in that crate's tests/ covering its INV-* references." >&2
  echo "     If intentional, add the crate to scripts/proptest-density-allowlist.txt" >&2
  echo "     with a closing follow-up WI ID." >&2
  exit 1
fi

echo "proptest density gate: PASS (no regressions outside the allowlist)."
exit 0
