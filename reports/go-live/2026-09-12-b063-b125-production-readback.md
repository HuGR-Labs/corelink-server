# B-063 / B-125 production readback — 2026-09-12

Status: **both remain open**. This is a versioned, redacted observation, not a
repair, deployment, workflow run, or backlog transition. Base:
`cdd6a671484a378e443bd8b4c4b7b5e0948dcb8b` (`main`).

## Method and safety

An authorized Wrangler OAuth session queried the canonical production D1
binding `corelink-config-prod` using `wrangler d1 execute
corelink-config-prod --env prod --remote --json --command <SELECT>`. Every
statement was a single aggregate or partition-population `SELECT`; no
`--file`, import, DDL, DML, drain endpoint, workflow dispatch, or PagerDuty
action was used. Every response had `success=true`, `served_by=v3-prod`,
`served_by_primary=true`, `changes=0`, `changed_db=false`, and
`rows_written=0`. The CLI response metadata reported `total_attempts=1` for
each read. No credential, payload, request ID, or full tenant ID is retained.
Only the previously documented eight-character canary tenant prefix appears.

The control `SELECT` at `2026-09-12T22:54:14Z`
(`observed_ms=1789253654000`) returned `total_rows=84979`,
`sealed_rows=84979`, `unsealed_rows=0`. A later population control returned
four regions and 367 full-tenant/region partitions. Grouping used the **full**
`tenant_id` and `region`; only the selected output truncated tenant IDs to
eight characters. This avoids the prefix-filter defect in the canned D03
B-063 command.

## B-063 — archive partition remains stuck

Three independent executions of the runbook §3.3 per-partition predicate
grouped all sealed, unarchived, non-quarantined rows by full
`(tenant_id, region)`. Each compared `enqueued_at` and the same partition's
`MAX(archived_at)` against its own D1-clock three-hour cutoff. The complete
failing-partition result, redacted at selection, was:

| Read UTC | `observed_ms` | `cutoff_ms` | Failed partitions | Canary pending old | Oldest pending ms | Partition last archived ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 22:54:58 | 1789253698000 | 1789242898000 | 1 | 188 | 1787824088488 | 1787799644403 |
| 22:55:25 | 1789253725000 | 1789242925000 | 1 | 188 | 1787824088488 | 1787799644403 |
| 22:55:41 | 1789253741000 | 1789242941000 | 1 | 188 | 1787824088488 | 1787799644403 |

The one failing partition in every response was `93da3f7a/enam`; there were
no other failing partitions. Each read reported `rows_read=255316` and the
zero-write metadata above. These are **red**, not recovery, samples. No
post-repair `audit-archive-lag` runs or PagerDuty confirmation were obtained.
The closure rule still requires three distinct **zero-failure** population
reads including this canary, plus three completed detector runs with
`partitions_failed=0` and `verdict: OK` after an authorized repair.

## B-125 — current window cannot prove throughput

The fixed six-hour window was `[2026-09-12T16:56:08Z,
2026-09-12T22:56:08Z)` (`start_ms=1789232168000`,
`end_ms=1789253768000`). A bounded `audit_outbox` aggregate returned
`arrivals=0`, `seals_completed=0`, `unsealed_all_time=0`, and
`oldest_unsealed_age_ms=0`. Counts and valid-latency min/mean/max were SQL
`NULL` where no arrival rows existed. Six separate, fixed one-hour buckets
within that window each returned exactly zero arrivals and zero completed
seals. Both aggregate responses had `changed_db=false` and
`rows_written=0` (the hourly read reported `rows_read=1019825`).

The all-time population control is non-empty. A further aggregate returned
`last_enqueued_ms=1788953790544` (`2026-09-09T11:36:30.544Z`),
`last_emitted_ms=1788955250152` (`2026-09-09T12:00:50.152Z`), and 61
arrivals in the preceding seven-day control window
`[2026-09-05T22:56:08Z, 2026-09-12T22:56:08Z)`. Thus the empty live window
is not evidence of a broken query or a healthy burst-capacity result.

No median/p90 seal latency or sustained throughput can be claimed from zero
current samples. This pass did not obtain a production deploy/configuration
pin, a burst above one 512-row call, or new replay/sequence/head-tail controls;
the 512-row source literal alone is not production proof. The earlier
[`B-125` readback](../../evidence/owner-actions/B-125/audit-throughput-readback.json)
records historical integrity anomalies, which this quiet-window measurement
does not resolve. B-125 remains open pending a non-empty, post-deploy
six-hour readback with hourly arrivals/seals, oldest-unsealed age,
median/p90 valid latency, and burst/replay/integrity controls against the
owner's agreed numeric threshold.
