#!/usr/bin/env bash
# pre-merge-gate-check.sh <PR-number>
# =============================================================================
# MANDATORY pre-merge gate. Run this BEFORE every `gh pr merge`.
#
#   bash scripts/pre-merge-gate-check.sh <PR>
#
# Exits 0 ONLY when every non-skipping PR check is green. Exits 1 (and prints
# the offenders) if anything is failing or still pending — in which case DO NOT
# MERGE: fix it, or re-run the check, until green.
#
# Why this exists: the heavy gates (coverage, CodeQL, the TLA+ model-checks,
# ffi-matrix, reproducible-build, …) were moved OFF per-PR to keep PRs fast
# (they run nightly + on main + on-demand). So the checks that REMAIN on a PR
# are exactly the fast, load-bearing ones — and they must all be green before
# merge. This guards against blind `--admin` merges that skip them.
# =============================================================================
set -euo pipefail

PR="${1:?usage: bash scripts/pre-merge-gate-check.sh <PR-number>}"

json="$(gh pr checks "$PR" --json name,bucket,link 2>/dev/null || true)"
if [ -z "$json" ] || [ "$json" = "[]" ]; then
  echo "  (PR #$PR has no checks reported — nothing to gate.)"
  exit 0
fi

GATE_JSON="$json" python3 - "$PR" <<'PY'
import os, sys, json
pr = sys.argv[1]
data = json.loads(os.environ["GATE_JSON"])
order = {"fail": 0, "pending": 1, "skipping": 2, "pass": 3}
mark = {"pass": "✓", "fail": "✗", "pending": "…", "skipping": "-"}
fails, pends = [], []
for c in sorted(data, key=lambda x: order.get(x.get("bucket"), 9)):
    b = c.get("bucket")
    print(f"  {mark.get(b,'?')} {b:9} {c.get('name')}")
    if b == "fail":
        fails.append(c)
    elif b == "pending":
        pends.append(c)
print()
if fails or pends:
    print(f"  ⛔ DO NOT MERGE PR #{pr} — {len(fails)} failing, {len(pends)} pending.")
    for c in fails:
        print(f"     ✗ {c.get('name')}  {c.get('link','')}")
    print("     Fix or re-run until green. Use --admin ONLY for a documented,")
    print("     non-blocking infra/flake reason you state explicitly.")
    sys.exit(1)
print(f"  ✅ All gates green — OK to merge PR #{pr}.")
PY
