# Production drain lane readback — B-063 / B-125 / B-127

Date: 2026-09-09 (UTC). Base: `79d5d55ada1c851707c6afaae8f6af777302bb7e`.
Operator: `codex-live-readback`. All Cloudflare operations used the local
Wrangler OAuth session and were remote `SELECT` statements. No credentials,
payloads, full tenant IDs, endpoint calls, workflows, migrations, or rows were
written.

## B-063 — archive partition

The runbook §3.3 query was run three times with separate inlined three-hour
cutoffs. Every read returned the same one failing partition:

| sample | captured (UTC) | cutoff ms | partition | pending old | oldest pending ms | partition watermark ms | writes |
|---:|---|---:|---|---:|---:|---:|---:|
| 1 | 01:50:26 | 1788907826000 | `93da3f7a/enam` | 188 | 1787824088488 | 1787799644403 | 0 |
| 2 | 01:50:50 | 1788907850000 | `93da3f7a/enam` | 188 | 1787824088488 | 1787799644403 | 0 |
| 3 | 01:51:17 | 1788907877000 | `93da3f7a/enam` | 188 | 1787824088488 | 1787799644403 | 0 |

The redacted owner-action artifact is
`evidence/owner-actions/B-063/audit-archive-partition-recovery.json`.
The three latest read-only workflow records remain failures with
`partitions_failed=1` (runs `33544312348`, `33522527135`, `33495959736`). No
authorized repair approval (`B063_REPAIR_APPROVED=1`) was present, so the
workflow was not dispatched and PagerDuty was not touched. B-063 remains open.

## B-125 — audit seal throughput

The owner-packet aggregate query was rerun against the same production D1:

```text
arrivals=0
sealed_rows=NULL (no rows in the six-hour window)
oldest_unsealed_age_ms=NULL
seal_latency_ms=NULL
batch_limit=512 (configuration literal only)
changes=0; rows_written=0
```

The empty current window is not capacity evidence. The existing retained
readback in `evidence/owner-actions/B-125/audit-throughput-readback.json`
already records the non-empty historical burst/replay controls and their
failures (repeated sequence groups, sequence gaps, head/tail mismatches, and
negative seal latencies). A fresh empty-window aggregate cannot satisfy the
owner threshold, so B-125 remains open; no batch limit was changed.

## B-127 — residency three-state census

The canonical `scripts/verify_audit_residency.py::RESIDENCY_SQL` was rerun.
The aggregate is exhaustive and non-empty:

| bucket/control | rows/tenants |
|---|---:|
| total denominator | 84,932 |
| satisfied | 81,262 |
| violated | 0 |
| unevaluable/orphan rows | 3,670 |
| unevaluable/orphan tenants | 175 |
| erased-orphan rows/tenants | 3,526 / 170 |
| unexplained orphan rows/tenants | 144 / 5 |
| `weur` rows / orphan rows / tenant rows | 4 / 4 / 0 |
| `dsr_erasure_log` control rows | 2,044 |

The raw redacted response is
`evidence/owner-actions/B-127/audit-residency-census.json`. The three-state
partition is reproducible, but the acceptance is not met: unevaluable rows
remain, including the five-tenant/144-row unexplained residual. No migration or
historical row mutation was attempted. B-127 remains open.

## Status decision

No item transitioned. Literal closure requires, respectively, three zero-row
partition reads plus three successful lag runs (B-063), a current non-empty
throughput/latency readback with controls (B-125), and zero unevaluable rows
with the residual explained (B-127). None of those acceptance predicates is
true in this readback.
