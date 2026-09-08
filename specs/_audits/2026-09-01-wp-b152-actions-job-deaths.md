# WP-B152 — GitHub Actions job-death evidence (2026-09-01)

## Result

The common **signature** is reproducible in the retained Actions metadata, but the
common **cause** is not provable today. This is a HALT/indeterminate result, not a
closure and not a workflow/code fix.

At capture time, the read-only observation covered the UTC window
`2026-08-31T03:50:00Z` through `2026-08-31T07:20:00Z` and selected 1,621 runs.
GitHub's run search exceeded the 1,000-result cap for the combined interval; the
collector's executable verifier therefore recursively splits at UTC midpoints,
uses half-open bounds, and paginates every resulting leaf. Known-run controls
`33355619309` and `33359740663` were present in the complete selected set. The
command below is the verifier for these historical facts: if any current API page
is unavailable, malformed, capped, or incomplete, it must report `INDETERMINATE`
rather than reassert them. No token, log body, or other secret is stored here.

## Read-only refresh (2026-09-08)

The same closed historical window was queried again from the Actions API (no
dispatch, rerun, mutation, log body, or secret access). The 1,621-run
population and all three known run/job controls are still present. Their
durations remain exactly 600 seconds, with one independent lane each. On this
refresh the Jobs API returned an empty `steps` array for each retained match;
the collector records `step_evidence_available: false` and does not infer either
an in-progress step or completed checkout from that absence. The timing and
identity metadata remain usable; step-level metadata has not been retained by
the current API response.

The prior 2026-09-05 refresh also covered `2026-09-04T00:00:00Z` through
`2026-09-05T23:59:59Z`: 286 runs, all `startup_failure`, with zero jobs. A
sample run pointed to workflow ID `303501160`, whose read-only workflow record
was the deleted `BuildFailed` path. This is a separate Actions control-plane
failure, not evidence that the B-152 600-second jobs were cured or caused.
The repository diagnostic now retains non-success run conclusions (including
`startup_failure`) instead of reporting this population as an empty set.

## Closed-population snapshot (refreshed 2026-09-08)

The exact historical window was collected again through the read-only Actions
API and persisted at
`reports/perf/b152-actions-2026-09-06.json`. The snapshot contains all 1,621
selected run IDs, all 33 failed-run job objects, and the three in-window jobs;
each failed job retains its run ID, job ID, creation/start/completion times,
runner, and duration. The current response has no retained step records for the
three matches, so each carries `step_evidence_available: false`; no step status
or checkout claim is made. It contains no log body. The three log requests were
unavailable again, so each has the fixed `log_signature: "indeterminate"` and no
digest; no causal conclusion is asserted.

The snapshot is checked offline with the fail-closed verifier below. Missing
run/job/step identity, a duplicated ID, an omitted log signature, an
unavailable log marked causal, or a zero-population closure guard is a verifier
failure. The verifier also rejects a `closed` causal result, so a clean window
cannot close B-152.

```sh
python3 scripts/verify_b152_snapshot.py reports/perf/b152-actions-2026-09-06.json
```

## Retained failed-job matches

The API returned 33 failed runs and 33 failed jobs in this window; exactly three
failed jobs fell in the inclusive 594–615 second duration window:

| run ID | job ID | workflow / lane | job created → started | started → completed | duration | declared job timeout | step metadata / checkout | runner |
|---:|---:|---|---|---|---:|---:|---|---|---|
| 33355619309 | 99377079175 | docs CI / `typecheck + lint + test + build` | 04:00:22 → 04:03:26Z (184 s) | 04:03:26 → 04:13:26Z | 600 s | 20 min (`.github/workflows/docs-ci.yml:132`) | unavailable (`steps=[]`; checkout unknown) | `cf-runner-ad7db4f0` |
| 33356199472 | 99378740877 | OKF-CoreLink Wiki Validation / `okf-wiki-validation` | 04:11:09 → 04:13:30Z (141 s) | 04:13:30 → 04:23:30Z | 600 s | no explicit `timeout-minutes` (`.github/workflows/okf_wiki.yml`; platform default applies) | unavailable (`steps=[]`; checkout unknown) | `cf-runner-8bf42fee` |
| 33359740663 | 99388658702 | admin-ui e2e / `playwright critical-flows (chromium)` | 05:13:13 → 05:13:14Z (1 s) | 05:13:14 → 05:23:14Z | 600 s | 30 min (`.github/workflows/admin-ui-e2e.yml:111`) | unavailable (`steps=[]`; checkout unknown) | `cf-runner-9debaeaa` |

The exact lane distribution is one match each; the three jobs use different
workflow suites and different ephemeral runner names. The current API response
does not retain step state for any of the three, so it cannot establish whether
the jobs stopped during setup or after checkout. The earlier step-level claims
are historical evidence from the 2026-09-06 capture, not claims made by this
refresh.

The regenerated collector output derives the queue figures above from the job
objects (rather than from prose): `count=3`, `min=1 s`, `median=141 s`,
`max=184 s`, with the source explicitly recorded as
`job.created_at -> job.started_at`. The three raw job/run ID pairs are retained
in the table; the command's JSON output retains all selected run IDs.

The observed duration is `completed_at - started_at`. It is not the workflow's
`timeout-minutes`; in particular the 20-minute docs build and 30-minute Playwright
job both stopped at 600 seconds. Run `started_at` is also not queue wait. The
observed queue waits (`job.created_at - job.started_at`) were approximately 184 s,
141 s, and 1 s respectively.

## Logs and causal classification

`GET /repos/HuGR-Labs/corelink-server/actions/jobs/<job-id>/logs` returned HTTP
404 `BlobNotFound` for all three retained matches. The collector records each as
`{"status":"indeterminate","causal":"indeterminate","log_signature":"indeterminate"}`
and emits the top-level `causal_classification.status = "indeterminate"`; a
normal-looking report with a missing log is not a valid result. When a log is
available, the collector stores only a SHA-256 digest and a conservative
signature from the explicit categories `runner_cancellation`,
`playwright_webserver`, `job_timeout`, `billing_or_startup`,
`enospc_or_linker_failure`, `test_failure`, or `indeterminate`; availability
alone never establishes a common cause. The current unavailable blobs therefore
do not distinguish runner termination, Playwright webServer failure, a job
timeout, billing/startup failure, or ENOSPC, and no ENOSPC refutation can be made
from currently available evidence.

Classification: **common 600-second signature confirmed; step metadata and
common root cause unclassified; HALT pending log/artifact recovery or a fresh
instrumented occurrence**. A zero count in a later window must not close B-152:
it measures only that window and is not evidence that the intermittent fault was
removed.

## Reproduction

From the repository root, with `gh` authenticated read-only:

```sh
PYTHONPATH=scripts python3 scripts/b152_actions_diagnostic.py \
  --start 2026-08-31T03:50:00Z --end 2026-08-31T07:20:00Z \
  --known-run 33355619309 --known-run 33356199472 --known-run 33359740663 \
  --fetch-logs --output /tmp/b152-report.json
```

For a retained report, run the snapshot verifier as well:

```sh
python3 scripts/verify_b152_snapshot.py /tmp/b152-report.json
```

`--fetch-logs` records only a fixed availability/error category; it never writes
log contents or raw command errors. Fractional runtime/queue values are rejected
rather than truncated, while fractional CLI bounds through six digits are
preserved in the API query; greater precision is rejected rather than rounded.
Step status evidence accepts the explicit API states `queued`, `in_progress`,
`pending`, and `completed`; any other value, including a non-string JSON value,
exits `INDETERMINATE`. `pending` is retained as unfinished evidence because it
is present on historical post-kill job objects. Each `gh` request has a
30-second timeout and at most three
attempts with bounded 1/2-second retry delays. API/network failures, malformed
or truncated pages, exhausted API timeouts, and output-write failures exit with
`INDETERMINATE`; an exhausted log-download timeout remains the structured,
non-causal `indeterminate` outcome because it does not invalidate the metadata
walk. The test suite includes a load-bearing mutation where a page claims 1,000
results: the collector must split the interval and apply exact half-open UTC
bounds instead of silently accepting a capped result. It also rejects a short
page whose declared total requires further records; an available log alone must
remain `not_established`, never a causal attribution. The same suite is an
explicit required input to `.github/workflows/python-tests.yml` on pull requests,
pushes to `main`, and manual dispatch; deleting either its required-suite entry
or its pytest invocation makes the suite red.

```sh
python3 scripts/test_b152_actions_diagnostic.py
python3 scripts/test_b152_snapshot.py
```

## Proposed backlog follow-up

| status | owner | exact verifier | evidence | completeness | next action |
|---|---|---|---|---|---|
| **OPEN — B-152** | GitHub Actions / runner operations owner | Run the command above with both known-run controls; preserve any newly available job log or runner-side artifact under the incident retention process, then correlate it to the exact job ID and timestamp. | Three independent lanes share the 600-second signature, but all retained job-log blobs were unavailable and none establishes a causal chain. | Incomplete: logs/artifacts cannot presently prove causation. | Recover retained artifacts if still possible, or capture a fresh instrumented occurrence. Close only when the exact cause is evidenced; a clean later window or an available log without such evidence is not closure. |
| **IMPLEMENTED IN THIS DIAGNOSTIC — no backlog state change** | B-152 diagnostic maintainer | Every run/job conclusion is validated against an explicit Actions contract before filtering; unknown/non-string values exit `INDETERMINATE`. | `test_unknown_run_conclusion_fails_closed_before_failure_filtering` and `test_unknown_job_conclusion_fails_closed_before_job_filtering`. | Complete for repository-controlled collection; production cause remains open. | Keep the tests required by `python-tests.yml`; do not treat this structural guard as B-152 causal closure. |
| **IMPLEMENTED IN THIS DIAGNOSTIC — no backlog state change** | B-152 diagnostic maintainer | Job IDs are checked globally across selected failed runs; repeated identity exits `INDETERMINATE`. | `test_duplicate_job_ids_across_runs_fail_closed`. | Complete for repository-controlled collection; production cause remains open. | Preserve the exact identity contract and retain any future runner artifact by run/job ID. |
