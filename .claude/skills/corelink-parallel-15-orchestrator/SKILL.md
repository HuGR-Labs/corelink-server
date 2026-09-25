---
name: corelink-parallel-15-orchestrator
description: >-
  Orchestrate the fixed 2026-09-25 cross-repository package of 15 CoreLink
  issues with Luna agents, isolated worktrees, scoped hosted CI, one cold
  review, and central-only merge authority.
---

# CoreLink Parallel 15 Orchestrator

Use this skill only with:

1. `docs/handoff/2026-09-25-parallel-15-issue-orchestrator-handoff.md`
2. `docs/handoff/2026-09-25-parallel-15-issue-orchestrator-manifest.json`

## Fixed authority

- Own exactly the 15 manifest entries. Do not absorb nearby issues.
- GitHub issues are the canonical ledger. Refresh issue, PR, default branch, and ownership before every dispatch.
- The original central session owns every merge and final issue closure. This session produces merge-ready PRs and evidence only.
- Use Luna for implementation and cold review. Terra requires an explicit complexity escalation. Sol is forbidden without fresh user authorization.
- Never touch OKF work, provider consoles, secrets, staging, production, release publication, legal decisions, or another session's owned files.

## Rolling loop

1. Reconcile live state and mark ownership in the issue.
2. Select only ready DAG nodes whose owned files do not overlap any active node.
3. Give one implementation agent one issue, one branch, one isolated worktree, the five acceptance sections, exact paths, CI pack, stop conditions, and compact return schema.
4. Refill a released slot immediately while useful ready work exists. Keep up to 12 Luna agents in flight, reserving capacity for independent cold reviewers.
5. Run all tests, builds, lints, and long work asynchronously on GitHub-hosted Actions at the exact PR SHA. Never use `runs-on: corelink`.
6. Cold-review the exact head once. Permit at most one consolidated correction. Then return `MERGE_READY`, `BLOCKED`, or `TERMINAL_REJECT`.
7. Deliver the green PR to the central merge owner. Do not merge or close the issue.
8. Remove the worktree and temporary refs after the PR is safely pushed and evidence recorded.

## Branch and worktree contract

- Start from a fresh fetch of the live default branch.
- Use `codex/issue-<number>-<slug>-20260925` and a unique path under `/private/tmp`.
- Never implement in a shared or dirty checkout. Never use `git reset --hard`, `git clean -fd`, blanket `git stash`, or deletion of unrelated refs.
- Before push, re-fetch the base, reconcile drift, verify the diff and owned paths, and record base/head SHAs.
- Force push is allowed only for the two explicitly listed rescue PRs, with `--force-with-lease` against the recorded old OID and unchanged patch intent.
- Every commit must have a valid signature and `Signed-off-by` trailer.

## Acceptance and review contract

Every issue packet and PR must preserve the issue's:

- Success criteria
- Definition of done
- Completeness criteria
- Invariants
- Quality standards

The cold reviewer receives the issue contract, exact diff, exact head SHA, and hosted receipts without the author's reasoning. The only useful verdicts are `APPROVE`, `FIX-FIRST` with one consolidated defect list, `BLOCKED`, or `TERMINAL_REJECT`.

## Return firewall

Return one compact row per issue: repository/issue, status, PR, base SHA, head SHA, changed paths, hosted run IDs, cold verdict, closure eligibility, blocker, and cleanup state. Put long evidence in the issue or final handoff, not in chat.

