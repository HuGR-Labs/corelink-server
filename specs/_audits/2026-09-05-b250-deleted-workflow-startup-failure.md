# B-250 — deleted `BuildFailed` workflow emits startup failures

## HOLD / evidence

The canonical snapshot is a read-only Actions API refresh for
`HuGR-Labs/corelink-server` over the closed UTC window
`2026-09-04T00:00:00Z` (inclusive) through `2026-09-06T00:00:00Z` (exclusive).
The workflow-scoped endpoint for workflow ID `303501160` returned **278 runs**.
Every run had conclusion `startup_failure`; every run's jobs endpoint returned
an empty list. The workflow metadata identifies path `BuildFailed` and state
`deleted`.

The population is bound server-side by requesting
`/actions/workflows/303501160/runs`, not the global `/actions/runs` listing.
Therefore unrelated `backlog-verify` and Dependabot runs cannot be counted as
this finding. The verifier also checks every returned run's `workflow_id` as a
second binding.

This is a separate Actions control-plane finding surfaced during B-152 work. It
does not explain or cure the three historical 600-second job deaths: those are
ordinary failed jobs in distinct workflows and remain B-152 `OPEN` with cause
indeterminate. No run was dispatched, rerun, cancelled, or mutated, and no log
body or secret was retrieved.

The repository verifier is deliberately a HOLD instrument. It reads the
retained redacted snapshot beside this document; it performs no API, `gh`,
history, jobs, or log operation. Its repository, window, count (`278`),
workflow ID (`303501160`), path, and state are constants; the CLI accepts only
an optional output path, so no invocation can redefine the truth. The snapshot
retains the count/location of the original run IDs and timestamps without
copying raw values into the repository. It carries `unique_count: 278`,
`duplicate_count: 0`, and a SHA-256 uniqueness witness over the canonical
population identity tuple. The source record pins `last_refreshed`, max-age,
and this document's SHA-256/content markers. It returns exit 0 only when the exact
bounded population and deleted-workflow identity are observed, and its report
has `closure_permitted: false`. A changed count, conclusion, workflow
identity/state, missing redaction/provenance, or owner closure claim returns
`INDETERMINATE` (exit 2). A later empty window cannot close this finding.

## Exact verifier

From the repository root; this canonical check requires no network or `gh` authentication:

```sh
python3 scripts/b250_deleted_workflow_startup_failure.py \
  --output /tmp/b250-report.json
```

The focal mutation suite is:

```sh
python3 scripts/test_b250_deleted_workflow_startup_failure.py
```

The live refresh is an owner-only action and is not the canonical verifier:

```sh
gh api --paginate --slurp \
  "repos/HuGR-Labs/corelink-server/actions/workflows/303501160/runs?per_page=100&created=2026-09-04T00:00:00Z..2026-09-06T00:00:00Z" \
  > /tmp/b250-owner-runs.json
gh api "repos/HuGR-Labs/corelink-server/actions/workflows/303501160" \
  > /tmp/b250-owner-workflow.json
```

The owner must independently preserve and review the returned IDs/timestamps;
the local gate never imports this live payload or changes the HOLD decision.

## Action packet

| owner | status | action | evidence required to change status |
|---|---|---|---|
| `tl` | HOLD | Identify the owner of the scheduler/automation still targeting deleted workflow `303501160` (`BuildFailed`), disable or repair that trigger through the authorized GitHub administration path, and preserve the exact run/workflow IDs and timestamps. | A read-only run listing shows the startup-failure population stops for a documented reason, and the workflow/trigger state change is independently recorded. |

`tl` is the accountable backlog owner for this repository finding, not proof
that `tl` has exclusive authority to change GitHub scheduler state. Do not mark
B-250 complete from a quiet window, a rerun, or a green unrelated workflow.
