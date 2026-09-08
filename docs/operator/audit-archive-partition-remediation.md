# Audit archive partition incident — 2026-09-05 evidence and action packet

> **Status: OPEN — recovery is not proven.** This packet records fresh,
> read-only production evidence for B-063. It does not assert that the archive
> is healthy, that a PagerDuty incident is resolved, or that the backlog has
> drained.

## Scope and controls

The measurement follows
[`RB-AUDIT-ARCHIVE-ABSENT.md`](../../specs/_runbooks/RB-AUDIT-ARCHIVE-ABSENT.md)
§3.3 against production D1 `corelink-prod-d1` (`audit_outbox`). It uses the
per-`(tenant_id, region)` predicate; a fresh fleet-wide watermark cannot mask a
partition that never advances.

- Every production operation in this observation was a D1 HTTP `POST` carrying
  a `SELECT` only. No archive/drain endpoint, deployment, secret, or
  PagerDuty action was invoked.
- The database/account identifiers and authorization material are not recorded.
  The detector's eight-character tenant prefix is retained; the full tenant id
  is intentionally omitted.
- Each cutoff is an inlined integer Unix-millisecond value, avoiding D1 REST
  JSON-number binding ambiguity. Samples are separate reads, not an atomic
  snapshot.

The exact per-partition query was:

```sql
SELECT o.tenant_id, o.region, COUNT(*) AS pending_old,
       MIN(o.enqueued_at) AS oldest_pending_ms,
       (SELECT MAX(o2.archived_at) FROM audit_outbox o2
          WHERE o2.tenant_id = o.tenant_id AND o2.region = o.region)
         AS part_last_archived_ms
FROM audit_outbox o
WHERE o.emitted_at IS NOT NULL AND o.archived_at IS NULL
  AND o.quarantined_at IS NULL AND o.enqueued_at < <T_MS>
GROUP BY o.tenant_id, o.region
HAVING part_last_archived_ms IS NULL OR part_last_archived_ms < <T_MS>;
```

## Production observations — 2026-09-05

`T = 3h`. All three partition reads returned the same single row:
`93da3f7a/enam`, with `pending_old=188`,
`oldest_pending_ms=1787824088488`, and
`part_last_archived_ms=1787799644403`.

| Sample (UTC) | Cutoff (`T_MS`) | Result |
| --- | ---: | --- |
| `2026-09-05T22:57:46Z` | `1788638262347` | `93da3f7a/enam`, 188 rows |
| `2026-09-05T23:00:25Z` | `1788638423510` | `93da3f7a/enam`, 188 rows |
| `2026-09-05T23:00:28Z` | `1788638427320` | `93da3f7a/enam`, 188 rows |

At the first sample, the oldest pending row was approximately 226.16 hours
old and the partition watermark approximately 232.95 hours old. The fleet
context read at `2026-09-05T22:57:47Z` returned:

| Measurement | Value |
| --- | ---: |
| Sealed rows | `84918` |
| Unarchived sealed rows | `12006` |
| Quarantined rows (excluded by §3.3) | `11818` |
| Fleet `MAX(archived_at)` | `1788343203350` (`2026-09-02T10:00:03Z`) |

The fleet watermark was approximately 81.96 hours old at that sample, beyond
`T`. Therefore both absence clauses were true, and the independent
per-partition predicate was also true. This is not a healthy historical tail:
the named partition and its watermark were unchanged across the three reads.

## Detector evidence

No post-repair detector run was created. The latest three completed runs
available through the read-only GitHub Actions listing were all failures:

| Started (UTC) | Run | Result |
| --- | --- | --- |
| `2026-09-01T18:33:20Z` | [33544312348](https://github.com/HuGR-Labs/corelink-server/actions/runs/33544312348) | `failure`; `partitions_failed=1`, `93da3f7a/enam(n=184)` |
| `2026-09-01T14:55:33Z` | [33522527135](https://github.com/HuGR-Labs/corelink-server/actions/runs/33522527135) | `failure` |
| `2026-09-01T10:09:35Z` | [33495959736](https://github.com/HuGR-Labs/corelink-server/actions/runs/33495959736) | `failure` |

The Sep 1 run's summary explicitly said `verdict: PAGE` because the
per-partition failure predicate was true. Absence of newer runs is not green
evidence; this packet does not dispatch one because the workflow pages
PagerDuty on a red result.

## B-112 reconciliation

B-112 is a separate CLI release-chain repair. Its technical changes are scoped
to the release, signer, notarization, and provenance workflows and their tests;
they do not change `audit_outbox`, the archive handler, the archive cron, or the
production container image. The B-112 candidate also remains open until a
successful production release run is observed. It is not deployed evidence for
this incident and cannot be treated as the archive-drain repair.

The older B-063 text called B-112 the mechanical cause. That attribution is
unsupported by the B-112 surface and is corrected here: the archive partition's
underlying error remains to be diagnosed from authorized container/archive
observability. No production endpoint invocation is part of this packet.

## Action and closure gate

1. Keep B-063 **open**. Do not resolve PagerDuty from this repository evidence.
2. An authorized operator must diagnose and repair the archive path, then deploy
   the relevant archive/container change. B-112's release-chain work is neither
   that diagnosis nor that deployment.
3. After the repair, retain three distinct §3.3 production reads with **zero
   rows** for `93da3f7a/enam` (and no other failing partition), plus three
   completed `audit-archive-lag` runs whose summaries report
   `partitions_failed=0` and `verdict: OK`.
4. Only after both sets exist may the owner update B-063 to `done` and resolve
   the matching PagerDuty dedup key. A green global watermark alone, a green
   detector run without a zero-row population query, or a PagerDuty resolution
   alone is insufficient.
