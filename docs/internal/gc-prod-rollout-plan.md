# CoreLink — GC Production Rollout Plan (S-06)

> **Sprint:** S-06 · **Lane:** HIGH_RISK · **WI:** WI-S06-007 §6.1.11 + §11
> **Date:** 2026-05-02 · **Status:** DEFAULT-OFF ARTIFACT WIRED — destructive execution remains owner-gated

This document is the production rollout plan for CoreLink's S-06
Garbage Collection surface. Per Lote 10.4bis lesson, gradual rollout
10% → 50% → 100% is mandatory; direct 100% is anti-scope.

## B-071 artifact boundary (2026-09-02)

The production container image includes the fixture self-check at
`/usr/local/bin/gc_sweep` and the real D1-backed observation binary at
`/usr/local/bin/corelink-gc-sweep-production`. Its `ENTRYPOINT` remains
`/usr/local/bin/corelink-server`: shipping either executable does not schedule
or enable collection. The production observation requires both
`GC_OBSERVATION_ONLY=true` and `GC_LIVE_DELETE=false`; absent, malformed, or
live values fail closed before a D1 request.

The scheduled `gc-sweep-dry-run` lane fixes `GC_LIVE_DELETE=false`, removes
Cloudflare credential variables before invocation, and records the fixture's
`reclaimable_count`, `reclaimable_bytes`, `deleted_count`, and `deleted_bytes`.
Its accepted report has one reclaimable 4,096-byte candidate and zero deleted
objects/bytes. This is artifact self-check evidence only. A real observation is
an explicitly tenant/region/run-scoped invocation of
`corelink-gc-sweep-production`: it reads durable D1 rows, constructs no R2
client, uses a reject-only R2 adapter, writes no D1 report row, and emits the
measured report on stdout for external evidence capture. Its JSON distinguishes
the number of rows scanned from the count and bytes positively classified as
reclaimable; it never labels reclaimable bytes as bytes across all candidates.
The unprovisioned `syd` physical region has no macro-residency mapping and is
rejected before the first D1 request instead of producing a false zero report.
The observation is hard-capped at 250 candidates: it reads at most one sentinel
row beyond that ceiling and fails instead of emitting a misleading partial
report.

The canonical multi-scope collector and evidence verifier are documented in
[`docs/operator/gc-production-observation.md`](../operator/gc-production-observation.md).
They require an immutable image digest and explicit tenant/region/run scopes,
force the observation flags independently for every invocation, remove inherited
delete confirmation and R2 credentials, validate balanced per-scope results,
and atomically assemble the owner-review package. A generated package remains
`PENDING_OWNER_REVIEW` and cannot itself authorize live deletion.

No production data may be deleted under this change. The owner must review a
real production-data observation and authorize a separately implemented first
destructive execution before any `GC_LIVE_DELETE=true` deployment or schedule
exists. The shipped production binary rejects live mode even when a confirmation
token is present.

## 0. Pre-rollout gates

All of the following MUST be green before phase 1 (10%):

- [ ] WI-S06-001..007 SEALED.
- [ ] PRR-S06 STAGING-STABLE decision.
- [ ] DASH-GC dashboard live in Grafana with all 11 alerts wired to
      PagerDuty + Slack.
- [ ] RB-FM-300 + RB-FM-404 + RB-FM-305 host-side dry-runs green +
      staging dry-run executed (≥ 3 independent runs with seed
      variance documented).
- [ ] 30d sustained TLA+ verde gate (CI history daily aggregate).
- [ ] 30d sustained chaos zero violations (post-sprint observation
      period concurrent with S-07/S-08).
- [ ] 4h-1kQPS chaos pre-merge gate green (≥ 3 runs with seed
      variance).
- [ ] Refcount drift sustained < 0.1% in 7d staging.
- [ ] Cost regression gate green for all WIs (criterion bench
      infrastructure ships at S-09).
- [ ] Customer-visible communication finalised (SLA addendum + release
      notes + safety doc; S-19 onboarding SEAL).
- [ ] Crypto SME D-14 booking confirmed (Lote 10.6bis P0-W7-1
      non-waivable).

## 1. Phase 1 — 10% canary (D+9)

**Scope:** 10% of tenant-tier shards across all 5 regions (sam / iad /
lhr / nrt / syd). The 10% is selected by tenant-id hash modulo 10,
NOT by tier — to stress-test the per-tier reclaim distribution.

**Gates to advance to 50%:**
- 24h SEV-0 free.
- 24h SEV-1 free OR documented + bounded.
- INV-GC-001 + INV-GC-004 violations counter = 0 sustained.
- DASH-GC reclaim metric > 0 (canonical S-06 DoD gate).
- Refcount drift < 0.1% global / < 1% per-tenant sustained.

**Rollback trigger:** any SEV-0 fires; degrade-mode `gc-pause` global
stop activated; canary tenants reverted to pre-S-06 cache behaviour.

## 2. Phase 2 — 50% (D+11)

**Scope:** 50% of tenant-tier shards (tenant-id hash mod 2).

**Gates to advance to 100%:**
- 48h SEV-0 free.
- 48h SEV-1 free OR documented + bounded.
- INV-GC-001 + INV-GC-004 violations sustained 0.
- Reconcile auto-fix gate fires correctly at boundary inputs (count
  = 5 / count = 6 / percent = 0.0001 / percent = 0.000_101).
- DASH-GC alert noise tuned (no false-positive SEV-2/SEV-3 spam).

**Rollback trigger:** same as phase 1.

## 3. Phase 3 — 100% (D+13)

**Scope:** all tenant shards.

**Gates to declare GA:**
- 7d SEV-0 free.
- INV-GC-001 + INV-GC-004 violations sustained 0.
- Customer-visible `bytes_reclaimed_last_30d` metric populating across
  all tiers.
- TLA+ CI 30d sustained verde gauge holds (DASH-GC panel 9).
- Refcount drift sustained < 0.1% global / < 1% per-tenant.

## 4. Rollback procedure

| Phase | Rollback action |
|---|---|
| 100% | Activate degrade-mode `gc-pause` global; revert wrangler version; expected recovery ≤ 5 min. |
| 50% | Same as 100%, scoped to the 50% canary cohort. |
| 10% | Same as 50%, scoped to the 10% canary cohort. |

The degrade-mode flag is a config-singleton DO read by the worker
pre-spawn; activating it stops new spawn but does NOT abort
in-flight runs (those finalise to `Aborted` audit). This is by
design — a sudden mid-run kill could leave `gc_run` rows in
inconsistent states.

## 5. Post-rollout review

- D+20 post-ship review (sprint contract §13 timeline).
- D+30 post-sprint 30d sustained validation gate (parallel observation
  period).
- Final retro covering any SEV-0/SEV-1 incidents during rollout.
- Adversarial review delta — any new attack surface discovered
  in production traffic.

## 6. Communication plan

| Phase | Audience | Channel | Content |
|---|---|---|---|
| Pre-phase 1 | Internal | Slack #corelink-eng | DASH-GC live; canary kickoff; alert rules wired |
| Phase 1 → 2 | Internal | Slack + email | Canary metrics; gate decision rationale |
| Phase 2 → 3 | Internal | Slack + email | 50% metrics; gate decision rationale |
| Phase 3 → GA | Customer | email + dashboard | Release notes; SLA addendum effective; safety doc |

## 7. References

- [PRR-S06 promotion decision](../../specs/04_sprints/_sealed/S06/PRR-S06.md)
- [SLA addendum](../customer/gc-sla-addendum-s06-ga.md)
- [Release notes S-06](../customer/release-notes-s06.md)
- [How CoreLink reclaims storage safely](../customer/gc-feature-overview.md)
- [DASH-GC dashboard](../../dashboards/grafana/DASH-GC.json)
- [DASH-GC alerts](../../dashboards/alerts/dash-gc-alerts.yml)

## 8. Change log

| Version | Date | Author | Change |
|---|---|---|---|
| 1.0.0 | 2026-05-02 | Gustavo Schneiter (via Claude Opus 4.7 1M) | Initial production rollout plan scaffold for WI-S06-007 SEAL Lote. Execution post-S-20 GA gate. |

---

**End rollout plan.**
