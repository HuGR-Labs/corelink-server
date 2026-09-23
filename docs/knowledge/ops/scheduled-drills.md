---
type: "Runbook"
title: "Scheduled drills"
description: "Fail-closed synthetic PagerDuty drill handoff seam with no active trigger."
checkpoint_sha: "648ecdccd229bdb5154b86843053c28b9cce9d36"
source_files:
  - "worker/src/index_common.ts"
  - "worker/src/index_schedule.ts"
  - "worker/src/index.ts"
  - "apps/synthetic-pager-worker/wrangler.toml"
  - "apps/synthetic-pager-worker/src/index.ts"
source_blobs:
  - "worker/src/index_common.ts@8ed0757b58effc33386205dc82d4b210fb4fe121"
  - "worker/src/index_schedule.ts@91e13983ad933a19c975028dbe3ea2cd076093d5"
  - "worker/src/index.ts@33446ee3c4cd0c85e285960d5c60d9770d548f11"
  - "apps/synthetic-pager-worker/wrangler.toml@ccb8760f365fa9234954408e20e5be98ad196f93"
  - "apps/synthetic-pager-worker/src/index.ts@ddf7951079039982780b5da15c7baed7a787b4bc"
---
# Scheduled drills

The repository contains a main-Worker scheduled handler and a separate synthetic
PagerDuty receiver, but its source does not establish an active main-Worker
trigger or deployed delivery. B-072 retired the source-only trigger until the
owner supplies receiver deployment, PagerDuty, and end-to-end evidence. The
handler and service-binding handoff remain fail-closed seams.

| Active trigger | Handoff seam | Contract |
| --- | --- | --- |
| None | `synthetic_page` | Owner-controlled non-production trigger, pending evidence. |

The scheduler maps the declared Monday cron to `synthetic_page`, constructs a
deterministic delivery id, and POSTs metadata through the service binding; an
unknown cron is rejected without retry, while absent binding or failed delivery
throws for retry `worker/src/index_common.ts:221-224`,
`worker/src/index_schedule.ts:5-19`, `worker/src/index_schedule.ts:22-69`.
The main export wires that handler as its scheduled entrypoint
`worker/src/index.ts:43-48`. An owner may reactivate a non-production trigger
only after receiver deployment, PagerDuty, and end-to-end evidence are supplied;
that change must update this contract and pass the focused B-072 verifier.

`worker/src/index.ts` is the scheduler, not the PagerDuty client. Each tick
requires the same-account `SCHEDULED_DRILL_DELIVERY` Service Binding. If that
binding is absent, throws, or returns a non-2xx response, the handler fails and
Cloudflare may retry the tick. It never logs or sends a PagerDuty routing key,
and it never treats a local fake or an absent binding as delivery.

When invoked by an owner-approved trigger, the handoff includes a deterministic
id derived from the cron and `scheduledTime`, allowing the receiving Worker to
deduplicate retries. Unknown cron values are rejected with `noRetry()`; delivery
exceptions and non-2xx responses remain retryable failures.

If an owner-approved non-production trigger is reintroduced, the synthetic-page
payload carries the runbook's non-secret PagerDuty contract:
`service=synthetic-drill`, `event_action=trigger`, native `severity=info`,
semantic `synthetic_severity=sev2_synthetic`, a four-week region selector, and
the `PAT-CORRELATION-ID-001` correlation prefix. Weeks 0–2 are immediate
handoffs. Week 3 (`boundary_handoff`) is deliberately not emitted at the
trigger time: the payload retains the actual `scheduled_at_ms`, sets
`delivery_mode=deferred`, and sets `emit_at_ms` to the following Sunday at
23:59:00 UTC. A receiver must schedule that effective timestamp and must not
send the PagerDuty event immediately. The receiver owns the actual
PagerDuty Events API call, D1 `synthetic_page_drills` record, and ack/escalation
processing described in
[`RB-SYNTHETIC-PAGE-DRILL`](../../../specs/_runbooks/RB-SYNTHETIC-PAGE-DRILL.md).

The separate receiver's tracked config enables only the Sunday deferred sweep at
the top level and in staging, while both top-level and staging vars set
`SYNTHETIC_DRILL_ENABLED = "false"`; production has an empty cron list and the
same disabled flag `apps/synthetic-pager-worker/wrangler.toml:6-19`,
`apps/synthetic-pager-worker/wrangler.toml:33-45`,
`apps/synthetic-pager-worker/wrangler.toml:47-55`. Its scheduled handler accepts
only that Sunday cron before calling the deferred-delivery runner
`apps/synthetic-pager-worker/src/index.ts:265-272`. These checked-in settings do
not establish deployed configuration or a PagerDuty event. No chaos experiment
is represented as an automated cron here.

# Citations
1. `worker/src/index_common.ts:221-224` — cron-to-drill mapping.
2. `worker/src/index_schedule.ts:5-19` — reject unknown cron and require delivery binding.
3. `worker/src/index_schedule.ts:22-69` — deterministic id, metadata-only request, and retry behavior.
4. `worker/src/index.ts:43-48` — main Worker exports the scheduled handler.
5. `apps/synthetic-pager-worker/wrangler.toml:6-19`, `33-45`, `47-55` — receiver's checked-in top-level, staging, and production settings.
6. `apps/synthetic-pager-worker/src/index.ts:265-272` — separate receiver's scheduled entrypoint.
