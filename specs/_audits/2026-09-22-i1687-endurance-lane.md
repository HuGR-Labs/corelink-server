# Issue #1687 bounded endurance lane

This change proves the hosted lane contract while the canonical staging
boundary remains unprovisioned under #1700. The workflow has
`workflow_dispatch` only, requires the exact `run-bounded-endurance`
confirmation, accepts exactly `2h`, and rejects every target except
`https://staging.corelink.humangr.com` after removing one trailing slash.

Each dispatch records the merged commit SHA, run and attempt identity, runner
identity, canonical target, lifecycle checkpoints, a 30 second heartbeat, and
SHA-256 digests for result artifacts. Both jobs use GitHub-hosted
`ubuntu-24.04`; both checkouts disable credential persistence. The k6 command
has a 130 minute graceful timeout inside the 145 minute job bound. After a
validated target identity, teardown runs after any load outcome and addresses
only the current GitHub run plus the `endurance-2h` scenario. Cleanup failure
fails the job and its HTTP outcome is recorded in the receipt. CAS writes and
webhook event IDs carry that same run ID for server-side attribution. The
lane sets `teardown_deletion_proven` to true only after validating the exact
server receipt; #2161 owns the missing handler and staging deployment. The
workflow pins the endurance population to 50 VUs, the scenario rejects any
population above that ceiling, and the receipt records the effective VU count.

The response contract proposed for #2161 is
`corelink.load-test-teardown-receipt.v1`: it binds the exact `run_id`,
`endurance-2h` scenario, staging `target_deployment_sha`, a complete inventory,
per-resource `inventory`/`attempted`/`deleted`/`remaining` counts, and
`cross_run_deletions: 0`. The required resource classes are `cas_objects`,
`webhook_idempotency_rows`, `dsr_jobs`, and `audit_entries`; expanding that set
requires updating this versioned contract before a new writer enters the lane.
The lane validates those fields against the GitHub
run and the deployment SHA in the owner-issued target identity receipt, then
retains only the normalized redacted response as
`teardown-receipt-${GITHUB_RUN_ID}.json` and includes it in artifact digests.
Redirects and status codes without a reconciled receipt
fail the lane and cannot set the deletion proof flag. The server-side handler
and its staging deployment remain owned by #2161, which is blocked on #1700.

Contract and mutation checks:

```sh
python3 scripts/test_i1687_endurance_lane.py
```

No live dispatch or staging evidence is claimed here. Issue #1687 stays open
until #1700 provisions the canonical target, #2161 implements and attests exact
run-scoped cleanup, and a successful hosted two-hour run retains the resulting
receipt.

## Hosted CI manifest

| Field | Exact value |
|---|---|
| Workflow | `.github/workflows/endurance-2h-nightly.yml` |
| Trigger | `workflow_dispatch` only; no schedule or PR trigger |
| Dispatch ref | canonical repository, protected `refs/heads/main` only |
| Input | Exact confirmation `run-bounded-endurance`; duration fixed at `2h` |
| Population | 50 VUs, pinned by the workflow and recorded in the receipt |
| Job / runner | `endurance-2h` and `baseline-drift-check` / GitHub-hosted `ubuntu-24.04` |
| Environment | `staging` |
| Permissions | `contents: read`, `actions: read`, `id-token: none` |
| Job timeout | 145 minutes |
| k6 timeout | 130 minutes; TERM grace 60 seconds |
| Canonical target | `https://staging.corelink.humangr.com` (one trailing slash accepted) |
| Receipt | `tests/load/results/endurance-2h/endurance-lane-receipt-${GITHUB_RUN_ID}.json` |
| Checkpoints | `prepared`, `running`, `completed` when successful, teardown outcome after accepted target identity |
| Heartbeat | `heartbeat.json`, refreshed every 30 seconds during k6 |
| Upload | Always runs; uploads `tests/load/results/endurance-2h/` |
