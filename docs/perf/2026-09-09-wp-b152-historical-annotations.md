# B-152 historical Actions diagnosis — 2026-09-09

Status: **OPEN; common immediate failure identified, underlying mechanism not retained**

## Result

The complete historical window was refreshed through the GitHub Actions API on
2026-09-09. It is byte-identical to the sealed snapshot: 1,621 runs, 33 failed
runs/jobs, and exactly three jobs in the inclusive 594–615 second window. All
three lasted exactly 600 seconds, occupied different workflows and runner
instances, and have distinct queue durations (1, 141, and 184 seconds).

The previously unqueried Check Runs annotations endpoint retains one
GitHub-generated failure annotation for each job. All three annotations are
identical and classify the immediate failure as loss of communication between
the self-hosted runner and GitHub. The annotation digest is identical across
the three jobs. This is positive evidence that the common immediate failure is
runner loss, not a shared test suite or the declared workflow timeout.

The PR #1492-specific population corrects a second historical ambiguity. The
Actions query explicitly used `branch=claude/wp-a-b080-scopes`,
`event=pull_request`, the closed UTC `created` interval, `per_page=100`, and
pagination. It returns 207 workflow runs for the branch and exactly three jobs from
non-success runs in the 594–615 second duration window on the relevant push:

| job | conclusion | observed duration | declared timeout | retained annotation |
|---|---|---:|---:|---|
| `dco` | cancelled | 600 s | 5 min | exceeded maximum execution time of 5m0s |
| `playwright critical-flows (chromium)` | failure | 600 s | 30 min | self-hosted runner lost communication |
| `proptest density gate` | cancelled | 600 s | 5 min | exceeded maximum execution time of 5m0s |

Therefore PR #1492 does not contain five instances of one defect. Its retained
600-second population has two explicit job-timeout terminations and one runner
communication loss. The other two failed jobs in the closed-window snapshot
belong to different branches (`claude/wp-c-b085` and `claude/wp-h-okf`), and both
carry the runner-loss annotation. The former wording combined five temporally
near records while attributing all of them to one PR and one cause; the API
does not support either attribution.

This does **not** yet establish which mechanism made the three runner-loss jobs disappear.
GitHub's annotation explicitly leaves process termination, CPU/memory starvation,
and network loss as alternatives. The jobs endpoint now returns zero retained
steps, the artifacts endpoint returns zero artifacts, and every job-log endpoint
returns HTTP 404. Therefore the historical API cannot distinguish those
alternatives or prove that the mechanism was removed. B-152 remains open under
its existing closure rule; the result is narrower than the former wholly
indeterminate classification, but it is not a fabricated causal closure.

## Evidence

The compact evidence record is
`reports/perf/b152-actions-check-annotations-2026-09-09.json`. It contains no
credential, raw log body, or mutable URL. Its source calls were read-only:

```sh
PYTHONPATH=scripts python3 scripts/b152_actions_diagnostic.py \
  --start 2026-08-31T03:50:00Z --end 2026-08-31T07:20:00Z \
  --known-run 33355619309 --known-run 33356199472 \
  --known-run 33359740663 --fetch-logs \
  --output /tmp/b152-actions-history-2026-09-09.json

gh api repos/HuGR-Labs/corelink-server/check-runs/JOB_ID
gh api 'repos/HuGR-Labs/corelink-server/check-runs/JOB_ID/annotations?per_page=100'
gh api repos/HuGR-Labs/corelink-server/actions/runs/RUN_ID/attempts/1/jobs?per_page=100
gh api repos/HuGR-Labs/corelink-server/actions/runs/RUN_ID/artifacts?per_page=100
gh api repos/HuGR-Labs/corelink-server/actions/jobs/JOB_ID/logs

gh api --paginate \
  'repos/HuGR-Labs/corelink-server/actions/runs?branch=claude/wp-a-b080-scopes&event=pull_request&created=2026-08-31T03:50:00Z..2026-08-31T07:20:00Z&per_page=100'
```

Its closed population, query filter, per-job annotation identity and non-closure
boundary are mutation-tested:

```sh
python3 scripts/verify_b152_check_annotations.py \
  reports/perf/b152-actions-check-annotations-2026-09-09.json
python3 scripts/test_verify_b152_check_annotations.py
```

The final call returned HTTP 404 for job IDs `99377079175`, `99378740877`,
`99388658702`, `99388657822`, and `99388658533`. Check Run and annotation calls
returned HTTP 200 for all five.
The refreshed snapshot SHA-256 is
`6eab5821a3f010aff008cb99ebd4bcbe528b4479a01ad5e649d1a26c11aae38f`,
the same digest as `reports/perf/b152-actions-2026-09-06.json`.

## Boundary and next proof

- B-128 is not asserted: the retained API data contains no ENOSPC signature.
- The two five-minute timeout jobs are classified from their exact workflow
  definitions at head SHA `72a7908f5a` and their GitHub annotations; their
  600-second API duration does not turn them into runner-loss cases.
- A clean later observation window cannot close this historical incident.
- No workflow implementation is changed from an annotation alone.
- Closure requires evidence for the mechanism below runner communication loss,
  its remediation, and a post-remediation observation capable of detecting its
  recurrence.
