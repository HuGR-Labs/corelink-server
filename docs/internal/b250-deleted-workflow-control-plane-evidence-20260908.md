# B-250 control-plane evidence — 2026-09-08

This note records the owner-authorized, read-only inspection and the two narrow
administrative mutation attempts against the stale deleted workflow. The run
count below is a **capture-time population** for the stated bounded query, not
a permanent total and not a closure claim. B-250 remains HOLD/open until the
administrative scheduler record is removed or a new control-plane owner proves
that it is gone. This note does not persist raw run IDs, timestamps, logs, or
secrets.

## Live read

The workflow-scoped endpoint
`GET /repos/HuGR-Labs/corelink-server/actions/workflows/303501160` returned:

```json
{"id":303501160,"name":"","path":"BuildFailed","state":"deleted"}
```

The bounded run listing captured at this inspection time
`GET /repos/HuGR-Labs/corelink-server/actions/workflows/303501160/runs`
with `created=2026-09-06T00:00:00Z..2026-09-09T00:00:00Z` returned 236 runs.
The returned population was `startup_failure` and had these event counts:

| event | count |
|---|---:|
| `schedule` | 178 |
| `pull_request_target` | 29 |
| `pull_request` | 14 |
| `push` | 12 |
| `workflow_run` | 3 |

All 236 runs were completed with `startup_failure`, actor `gmhelmold`, and
workflow ID `303501160`. This confirms that the stale emitter is still active;
it is not a quiet historical window. The 278-run 2026-09-04..2026-09-06
population remains the canonical B-250 evidence packet.

## Narrow mutation attempts

The authorized disable request:

```text
PUT /repos/HuGR-Labs/corelink-server/actions/workflows/303501160/disable
HTTP 403
{"message":"Unable to disable a workflow that is not active.","status":403}
```

The exact-workflow delete request:

```text
DELETE /repos/HuGR-Labs/corelink-server/actions/workflows/303501160
HTTP 404
{"message":"Not Found","status":404}
```

No run was dispatched, rerun, cancelled, or deleted. The GitHub API exposes
the workflow as `deleted`, while its stale schedule/event emitters continue to
produce startup failures. Further repair requires the repository/organization
Actions owner to remove the stale scheduler record through an administrative
path that is not exposed by these workflow endpoints.
