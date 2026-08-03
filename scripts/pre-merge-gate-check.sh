#!/usr/bin/env bash
# pre-merge-gate-check.sh <PR-number>
# =============================================================================
# MANDATORY pre-merge gate. Run this BEFORE every `gh pr merge`.
#
#   bash scripts/pre-merge-gate-check.sh <PR>
#
# Exits 0 ONLY when the PR is mergeable AND its real gates ran AND every
# non-skipping check is green. Exits 1 (and prints why) otherwise — in which
# case DO NOT MERGE.
#
# Why this exists: the heavy gates (coverage, CodeQL, the TLA+ model-checks,
# ffi-matrix, reproducible-build, …) were moved OFF per-PR to keep PRs fast
# (they run nightly + on main + on-demand). So the checks that REMAIN on a PR
# are exactly the fast, load-bearing ones — and they must all be green before
# merge. This guards against blind `--admin` merges that skip them.
#
# ⚠️ Branch protection has `required checks = []` (CLAUDE.md), so THIS SCRIPT is
# the last line of defense. It must never fail OPEN.
#
# ── The 2026-08-02 hole this closes ──────────────────────────────────────────
# It printed "✅ All gates green — OK to merge PR #967" for a PR touching two
# Workers on which dco, changelog-validate, gitleaks, trivy, secrets-matrix and
# every vitest job had NEVER RUN. Six checks existed; all six were
# `pull_request_target` metadata jobs (labels, size bucket, welcome comment,
# dependabot sentinel). Zero failures out of zero real gates read as green.
#
# Mechanism: `pull_request` workflows run on the MERGE REF. When a PR conflicts,
# GitHub cannot build that ref, so it creates NO runs at all — not queued, not
# failed, ABSENT. `pull_request_target` workflows run on the BASE branch, so
# those still fire. That asymmetry is the fingerprint, and it means the MOST
# DANGEROUS PR STATE PRODUCED THE MOST REASSURING OUTPUT.
#
# It is not a rare state here: every `feat:`/`fix:` commit needs a CHANGELOG
# `[Unreleased]` entry and everyone inserts at the top of the same section, so
# two concurrent PRs conflict by construction, and `main` moves under a branch
# within minutes.
#
# Three defenses below, in order of how badly the old script needed them:
#   1. mergeable must be MERGEABLE (not CONFLICTING/UNKNOWN)
#   2. the always-present gates must actually appear in the check list
#   3. "no checks at all" is a FAILURE, not "nothing to gate"
# =============================================================================
set -euo pipefail

PR="${1:?usage: bash scripts/pre-merge-gate-check.sh <PR-number>}"

# ── Defense 1: mergeability ───────────────────────────────────────────────────
# GitHub computes `mergeable` asynchronously, so UNKNOWN means "ask again", not
# "fine". Both non-MERGEABLE states are refused: on a conflict the check list is
# actively misleading (see the header), and on UNKNOWN we cannot yet tell.
state="$(gh pr view "$PR" --json mergeable,mergeStateStatus,state \
  -q '"\(.mergeable) \(.mergeStateStatus) \(.state)"' 2>/dev/null || echo "ERROR ERROR ERROR")"
mergeable="${state%% *}"
rest="${state#* }"
mergestatus="${rest%% *}"
prstate="${rest#* }"

if [ "$prstate" != "OPEN" ]; then
  echo "  ⛔ DO NOT MERGE PR #$PR — the PR is $prstate, not OPEN."
  exit 1
fi

if [ "$mergeable" != "MERGEABLE" ]; then
  echo "  ⛔ DO NOT MERGE PR #$PR — mergeable=$mergeable ($mergestatus)."
  echo
  if [ "$mergeable" = "CONFLICTING" ]; then
    echo "     A CONFLICTING PR gets NO \`pull_request\` checks at all: those run on"
    echo "     the merge ref, which GitHub cannot build. Any green you see below is"
    echo "     \`pull_request_target\` metadata jobs only — it proves NOTHING."
    echo
    echo "     Fix:  git rebase origin/main && git push --force-with-lease"
    echo "     Then re-run this script and wait for the real checks to appear."
  else
    echo "     GitHub has not finished computing mergeability. Re-run in a moment;"
    echo "     do NOT read this as a green light."
  fi
  exit 1
fi

# ── Defense 3 (ordering: checked before the per-check loop) ───────────────────
# "No checks reported" used to `exit 0` with "nothing to gate". On a repo where
# every PR runs dco + gitleaks unconditionally, no checks means the workflows did
# not fire — which is a reason to STOP, not to merge.
json="$(gh pr checks "$PR" --json name,bucket,link 2>/dev/null || true)"
if [ -z "$json" ] || [ "$json" = "[]" ]; then
  echo "  ⛔ DO NOT MERGE PR #$PR — the PR reports NO checks at all."
  echo "     That is not 'nothing to gate': every PR here runs dco + gitleaks"
  echo "     unconditionally, so zero checks means the workflows never fired."
  exit 1
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

# ── Defense 2: the real gates must be PRESENT, not merely not-failing ─────────
# Chosen because both are triggered by a bare `on: pull_request` with NO paths
# filter, so they run for every PR regardless of what it touches. If either is
# missing, the `pull_request` workflows did not fire and the rest of this list
# is metadata jobs that gate nothing. Substring match keeps this robust against
# job-name edits ("dco-check" → "dco", "gitleaks (secret-leak scan)" →
# "gitleaks detect").
#
# Keep this list SMALL and path-filter-free. Adding a path-filtered gate here
# would make the script fail on PRs that legitimately skip it — turning a
# fail-open into a fail-noisy, which gets the whole check disabled by the next
# person in a hurry.
REQUIRED_PRESENT = ["dco", "gitleaks"]
names = " ".join(c.get("name", "").lower() for c in data)
missing = [g for g in REQUIRED_PRESENT if g not in names]
if missing:
    print(f"  ⛔ DO NOT MERGE PR #{pr} — the always-present gates never ran: "
          f"{', '.join(missing)}.")
    print("     Every PR triggers these unconditionally (bare `on: pull_request`,")
    print("     no paths filter), so their ABSENCE means the pull_request")
    print("     workflows did not fire for this head sha. The checks listed above")
    print("     are pull_request_target metadata jobs; they gate nothing.")
    print("     Usual cause: the PR conflicts, or the head sha was force-pushed")
    print("     while runs were being created. Rebase, push, and re-run.")
    sys.exit(1)

if fails or pends:
    print(f"  ⛔ DO NOT MERGE PR #{pr} — {len(fails)} failing, {len(pends)} pending.")
    for c in fails:
        print(f"     ✗ {c.get('name')}  {c.get('link','')}")
    print("     Fix or re-run until green. Use --admin ONLY for a documented,")
    print("     non-blocking infra/flake reason you state explicitly.")
    sys.exit(1)
print(f"  ✅ All gates green — OK to merge PR #{pr}.")
PY
