---
type: "Runbook"
title: "Scheduled drills"
description: "Fail-closed synthetic PagerDuty drill handoff seam with no active trigger."
source_files:
  - "worker/src/index_common.ts"
source_blobs:
  - "worker/src/index_common.ts@23c8989d129c494b48fdafdc289f64238e58aae2"
checkpoint_sha: "a65c7d7caed03adf00acd3a227dc20c4e857f7f0"
provenance: "AUTHORED"
tags: ["ops", "scheduled-drills"]

---
# Scheduled drills

The main Worker currently declares no synthetic Cloudflare cron. B-072 retired
the source-only trigger until the owner supplies receiver deployment, PagerDuty,
and end-to-end evidence. The scheduled handler and its same-account service
binding remain a dormant, fail-closed handoff seam.

| Active trigger | Handoff seam | Contract |
| --- | --- | --- |
| None | `synthetic_page` | Owner-controlled non-production trigger, pending evidence. |

There is no replacement scheduler active in the repository or documented as
deployed. An owner may reintroduce a non-production trigger only after the
receiver binding, PagerDuty integration, and end-to-end evidence are supplied;
that change must update this contract and pass the focused B-072 verifier.

`worker/src/index.ts` is the scheduler, not the PagerDuty client. Each tick
requires the same-account `SCHEDULED_DRILL_DELIVERY` Service Binding. If that
binding is absent, throws, or returns a non-2xx response, the handler fails and
Cloudflare may retry the tick. It never logs or sends a PagerDuty routing key,
and it never treats a local fake or an absent binding as delivery.

The handoff includes a deterministic id derived from the cron and
`scheduledTime`, allowing the receiving Worker to deduplicate retries. Unknown
cron values are rejected with `noRetry()`; delivery exceptions and non-2xx
responses remain retryable failures.

If an owner-approved non-production trigger is reintroduced, the synthetic-page
payload carries the runbook's non-secret PagerDuty contract:
`service=synthetic-drill`, `event_action=trigger`, native `severity=info`,
semantic `synthetic_severity=sev2_synthetic`, a four-week region selector, and
the `PAT-CORRELATION-ID-001` correlation prefix. Weeks 0–2 are immediate
handoffs. Week 3 (`boundary_handoff`) is deliberately not emitted at the
the trigger time: the payload retains the actual `scheduled_at_ms`, sets
`delivery_mode=deferred`, and sets `emit_at_ms` to the following Sunday at
23:59:00 UTC. A receiver must schedule that effective timestamp and must not
send the PagerDuty event immediately. The receiver owns the actual
PagerDuty Events API call, D1 `synthetic_page_drills` record, and ack/escalation
processing described in
[`RB-SYNTHETIC-PAGE-DRILL`](../../../specs/_runbooks/RB-SYNTHETIC-PAGE-DRILL.md).

Production and all regional production environments explicitly override the
inherited cron list with `crons = []`. Consequently a production Worker cannot
run synthetic paging, even if an operator accidentally deploys the same script
bundle there. No chaos experiment is scheduled by this Worker; that work
requires a separately provisioned, valid environment and is intentionally not
represented as an automated cron here.

# Citations
1. `worker/src/index_common.ts:1` — declared source anchor.
