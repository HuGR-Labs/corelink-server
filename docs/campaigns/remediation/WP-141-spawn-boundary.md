# WP-141 — `pull_request_target` spawn boundary

Date: 2026-09-05
Decision: hybrid actor gate plus Mac retention

## Decision and boundary

`pr-labels.yml` retains the ephemeral `corelink` runner only for pull requests
whose GitHub-owned `author_association` is `OWNER`, `MEMBER`, or
`COLLABORATOR`. The condition is job-level, so GitHub skips an external PR's
label and size jobs before it allocates a runner. `CONTRIBUTOR`,
`FIRST_TIME_CONTRIBUTOR`, `FIRST_TIMER`, and `NONE` are deliberately outside
the allowlist. Missing or unexpected association data is therefore fail-closed.

`welcome-first-pr.yml` must still greet a genuine first-time issue or fork-PR
author, so it cannot take that allowlist. It runs instead on the exact
`[self-hosted, mac, corelink-builder]` pool. That preserves the product purpose
without making an unauthenticated public event invoke the ephemeral fabric.
The five-minute timeout limits only a running greeting; it is not represented
as a queue deadline.

The same Mac boundary is the permanent policy for the non-Dependabot sentinel
in `dependabot-policy.yml`: it remains a required, no-op success for ordinary
PRs, but an external PR cannot allocate `corelink`. Its `policy-gate` job and
the `dependabot-auto-merge.yml` job are both conjunctively restricted to the
Dependabot actor and PR author, so a human event on a Dependabot PR cannot
reach the write-capable fabric lane.

This is intentionally not an `isPrivate` control. Repository visibility is not
an admission control and is not consulted by either workflow.

## Queue and recovery evidence

The fabric's recovery is not an admission control: its `redriveOrphanedJobs`
cron retries queued, labelled, runnerless jobs, while the retry/dead-letter
budget is bounded. A refused or lost fabric spawn therefore must not be used to
justify accepting unauthenticated work. The external paths above cannot reach
that fabric lifecycle.

The retained Mac path has a separate, reachable recovery: the local
`runner_wedge_watchdog.py` watches a job queued for at least 600 seconds and
requires no live `Runner.Worker` before it uses `launchctl kickstart`. It does
not claim to time out a GitHub queue. This is the documented recovery path in
`docs/internal/ci-runner-fabric-box.md`.

## Population and proof

The complete `pull_request_target` population on this baseline is
`dependabot-auto-merge.yml`, `dependabot-policy.yml`, `file-size-ratchet.yml`,
`pr-labels.yml`, and `welcome-first-pr.yml`. The two Dependabot lanes have
their independent Dependabot actor gates; this change also moves the
non-Dependabot policy sentinel off the fabric. `tests/test_pull_request_target_spawn_boundary.py`
fails if any `corelink` job loses its closed actor gate, if a non-gated job
returns to `corelink`, if the welcome lane returns to the fabric or loses its
runtime bound, if permissions drift, or if the trigger population changes. Its
mutation cases prove that removing or widening an actor gate, moving a safe job
back to the fabric, and broadening token scopes are not silently accepted.

The permanent policy decision is recorded here: first-timer greetings and the
ordinary-PR sentinel remain on `[self-hosted, mac, corelink-builder]`; only
trusted associations (or the exact Dependabot bot identity) may allocate
`corelink`. B-141 is therefore closed. No repository-visibility observation or
unverified external spawn quota is part of the admission proof.
