---
id: "AUDIT-2026-05-03-RB-OBS-CARDINALITY-001-DRY-RUN"
type: "audit"
doc_status: "FROZEN"
audit_status: "AUDITED"
version: "1.0.0"
created: "2026-05-03"
updated: "2026-05-03"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "rb-dry-run", "rb-obs-cardinality-001", "cardinality-explosion", "observability", "cost-control", "s09", "wi-s09-007"]
---

# RB-OBS-CARDINALITY-001 dry-run — 2026-05-03 (WI-S09-007)

> **Sprint:** S-09 · **WI:** WI-S09-007 §6.1.6 · **Runbook:** [RB-OBS-CARDINALITY-001](../05_quality/runbooks/RB-OBS-CARDINALITY-001.md) · **Harness:** `scripts/rb_obs_cardinality_001_dry_run.sh`
> **Mode:** host-side (full staging dry-run with real Mimir tier
> limit secondary defense + 25k synthetic series injection + on-call
> engineer execution deferred until staging account provisioned)

---

## 0. Identification

| Field | Value |
|---|---|
| Runbook | RB-OBS-CARDINALITY-001 (Cardinality Explosion — Métrica → OOM Mimir / Cost Spike) |
| INV | INV-OBS-CARDINALITY-BUDGET (HIGH; new §3.12 — S-09 NEW group) |
| Invariants tested | INV-OBS-CARDINALITY-BUDGET (HIGH) + INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER (HIGH) + INV-TENANT-ISOLATION (CRITICAL, TLA+) |
| Injection magnitude | synthetic injection 25k unique séries via test-only metric `corelink_test_cardinality_explosion{label_a..label_z}` em staging |
| Detection target | ≤ 1h p95 via SEV-2 alert `corelink_metrics_cardinality_budget_violation_total > 0` + Mimir tenant 429 (over-quota) |
| Mitigation target | ≤ 6h p95 (drop label via Mimir relabel rule OR rollback PR; cardinality_check.py CI gate hardened) |
| Recovery target | auto-resolve on rollback (idempotent budget reset; PR revert restores ledger) |
| Pass criteria | (a) cardinality_check.py CI gate rejects offline + (b) Mimir tenant tier limit rejects ingest beyond 20k per-metric (secondary defense) + (c) SEV-2 alert fires + (d) degraded observability documented (some series dropped; not all queryable) + (e) recovery on rollback auto-resolves |
| Independent runs | ≥ 3 host-side iterations green; staging dry-run with seed variance documented forward |

## 1. Run summary

| Step | Description | Outcome | Evidence |
|---|---|---|---|
| 1 | Detection — `corelink-analytics::CardinalityValidator` emits `AnalyticsError::CardinalityBudgetExceeded { scope=PerMetric \| Global }` when per-metric ledger reaches canonical 20k unique-tuple cap | PASS | `cargo test -p corelink-analytics --test prop_analytics` 17 prop tests green @ 10k iter |
| 2 | Communication — SEV-2 page synthesis (host-side stub) | SIMULATED | Synthetic SEV-2 payload emitted; PagerDuty wiring deferred to live observability stack (S-20 GA gate forward) |
| 3 | Mitigation — per-metric + global ledger isolation under high-cartesian sweep | PASS | `cargo test -p corelink-analytics --test prop_analytics` (covered by Step 1) |
| 4 | Diagnóstico — `corelink-analytics::FORBIDDEN_LABEL_NAMES` const lint catches `trace_id` / `tenant_id` / `request_id` / `blob_digest` forbidden labels at the structural cartesian boundary | PASS | `cargo test -p corelink-analytics --lib` 81 lib tests green |
| 5 | Resolução — hot fix (drop label via Mimir relabel rule; idempotent budget reset on PR revert) | SIMULATED | Rollback path canonical |
| 6 | Post-incident — post-mortem mandatório + 5-Why + observability_model.md §11.2 update | SIMULATED | Template ready; live execution deferred until staging account provisioned |
| 7 | Evidence — forensic chain template (Mimir cardinality dashboard + Grafana billing report + PR/commit + CI cardinality_check.py logs) | SIMULATED | Forensic chain template ready |

## 2. Drift detection

PASS — every expected runbook header present (Detecção / Comunicação /
Mitigação imediata / Diagnóstico / Resolução / Post-incident /
Evidence / References). Runbook + harness MUST be updated together
on any structural change.

## 3. Findings

- **Defense-in-depth canonical**: the cardinality budget enforcement
  pattern ships defense-in-depth at TWO layers: (1) primary defense is
  the runtime `corelink-analytics::CardinalityValidator`
  per-instance `Arc<Mutex<HashMap<RedMetricKind, HashSet<MetricLabelTuple>>>>`
  ledger that fails the emit at the type system boundary BEFORE any
  external metric system call (per WI-S09-001 §6.1.13); (2) secondary
  defense is the Mimir tenant tier limit that rejects ingest beyond
  20k per-metric at the network boundary. Either layer alone catches
  cardinality explosions; both layers green is the canonical pattern.
- **Forbidden-label-names structural lint**: the `MetricLabelTuple`
  enum-typed cartesian struct has NO `String` slot; forbidden labels
  `trace_id` / `tenant_id` / `request_id` / `blob_digest` are NOT
  representable by construction. The `FORBIDDEN_LABEL_NAMES` const is
  the CI lint secondary defense per WI-S09-001 §1 invariant 2/3 + Lote
  10.8bis cardinality discipline absorbed.
- **Per-metric + global budget canonical**: per-metric ≤ 20k unique
  series; global ≤ 100k cumulative across all metrics per WI-S09-001
  §1 invariant 1 + sprint contract §5.1 R-S09-2. Both bounds enforced
  by the validator + cross-validated by `prop_cardinality_budget_enforced`
  10k iter PR / 100k iter nightly via `PROPTEST_CASES` env var
  override per S-07 P1-2 fix.
- **Idempotent repeat label set canonical**: `prop_cardinality_idempotent_repeat_label_set`
  10k iter pins the discipline that emitting the same `(metric_kind,
  label_tuple)` cartesian repeatedly does NOT inflate the unique-tuple
  count; only NEW tuples advance the ledger. This prevents legitimate
  high-volume tenants from accidentally tripping the budget.
- **Audit-emit-BEFORE-mutation envelope canonical**: every decision
  arm (Registered / AlreadyRegistered / CardinalityRejected /
  BudgetExceeded) emits the audit row BEFORE the ledger mutation;
  audit failure aborts the validator path per Lote 10.6bis pattern +
  S-07 sprint-close P1-1 fix. `prop_audit_emit_per_decision_arm` 10k
  iter pins the envelope ordering.

## 4. Deferred items

- Full staging dry-run with real Mimir tenant tier limit secondary
  defense + 25k unique series synthetic injection via test-only
  metric `corelink_test_cardinality_explosion{label_a..label_z}` +
  real SEV-2 alert + on-call engineer execution + ≥ 3 independent
  runs with seed variance documented deferred until staging account
  provisioned (sprint contract §13 post-sprint observation period).
  Revalidation trigger: staging account provisioned + S-20 GA gate.
- Real `cardinality_check.py` CI gate (PR-time static analysis of
  newly-introduced metric / label cartesian estimation against the
  canonical budget) deferred per charter `trait-abstraction-defer`
  pattern; structural inability to emit forbidden labels via the
  enum-typed `MetricLabelTuple` is the runtime primary defense.
- Real Grafana Mimir tenant API integration for per-metric series
  count introspection + auto-quarantine flapping cardinality detection;
  deferred per charter `trait-abstraction-defer` pattern.
- 30d sustained chaos test (post-sprint observation period concurrent
  with S-10/S-11 sprints per spec contract §13 timeline).

## 5. Sign-off

| Role | Status | Date |
|---|---|---|
| Owner | ✅ ACKNOWLEDGED | 2026-05-03 |
| SRE Lead | ⚠️ WAIVED (ADR-0034) | 2026-05-03 |
| Security Lead | ⚠️ WAIVED (ADR-0034) — INV-OBS-CARDINALITY-BUDGET cross-validated via prop_cardinality_budget_enforced 10k iter | 2026-05-03 |

## 6. Change log

| Version | Date | Author | Change |
|---|---|---|---|
| 1.0.0 | 2026-05-03 | Gustavo Schneiter (via Claude Opus 4.7 1M) | Initial RB-OBS-CARDINALITY-001 host-side dry-run audit trace (WI-S09-007 SEAL Lote). |

---

**End RB-OBS-CARDINALITY-001 dry-run audit v1.0.0.**
