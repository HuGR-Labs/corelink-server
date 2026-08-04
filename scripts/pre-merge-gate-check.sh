#!/usr/bin/env bash
# pre-merge-gate-check.sh [--merge [--dry-run] [--admin-reason "<why>"]] <PR-number>
# =============================================================================
# MANDATORY pre-merge gate. Run this BEFORE every `gh pr merge` — or, better,
# let it do the merge itself so there is no `&&` for a shell to disarm:
#
#   bash scripts/pre-merge-gate-check.sh <PR>              # gate + report only
#   bash scripts/pre-merge-gate-check.sh --merge <PR>      # gate, then merge IFF green
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
# ── The 2026-08-04 hole this closes (PR #1049) ───────────────────────────────
# The script was correct and it still failed to gate, because gating and merging
# were two commands joined by shell:
#
#   bash scripts/pre-merge-gate-check.sh 1049 | tail -3 && gh pr merge 1049 --squash
#
# `&&` binds to the PIPELINE, whose exit status is `tail`'s — always 0. The gate
# printed `⛔ DO NOT MERGE — 4 pending`, exited 1, and the merge ran anyway with
# four checks still in flight. They happened to pass; that was luck, not process.
# The same shape was used on #1043/#1045/#1046/#1047 (green, so no harm, but no
# enforcement either). A guard piped through `tail` is disarmed exactly as
# thoroughly as the bug the guard was written to catch.
#
# The fix is NOT "be more careful". `--merge` puts the gate and the merge in ONE
# process: the `gh pr merge` call is reachable only through a single `if` on the
# gate's own return code, so there is no exit status left for a pipeline to
# swallow. Piping `--merge`'s output changes nothing — the decision never leaves
# this process. When stdout is NOT a tty and `--merge` was not used, the script
# prints a warning on STDERR (which the pipe does not capture) naming the footgun.
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

usage() {
  cat <<'USAGE'
usage: bash scripts/pre-merge-gate-check.sh [flags] <PR-number>

  (no flags)              gate + report. Exit 0 = green, 1 = DO NOT MERGE.
  --merge                 gate, then `gh pr merge --squash --delete-branch`
                          IFF the gate went green. Nothing to chain with `&&`.
  --dry-run               with --merge: print the merge command, do not run it.
  --admin-reason "<why>"  with --merge: add `--admin`, and ONLY when the sole
                          reason the gate refused is a failed/cancelled check.
                          Never usable on pending, conflicting, or missing-gate
                          refusals. The reason is echoed into the output.
USAGE
}

MODE="gate"
DRY_RUN=0
ADMIN_REASON=""
PR=""

while [ $# -gt 0 ]; do
  case "$1" in
    --merge)          MODE="merge"; shift ;;
    --dry-run)        DRY_RUN=1; shift ;;
    --admin-reason)
      if [ $# -lt 2 ] || [ -z "${2-}" ]; then
        echo "  ⛔ --admin-reason needs a non-empty reason string." >&2
        exit 2
      fi
      ADMIN_REASON="$2"; shift 2 ;;
    --admin-reason=*)
      ADMIN_REASON="${1#*=}"
      if [ -z "$ADMIN_REASON" ]; then
        echo "  ⛔ --admin-reason needs a non-empty reason string." >&2
        exit 2
      fi
      shift ;;
    --admin)
      echo "  ⛔ bare --admin is not accepted. Use: --merge --admin-reason \"<documented infra/flake reason>\"" >&2
      exit 2 ;;
    -h|--help)        usage; exit 0 ;;
    --)               shift ;;
    -*)               echo "  ⛔ unknown flag: $1" >&2; usage >&2; exit 2 ;;
    *)
      if [ -n "$PR" ]; then echo "  ⛔ more than one PR number given: $PR and $1" >&2; exit 2; fi
      PR="$1"; shift ;;
  esac
done

case "$PR" in
  ""|*[!0-9]*) usage >&2; exit 1 ;;
esac

if [ -n "$ADMIN_REASON" ] && [ "$MODE" != "merge" ]; then
  echo "  ⛔ --admin-reason is only meaningful with --merge." >&2
  exit 2
fi

# ── The pipe footgun, named on stderr ─────────────────────────────────────────
# `[ -t 1 ]` is false exactly when the caller piped or redirected stdout, i.e.
# in the `… | tail -3 && gh pr merge …` shape that produced the #1049 incident.
# The warning goes to STDERR on purpose: the pipe does not capture it, so it
# reaches the human even when stdout is being truncated by `tail`. It is emitted
# from an EXIT trap so it survives `2>&1 | tail -N` too, and so that every exit
# path (including the early structural refusals) carries it.
VERDICT_FILE=""
on_exit() {
  local rc=$?
  if [ -n "$VERDICT_FILE" ]; then rm -f "$VERDICT_FILE"; fi
  if [ "$MODE" != "merge" ] && [ ! -t 1 ]; then
    echo "  ⚠️  stdout is not a terminal — if you are about to chain \`&& gh pr merge\`," >&2
    echo "      DON'T: a pipeline's exit status is the LAST command's (\`tail\` = 0), so" >&2
    echo "      this gate's verdict is discarded. This is how PR #1049 merged with 4" >&2
    echo "      checks pending. Use instead:" >&2
    echo "        bash scripts/pre-merge-gate-check.sh --merge $PR" >&2
  fi
  return "$rc"
}
trap on_exit EXIT

# `verdict` records WHY the gate refused, for --admin-reason eligibility only.
# STRUCTURAL  = not OPEN / not MERGEABLE / no checks / required gates absent /
#               anything pending. Never overridable: these mean the gates have
#               NOT RUN, which no amount of documented flake reason can fix.
# OVERRIDABLE = gates all present and finished, and the only non-green entries
#               are failed/cancelled checks — the documented-flake case.
verdict() {
  if [ -n "$VERDICT_FILE" ]; then printf '%s' "$1" >"$VERDICT_FILE"; fi
  return 0
}

# ── The gate itself ──────────────────────────────────────────────────────────
# Everything below is byte-for-byte the pre-existing gate, moved into a function
# so its return code can be tested by an `if` instead of being handed to a shell
# operator. It uses `return`, never `exit`: the caller decides what happens next.
run_gate() {
  # ── Defense 1: mergeability ─────────────────────────────────────────────────
  # GitHub computes `mergeable` asynchronously, so UNKNOWN means "ask again", not
  # "fine". Both non-MERGEABLE states are refused: on a conflict the check list is
  # actively misleading (see the header), and on UNKNOWN we cannot yet tell.
  local state mergeable rest mergestatus prstate json
  state="$(gh pr view "$PR" --json mergeable,mergeStateStatus,state \
    -q '"\(.mergeable) \(.mergeStateStatus) \(.state)"' 2>/dev/null || echo "ERROR ERROR ERROR")"
  mergeable="${state%% *}"
  rest="${state#* }"
  mergestatus="${rest%% *}"
  prstate="${rest#* }"

  if [ "$prstate" != "OPEN" ]; then
    echo "  ⛔ DO NOT MERGE PR #$PR — the PR is $prstate, not OPEN."
    verdict STRUCTURAL
    return 1
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
    verdict STRUCTURAL
    return 1
  fi

  # ── Defense 3 (ordering: checked before the per-check loop) ─────────────────
  # "No checks reported" used to `exit 0` with "nothing to gate". On a repo where
  # every PR runs dco + gitleaks unconditionally, no checks means the workflows did
  # not fire — which is a reason to STOP, not to merge.
  json="$(gh pr checks "$PR" --json name,bucket,link 2>/dev/null || true)"
  if [ -z "$json" ] || [ "$json" = "[]" ]; then
    echo "  ⛔ DO NOT MERGE PR #$PR — the PR reports NO checks at all."
    echo "     That is not 'nothing to gate': every PR here runs dco + gitleaks"
    echo "     unconditionally, so zero checks means the workflows never fired."
    verdict STRUCTURAL
    return 1
  fi

  GATE_JSON="$json" GATE_VERDICT_FILE="$VERDICT_FILE" python3 - "$PR" <<'PY'
import os, sys, json
pr = sys.argv[1]
data = json.loads(os.environ["GATE_JSON"])

def verdict(kind):
    """Side channel for --admin-reason eligibility. Never touches stdout, so the
    default (no --merge) output stays byte-identical to the pre-2026-08-04 script."""
    path = os.environ.get("GATE_VERDICT_FILE") or ""
    if path:
        with open(path, "w") as fh:
            fh.write(kind)

# Bucket handling is ALLOWLIST-based, not denylist-based, and that is the whole
# point. The previous version tested `b == "fail"` / `b == "pending"` and let
# every other bucket fall through as green — so a **cancelled** check printed as
# `? cancel  spec-validation` and the script still concluded
# "✅ All gates green". Observed on PR #982, 2026-08-03: a job the runner killed
# at 5m37s ("The operation was canceled", the shape an ENOSPC or an overloaded
# self-hosted mac takes) read as a pass. A cancelled gate has not run; it has
# proven nothing. Same failure shape this file already documents for zero-gates,
# one level down — the check list was present, one entry was simply uncountable.
#
# Anything that is not an explicit PASS or an explicit SKIP is now BLOCKING,
# including a bucket name GitHub has not invented yet. Unknown ⇒ blocking is the
# fail-CLOSED direction, which is the only acceptable one here.
PASS_BUCKETS = {"pass"}
SKIP_BUCKETS = {"skipping"}
order = {"fail": 0, "cancel": 0, "pending": 1, "skipping": 2, "pass": 3}
mark = {"pass": "✓", "fail": "✗", "cancel": "✗", "pending": "…", "skipping": "-"}
fails, pends = [], []
for c in sorted(data, key=lambda x: order.get(x.get("bucket"), 0)):
    b = c.get("bucket")
    print(f"  {mark.get(b,'✗')} {str(b):9} {c.get('name')}")
    if b in PASS_BUCKETS or b in SKIP_BUCKETS:
        continue
    if b == "pending":
        pends.append(c)
    else:
        # fail, cancel, or anything unrecognised.
        fails.append(c)
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
    verdict("STRUCTURAL")
    sys.exit(1)

if fails or pends:
    print(f"  ⛔ DO NOT MERGE PR #{pr} — {len(fails)} not-green, {len(pends)} pending.")
    for c in fails:
        b = c.get("bucket")
        why = "" if b == "fail" else f"  [bucket={b} — not a pass; it did not run to a verdict]"
        print(f"     ✗ {c.get('name')}  {c.get('link','')}{why}")
    print("     Fix or re-run until green. Use --admin ONLY for a documented,")
    print("     non-blocking infra/flake reason you state explicitly.")
    # A PENDING check has not produced a verdict yet, so no documented reason can
    # justify overriding it — that is precisely the #1049 state. Only an all-
    # finished list whose sole problem is fail/cancel is override-eligible.
    verdict("STRUCTURAL" if pends else "OVERRIDABLE")
    sys.exit(1)
print(f"  ✅ All gates green — OK to merge PR #{pr}.")
PY
}

if [ "$MODE" = "merge" ]; then
  VERDICT_FILE="$(mktemp "${TMPDIR:-/tmp}/premergegate.XXXXXX")"
fi

gate_rc=0
run_gate || gate_rc=$?

# Default mode: report and exit with the gate's own code. Unchanged since 2026-08-03.
if [ "$MODE" != "merge" ]; then
  exit "$gate_rc"
fi

# ── --merge: the ONLY branch that can reach `gh pr merge` ────────────────────
# There is exactly one call site below and it sits inside this `if`. A non-zero
# gate returns here; nothing downstream re-evaluates the verdict.
GATE_VERDICT="$(cat "$VERDICT_FILE" 2>/dev/null || true)"

if [ "$gate_rc" -ne 0 ]; then
  echo
  if [ -n "$ADMIN_REASON" ] && [ "$GATE_VERDICT" = "OVERRIDABLE" ]; then
    echo "  ⚠️  --admin OVERRIDE, reason stated by the caller:"
    echo "      \"$ADMIN_REASON\""
    echo "      (allowed only because every gate RAN and the sole non-green entries"
    echo "       are failed/cancelled checks — pending/conflicting is never overridable.)"
  else
    if [ -n "$ADMIN_REASON" ]; then
      echo "  ⛔ --admin-reason REFUSED: this refusal is $GATE_VERDICT, not OVERRIDABLE."
      echo "     Pending, conflicting, non-OPEN, or missing-gate states mean the gates"
      echo "     have NOT RUN. No documented reason substitutes for a verdict."
    fi
    echo "  ⛔ NO MERGE ISSUED for PR #$PR — the gate exited $gate_rc."
    exit "$gate_rc"
  fi
fi

MERGE_ARGS=(--squash --delete-branch)
# squash + delete-branch is house practice: every PR merged since #1013 landed as
# a single `… (#NNNN)` squash commit on main, and the repo has
# delete_branch_on_merge=false, so the branch must be deleted explicitly.
if [ -n "$ADMIN_REASON" ] && [ "$gate_rc" -ne 0 ]; then
  MERGE_ARGS+=(--admin)
fi

echo
echo "  ▶ gh pr merge $PR ${MERGE_ARGS[*]}"
if [ "$DRY_RUN" -eq 1 ]; then
  echo "  ⏸  --dry-run: NOT executed."
  exit 0
fi
gh pr merge "$PR" "${MERGE_ARGS[@]}"
