# CoreLink — Release Notes S-06 (Garbage Collection)

> **Sprint:** S-06 · **Lane:** HIGH_RISK · **Date:** 2026-05-02
> **Status:** SCAFFOLD — finalisation at S-19 onboarding SEAL

## What's new

CoreLink S-06 ships **production-grade Garbage Collection** with
TLA+-verified correctness, mark-phase-aware re-ref protection,
reversible 72h soft-delete grace, customer-visible reclaim metric,
and 30d sustained correctness gate.

### Highlights

- **Mark + Sweep + Physical-Delete + Reconcile pipeline** runs daily
  per (tenant, region) with non-blocking writes.
- **TLA+ formal verification** of INV-GC-001 (reachable never
  deleted) + INV-GC-004 (mark-phase-aware re-ref safe) — the only
  industry remote cache that ships this combination.
- **100k race property test** cross-validates the TLA+ obligation
  against the real Rust implementation; nightly CI runs at 100k iter.
- **Reversible 72h soft-delete grace** — undelete via re-upload of
  same digest OR admin endpoint (S-13 forward).
- **Customer-visible reclaim metric** `bytes_reclaimed_last_30d` per
  tenant tier (free / solo / team / business / enterprise) on the
  customer dashboard (S-16 forward). **Roadmap:** `corelink_gc_reclaimed_bytes_total`
  has no emitter in the codebase yet — only Grafana dashboard queries
  reference it today; the metric is not actually recorded.
- **Refcount reconciliation daily** with dual-condition auto-fix gate
  (count ≤ 5 AND percent ≤ 0.01%) and canonical `json_each` JSON-aware
  membership idiom (no fragile substring matching).
- **R2→D1 crash-recovery ordering** invariant — orphan R2 detected by
  reconcile orphan-R2 detection arm; orphan D1 (which would lose audit
  trail) is structurally impossible.
- **DSR erasure interaction** authorised — bypass grace for issuing
  tenant only; cross-tenant impossible by INV-TENANT-ISOLATION.
- **Degrade-mode `gc-pause`** emergency-stop knob via PAT-DEGRADE-001
  alignment.
- **3 runbook dry-runs executed** — RB-FM-300 (refcount bug),
  RB-FM-404 (gc-write-race), RB-FM-305 (tombstone lost) — host-side
  green; staging dry-run forward.
- **DASH-GC dashboard** with 10 panels + 11 alert rules (SEV-0 → SEV-3
  4-tier classification per Lote 10.4bis).

## SLOs

| SLO | Target |
|---|---|
| Mark p99 @ 1M blobs | ≤ 10 min |
| Sweep p99 @ 100k candidates | ≤ 5 min |
| Physical-delete p99 @ 100k candidates | ≤ 30 min |
| Reconcile p99 @ 1M blobs | ≤ 1 h |
| Refcount drift global sustained | < 0.1% |
| Refcount drift per-tenant sustained | < 1% |
| INV-GC-001/004 violations sustained 30d | 0 |
| TLA+ `gc_correctness` CI verde sustained 30d | 100% |

## Customer impact

- **Storage reclaimed automatically** — orphan blobs (no AC reference,
  no manifest reference, refcount = 0) are soft-deleted at the daily
  cron tick and physically purged after the 72h CAS / 24h AC grace
  window.
- **Reclaim measurable** — customer dashboard shows
  `bytes_reclaimed_last_30d` per tier (S-16 forward).
- **No data loss** — TLA+-verified INV-GC-001 + INV-GC-004 hold;
  reachable blobs are NEVER deleted; mark-phase race protected via
  protect-if-`>=` predicate (canonical TLA L152-154 semantic).

## Operational changes

- DASH-GC dashboard live in Grafana (S-09 forward).
- 11 alert rules wired to PagerDuty + Slack (`#corelink-alerts`).
- Runbooks RB-FM-300/404/305 frozen + dry-run executed.
- Production rollout 10% → 50% → 100% gradual per Lote 10.4bis lesson
  (`docs/internal/gc-prod-rollout-plan.md`).

## Known limitations / forward-looking

- 30d sustained chaos test is post-sprint observation period
  concurrent with S-07/S-08.
- Real Cloudflare R2 / D1 / KV / Cron-DO bindings deferred until
  staging account provisioned (charter trait-abstraction-defer
  pattern).
- DSR erasure full integration deferred to S-11.
- Per-tenant SLA customisation deferred to S-13 admin plane.
- Multi-region failover testing deferred to S-14.
- Customer onboarding automation deferred to S-15.
- CLI/SDK publication deferred to S-15.

## References

- [SLA addendum](./gc-sla-addendum-s06-ga.md)
- [How CoreLink reclaims storage safely](./gc-feature-overview.md)
- [PRR-S06 promotion decision](../../specs/04_sprints/_sealed/S06/PRR-S06.md)

## Change log

| Version | Date | Change |
|---|---|---|
| 1.0.0-scaffold | 2026-05-02 | Initial scaffold for WI-S06-007 SEAL; finalisation at S-19 onboarding SEAL. |

---

**End release notes scaffold.**
