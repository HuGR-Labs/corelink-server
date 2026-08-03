#!/usr/bin/env bash
# okf-reconcile-local.sh — the OKF self-healing "LLM half", run LOCALLY with your Claude
# Code CLI auth (your login/subscription — NO ANTHROPIC_API_KEY, no CI secret, no API billing).
#
# It detects whether a code change drifted any OKF concept's cited lines (the deterministic
# `okf_reconcile.py` reporter) and, only if drift exists, runs the `claude` CLI headless on the
# `okf-reconcile` skill to re-anchor the affected concepts to current code (claims re-verified,
# checkpoints advanced), then validates fail-closed.
#
# USAGE:
#   scripts/okf-reconcile-local.sh --check   # report drift only; exit 1 if drift (for hooks/scripts)
#   scripts/okf-reconcile-local.sh           # reconcile IN this worktree, validate, leave staged for review
#   scripts/okf-reconcile-local.sh --pr      # reconcile in an ISOLATED worktree off origin/main, validate,
#                                            # commit + open a PR (never touches your working tree)
#
# REQUIRES: `claude` on PATH (you're already logged in); `gh` for --pr. Run from the repo root.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"
MODE="${1:-inplace}"

drift_count() { python3 scripts/okf_reconcile.py --json | python3 -c "import json,sys; print(json.load(sys.stdin)['stale_count'])"; }

# ── --check: fast, no agent, no cost ──────────────────────────────────────────
STALE="$(drift_count)"
if [[ "$MODE" == "--check" ]]; then
  echo "OKF drift: stale_count=$STALE"
  [[ "$STALE" == "0" ]] && exit 0 || { python3 scripts/okf_reconcile.py; exit 1; }
fi
if [[ "$STALE" == "0" ]]; then
  echo "OKF: 0 drift — nothing to reconcile."; exit 0
fi
command -v claude >/dev/null || { echo "ERROR: 'claude' CLI not on PATH (log in to Claude Code first)."; exit 2; }

PROMPT='You are reconciling the OKF architecture wiki after a code change, using the repo skill
.claude/skills/okf-reconcile/SKILL.md. The deterministic reporter has the worklist:
run `python3 scripts/okf_reconcile.py` to see which concepts drifted and which cited ranges moved.
For EACH stale concept: open its cited code at current HEAD, re-anchor its `# Citations` ranges (and
inline path:line) to the CURRENT lines, and CONFIRM the claim still holds — if the code changed the
behavior, UPDATE the claim to the current truth HONESTLY (never a blind line-bump). Advance each
touched concept checkpoint_sha to the current HEAD SHA (the C5b anti-phantom check needs a real body
edit alongside it — your cite/claim edit satisfies it), and for a concept that carries `source_blobs`,
advance each re-authored file's anchor to `git hash-object <path>` — that anchor is what C5 compares
against, and NEVER delete one to clear a red (C4c refuses it). Then `python3 scripts/okf_index.py`.
HARD CONSTRAINTS: edit ONLY docs/knowledge/** + docs/internal/okf-wiki/** + docs/knowledge/index.md;
NEVER edit code (crates/worker/apps/scripts/migrations) — you document it, not change it. Every cite
must resolve to a real path:line that performs the claim; every cited file in source_files. Do NOT git.
STOP when `python3 scripts/validate_okf.py` is 0 stale / 0 drift for the concepts you fixed.'

run_agent_and_validate() {
  echo ">> $STALE concept(s) drifted — running the okf-reconcile skill (claude, local auth)…"
  claude -p "$PROMPT" --dangerously-skip-permissions --max-turns 80
  python3 scripts/okf_index.py
  # safety net: refuse if the agent touched anything outside the doc trees
  if git diff --name-only | grep -vE '^docs/knowledge/|^docs/internal/okf-wiki/' | grep -q .; then
    echo "ERROR: agent edited files outside docs/knowledge — aborting."; git diff --name-only; exit 1
  fi
  python3 scripts/validate_okf.py
  bash tests/okf/run_fixtures.sh
}

# ── --pr: isolated worktree, non-destructive, opens a PR ──────────────────────
if [[ "$MODE" == "--pr" ]]; then
  command -v gh >/dev/null || { echo "ERROR: 'gh' CLI needed for --pr."; exit 2; }
  git fetch origin -q
  TS="$(date +%Y%m%d-%H%M%S)"; WT=".claude/worktrees/okf-reconcile-$TS"; BR="auto/okf-reconcile-local-$TS"
  git worktree add -q "$WT" -b "$BR" origin/main
  pushd "$WT" >/dev/null
  trap 'popd >/dev/null 2>&1 || true; git worktree remove --force "$WT" 2>/dev/null || true' EXIT
  if [[ "$(drift_count)" == "0" ]]; then echo "drift cleared on origin/main — nothing to do."; exit 0; fi
  run_agent_and_validate
  if git diff --quiet; then echo "agent made no changes — nothing to PR."; exit 0; fi
  HEAD_SHA="$(git -C "$(git rev-parse --show-toplevel)" rev-parse origin/main)"
  git add -A
  git commit -q -m "docs(okf): auto-reconcile concept citations after $HEAD_SHA" \
    -m "Local self-healing run (okf-reconcile skill, your CLI auth): drifted cited lines re-anchored to current code, claims re-verified, checkpoints advanced. validate + fixtures green." \
    -m "$(printf 'Signed-off-by: %s <%s>' "$(git config user.name)" "$(git config user.email)")"
  git push -q -u origin "$BR"
  gh pr create --base main --head "$BR" \
    --title "docs(okf): local auto-reconcile $TS" \
    --body "Self-healing OKF reconcile run locally with the Claude Code CLI (your auth — no API key). Pre-validated green (validate_okf + fixtures). Review the diff for claim-correctness and merge."
  echo ">> PR opened. (isolated worktree auto-removed)"
  exit 0
fi

# ── default: reconcile in place, leave staged for your review ─────────────────
run_agent_and_validate
echo ""
echo ">> OKF reconciled in place + validated GREEN. Review and commit:"
echo "     git diff            # inspect the reconciled concepts"
echo "     git add -A && git commit -m 'docs(okf): reconcile concept citations'"
