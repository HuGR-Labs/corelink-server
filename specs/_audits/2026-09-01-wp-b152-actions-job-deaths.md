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

## Retained failed-job matches

The API returned 33 failed runs and 33 failed jobs in this window; exactly three
failed jobs fell in the inclusive 594–615 second duration window:

| run ID | job ID | workflow / lane | job created → started | started → completed | duration | declared job timeout | step still `in_progress` | checkout incomplete | runner |
|---:|---:|---|---|---|---:|---:|---|---|---|
| 33355619309 | 99377079175 | docs CI / `typecheck + lint + test + build` | 04:00:22 → 04:03:26Z (184 s) | 04:03:26 → 04:13:26Z | 600 s | 20 min (`.github/workflows/docs-ci.yml:132`) | Set up Node.js | no | `cf-runner-ad7db4f0` |
| 33356199472 | 99378740877 | OKF-CoreLink Wiki Validation / `okf-wiki-validation` | 04:11:09 → 04:13:30Z (141 s) | 04:13:30 → 04:23:30Z | 600 s | no explicit `timeout-minutes` (`.github/workflows/okf_wiki.yml`; platform default applies) | Install PyYAML (venv on system python3) | no | `cf-runner-8bf42fee` |
| 33359740663 | 99388658702 | admin-ui e2e / `playwright critical-flows (chromium)` | 05:13:13 → 05:13:14Z (1 s) | 05:13:14 → 05:23:14Z | 600 s | 30 min (`.github/workflows/admin-ui-e2e.yml:111`) | Set up pnpm | no | `cf-runner-9debaeaa` |

The exact lane distribution is one match each; the three jobs use different
workflow suites and different ephemeral runner names. All three stopped with a
step in progress. The retained records show checkout completed before the
in-progress step; the earlier claim that one checkout was incomplete cannot be
rechecked because that job attempt is no longer in the API result set.

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
`{"status":"indeterminate","causal":"indeterminate"}` and emits the top-level
`causal_classification.status = "indeterminate"`; a normal-looking report with a
missing log is not a valid result. This is consistent with an expired/unavailable
log blob, but it does not distinguish runner termination, service interruption, a
wrapper timeout, or another external kill. The logs therefore cannot be classified
as clean, and no `ENOSPC`, `os error 28`, or `Bus error` refutation can be made from
currently available evidence.

Classification: **common 600-second signature confirmed; common root cause
unclassified; HALT pending log/artifact recovery or a fresh instrumented
occurrence**. A zero count in a later window must not close B-152: it measures only
that window and is not evidence that the intermittent fault was removed.

## Reproduction

From the repository root, with `gh` authenticated read-only:

```sh
PYTHONPATH=scripts python3 scripts/b152_actions_diagnostic.py \
  --start 2026-08-31T03:50:00Z --end 2026-08-31T07:20:00Z \
  --known-run 33355619309 --known-run 33359740663 \
  --fetch-logs --output /tmp/b152-report.json
```

`--fetch-logs` records only a fixed availability/error category; it never writes
log contents or raw command errors. Fractional runtime/queue values are rejected
rather than truncated, while fractional CLI bounds through six digits are
preserved in the API query; greater precision is rejected rather than rounded.
Step status evidence is limited to the API states `queued`, `in_progress`, and
`completed`; any other value exits `INDETERMINATE`. API/network failures,
malformed or truncated pages, and output-write failures exit with
`INDETERMINATE`. The test suite includes a load-bearing mutation where a page
claims 1,000 results: the collector must split the interval and apply exact
half-open UTC bounds instead of silently accepting a capped result. It also rejects a
short page whose declared total requires further records; an available log alone
must remain `not_established`, never a causal attribution.

```sh
python3 scripts/test_b152_actions_diagnostic.py
```

## Proposed backlog follow-up

| status | owner | exact verifier | evidence | completeness | next action |
|---|---|---|---|---|---|
| **OPEN — B-152** | GitHub Actions / runner operations owner | Run the command above with both known-run controls; preserve any newly available job log or runner-side artifact under the incident retention process, then correlate it to the exact job ID and timestamp. | Three independent lanes share the 600-second signature, but all retained job-log blobs were unavailable and none establishes a causal chain. | Incomplete: logs/artifacts cannot presently prove causation. | Recover retained artifacts if still possible, or capture a fresh instrumented occurrence. Close only when the exact cause is evidenced; a clean later window or an available log without such evidence is not closure. |
| **PROPOSED — B-170, OPEN** | B-152 diagnostic maintainer | Add fixture tests where a selected run or listed job has a non-string/unknown `conclusion`; the collector must exit `INDETERMINATE` rather than omit it from failure accounting. | Current collection filters on `conclusion == "failure"` without structurally validating all conclusion values; malformed values could reduce the failed set silently. | Incomplete: review finding only; no production fixture is retained. | Define accepted GitHub conclusion states from the API contract, validate before filtering, add red/restore mutation coverage, and link the resulting commit here. |
| **PROPOSED — B-171, OPEN** | B-152 diagnostic maintainer | Add a multi-run fixture that repeats one job ID in different runs; assert either globally unique evidence or an `INDETERMINATE` result according to the documented API identity contract. | The collector rejects duplicate job IDs within one run, but does not yet detect a repeated job ID across distinct failed runs. | Incomplete: review finding only; no retained API response demonstrates cross-run duplication. | Establish whether GitHub job IDs are globally unique for this endpoint, enforce the chosen invariant across the collected evidence set, add red/restore mutation coverage, and link the resulting commit here. |
