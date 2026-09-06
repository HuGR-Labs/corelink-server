# OKF auto-reconcile runbook

The production automation is `.github/workflows/okf-autoreconcile.yml`.
It runs after a trusted push to `main` when an OKF grounding surface changes,
or by an operator dispatch on `main`. It deliberately has no `pull_request` or
`pull_request_target` trigger: the `OPENAI_API_KEY` is never exposed to fork
code. The local `scripts/okf-reconcile-local.sh` wrapper remains a manual
fallback and is not evidence that the Actions bot ran.

## Runner and inputs

The job is pinned to `[self-hosted, mac, corelink-builder]`, the persistent
authenticated macOS runner tuple. The ephemeral `corelink` label is not a
valid target for the Codex action: it has no founder CLI login. Checkout uses
full history and `persist-credentials: false`; actions and the Codex version
are pinned, no package installation is performed, and the job has a 20-minute
timeout.

## Safety contract

Before Codex starts, the workflow records HEAD, branch, refs, remotes, and
worktrees and installs the deny-by-default Git/`gh` wrappers. Codex may inspect
the repository and write only the OKF documentation trees. After it exits, the
workflow rejects any Git-state mutation, any denied command attempt, and any
tracked or untracked file outside `docs/knowledge/**`,
`docs/internal/okf-wiki/**`, and `docs/knowledge/index.md`. It then runs the
OKF validator and fixture suite before creating a PR.

The B-058 contract self-test runs before any Codex invocation. It mutates the
runner, automatic trigger, fork boundary, path filter, guard, and state
snapshot and requires all six mutations to fail. This proves the workflow
still has an opinion; it is not a claim about historical GitHub run counts.

## Triage

1. If the contract self-test is red, do not dispatch the agent. Restore the
   runner tuple, main-only trigger, complete path list, and both guards first.
2. If the reporter finds drift but Codex is skipped, inspect the Codex action
   step and `OPENAI_API_KEY` provisioning. A skipped step is not an execution
   success.
3. If containment fails, keep the run red and inspect the mutation log and
   before/after state diff. Do not open the generated PR manually from that
   run.
4. If validation fails, use the reporter worklist and re-author claims against
   current source. Never advance a checkpoint without a real citation/claim
   edit.
