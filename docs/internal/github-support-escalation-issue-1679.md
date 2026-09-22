# GitHub Support escalation packet — issue #1679

**Prepared:** 2026-09-22 UTC<br>
**Repository:** `HuGR-dev/corelink-server`<br>
**Issue:** [#1679](https://github.com/HuGR-dev/corelink-server/issues/1679)<br>
**Status:** external; owner follow-up is required

## Subject

GitHub Actions continues creating completed `startup_failure` runs for the
deleted `BuildFailed` workflow record. The repository has no operational
workflow with this path or ID.

## Bounded read-only receipt

Workflow-scoped request:

```text
GET /repos/HuGR-dev/corelink-server/actions/workflows/303501160/runs
    ?per_page=100&created=2026-09-21T00:00:00Z..2026-09-22T23:59:59Z
```

Returned **206** runs in the bounded UTC window. Every run had workflow ID
`303501160`, path `BuildFailed`, status `completed`, conclusion
`startup_failure`, and zero jobs. Event counts were: `schedule` 67,
`workflow_run` 35, `pull_request` 29, `pull_request_target` 29, `push` 27,
and `issues` 19.

Representative readbacks:

| Run | Created (UTC) | Event | URL |
|---:|---|---|---|
| 35676257895 | 2026-09-22T01:34:26Z | workflow_run | [run](https://github.com/HuGR-dev/corelink-server/actions/runs/35676257895) |
| 35676256742 | 2026-09-22T01:34:25Z | push | [run](https://github.com/HuGR-dev/corelink-server/actions/runs/35676256742) |
| 35676256007 | 2026-09-22T01:34:24Z | pull_request_target | [run](https://github.com/HuGR-dev/corelink-server/actions/runs/35676256007) |

For run `35676257895`, `GET /actions/runs/35676257895/jobs` returned
`{"total_count":0,"jobs":[]}`. Its check suite
`96591715062` was completed with `startup_failure` and
`latest_check_runs_count: 0`; `GET /check-suites/96591715062/check-runs`
returned `{"total_count":0,"check_runs":[]}`.

Workflow metadata readback:

```json
{"id":303501160,"name":"","path":"BuildFailed","state":"deleted"}
```

The repository workflow inventory contains no live `BuildFailed` path or
`303501160` reference. Existing references are evidence, verifier, and
handoff documentation only.

## Existing narrow mutation receipts

These were previously attempted through the workflow API; no run was
dispatched, rerun, cancelled, or deleted:

```text
PUT /repos/HuGR-dev/corelink-server/actions/workflows/303501160/disable
HTTP 403
{"message":"Unable to disable a workflow that is not active.","status":403}

DELETE /repos/HuGR-dev/corelink-server/actions/workflows/303501160
HTTP 404
{"message":"Not Found","status":404}
```

## Requested GitHub action

Please inspect the Actions backend/control plane and purge or reindex the
stale workflow registration and dispatcher/event state for repository
`HuGR-dev/corelink-server`, preserving valid workflow definitions, check names,
run history, and runner registrations. Identify the stale emitter or scheduler
record if possible. Do not recreate the workflow or run production/deploy
workflows as part of diagnosis.

## Reproduction and success criterion

The read-only workflow-scoped listing above reproduces the condition and can be
re-read with the same bounded query. Success is a fresh non-mutating repository
event resolving to the real workflow graph and creating expected jobs/checks,
with no new run bound to deleted workflow `303501160`.

No new two-minute probe run was dispatched. No secrets, tokens, raw logs, or
personal account data are included in this packet.

**Owner follow-up:** GitHub Support or the authorized repository/organization
Actions owner should provide the case/reference ID and the backend repair or
reindex result; retain the bounded run IDs and timestamps for comparison.
