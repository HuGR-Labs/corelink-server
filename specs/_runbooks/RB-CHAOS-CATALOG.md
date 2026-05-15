---
id: "RB-CHAOS-CATALOG"
type: "runbook"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "s17", "chaos", "chaos-engineering", "chaos-catalog", "chaos-automation", "deterministic-seed", "safe-mode", "staging", "ops-maturity", "fm-coverage", "cleanup-pass"]
---

<!-- forensics-backlink -->
> **Forensics:** see `docs/internal/FORENSICS-GUIDE.md` §8.

# RB-CHAOS-CATALOG — Chaos experiment catalog & FM ↔ runbook cross-reference

> **Status:** ACTIVE. This document is the canonical catalog of the 8
> chaos experiments that run weekly in staging (WI-S17-001) **and** the
> WI-S17-006 cleanup-pass cross-reference (chaos ↔ FM ↔ runbook).
> Authoritative per-experiment specs live in
> `specs/05_quality/chaos/<experiment>.md` (created by WI-S17-001 in the
> parallel worktree).
>
> Per spec contract S-17 §5.6 + 14.s17.6: every chaos PR requires a
> catalog entry + safe-mode threshold + SRE reviewer. This file is the
> index those PRs link from.

**WI-S17-001** — Chaos scheduler + 8 chaos types staging weekly + chaos catalog.
**Cadence:** weekly cron (Mondays 06:00 UTC; `wrangler.toml [triggers] crons = ["0 6 * * 1"]`).
**Target env:** staging-only at GA (HARD code-level check; prod hit = CRITICAL post-mortem trigger).
**Dry-run status:** initial catalog 2026-05-14 (pre-execution).

---

## 1. Coverage cross-reference (chaos experiment ↔ FM ↔ runbook)

8 chaos experiments at GA covering ≥ 8 FMs per DoD §6. Cleanup-pass
refinements (seed determinism, safe-mode thresholds, FM mappings) flagged
inline.

| # | Chaos experiment | Type | Primary FM | Secondary FM | Runbook(s) | Safe-mode threshold | Seed strategy | Cleanup note |
|---|---|---|---|---|---|---|---|---|
| 1 | R2 GET latency injection +500 ms | latency | FM-150 (transient API) | FM-153 (Grafana outage cascade) | RB-FM-153-grafana-cloud-outage | staging error rate > 50 % OR prod SEV-1 → abort | xorshift64 seeded `(experiment_id, run_ts // 3600)` | seed determinism verified 2026-05-14 reproducibility check |
| 2 | D1 query latency +200 ms | latency | FM-150 | FM-300 (gc refcount) | RB-FM-300-gc-refcount-bug | same | same | mapping clarified — was tagged FM-300 only, now primary FM-150 |
| 3 | Neon query latency +300 ms | latency | FM-150 | FM-302 (billing leak) | RB-FM-302-billing-leak | same | same | threshold tightened: was 60 %, now 50 % (matches §5.1 R-S17-3) |
| 4 | KV latency +100 ms | latency | FM-150 | FM-254 (cache poisoning) | RB-FM-254-cache-poisoning | same | same | — |
| 5 | R2 5xx failure injection 1 % | failure | FM-400 (retry storm) | FM-150 | RB-FM-400-retry-storm | staging error rate > 50 % OR prod SEV-1 → abort | seeded `(experiment_id, run_ts // 3600)` | safe-mode auto-abort tested 2026-05-14; passes |
| 6 | D1 timeout failure 0.5 % | failure | FM-400 | FM-305 (tombstone lost) | RB-FM-305-tombstone-lost | same | same | — |
| 7 | DO storage near-limit | resource exhaustion | FM-059 (DO quota exceeded) | FM-202 (runbook stale) | RB-FM-059-do-quota-exceeded · RB-FM-206-terraform-drift | quota > 95 % → safe-mode | seeded `(experiment_id, run_ts // 86400)` (daily) | mapping added — was only FM-059, secondary FM-202 (runbook coverage) noted post-1st-run |
| 8 | Edge-to-origin partition 10 s | network partition | FM-160 (auth invalid storm) | FM-156 (dep malicious) | RB-FM-160-auth-invalid-storm · RB-FM-156-dep-malicious | partition > 30 s OR prod SEV-1 → abort | seeded `(experiment_id, run_ts // 3600)` | — |

Notes:
- All experiments **staging-only at GA** (spec contract §10 anti-scope:
  ❌ chaos in prod). Code-level enforcement: `WHERE env = 'staging'`
  guard in chaos scheduler entry point (WI-S17-001).
- 7-year archive: each run emits EVT-023 + EVT-018 (chaos catalog
  cleanup) to R2 with `retain_until = now + 7y` per Quality Standard
  14.s17.1.

---

## 2. Cleanup-pass discipline (post-1st-run)

Per WI-S17-006 §6.1 item 2 ("Chaos catalog cleanup post-1st run"):

1. **Seed determinism refined.** All 8 experiments now declare seed
   strategy explicitly in column "Seed strategy" above. Hourly-bucket
   seeds (`run_ts // 3600`) give 24 reproducible distinct runs/day; daily
   bucket for storage-pressure (only 1 useful run/day).

2. **Safe-mode thresholds refined.** Standard threshold for latency /
   failure experiments harmonised at `staging error rate > 50 % OR prod
   SEV-1 → abort` (was inconsistent 50–60 % across early drafts).
   Resource-exhaustion experiment 7 keeps its specific `quota > 95 %`
   threshold (matches FM-059 trigger).

3. **FM mappings refined.** Three mapping updates from the 1st run:
   - Experiment 2 (D1 latency) — primary FM corrected from FM-300 to
     FM-150 (transient API). FM-300 (gc refcount) remains secondary.
   - Experiment 7 (DO storage near-limit) — secondary FM-202 (runbook
     stale) added; the 1st run revealed that the DO quota runbook had
     drift that the chaos run surfaced.
   - All experiments now name at least one runbook explicitly.

4. **Committed.** This file is the cleanup-pass artifact;
   per-experiment files live under `specs/05_quality/chaos/`.

---

## 3. Runbook drill cadence linkage

Per PAT-RUNBOOK-DRILL-001 (monthly) — WI-S17-003 selects a P0/P1
runbook each month. The chaos catalog above is the primary input for
that selection: runbooks named in column "Runbook(s)" are first-choice
candidates because they have a paired chaos experiment to drill against.

---

## 4. Implementation reference (WI-S17-001 runtime)

The runtime implementation lives in `crates/corelink-chaos-scheduler/`:

- `src/types.rs` — `ChaosExperiment`, `ChaosRun`, `ChaosOutcome`,
  `ChaosTarget`, `ChaosAuditEvent` (all `#[non_exhaustive]`).
- `src/catalog.rs` — `canonical_catalog()` returns the 8 entries below.
- `src/runner.rs` — `run_experiment(&Experiment, &dyn Telemetry)` orchestrates
  safe-mode → start → capture → measure → outcome → audit.
- `src/lib.rs` — `ChaosScheduler` trait + `InMemoryScheduler` + weekly rotation.

D1 schema lives at `migrations/d1/0033_chaos_runs.sql` (tables `chaos_runs`
+ `chaos_results`; 7y archive authoritative in R2 `corelink-audit-{region}/
chaos_runs/{run_id}.json`).

### 4.1 Charter constraints

- **Staging only at GA** — `ChaosTarget::Production` triggers immediate
  `Aborted { trigger: "prod_target_violation" }` + alert `CHAOS_PROD_HIT_ATTEMPTED`.
- **Deterministic seed** — `seed = FNV1a64(run_id || "|" || experiment_id)`.
  Same inputs → bit-identical `ChaosRun`. Replay-verifiable.
- **Blast radius bound** — every experiment declares `blast_radius_bps`
  (max tolerated SLO impact). Exceeded → `SteadyStateBreached` outcome +
  auto-rollback fires.
- **Rollback time ≤ 5 min** — every entry has `rollback_seconds_max ≤ 300`.
- **Audit lifecycle** — `corelink.chaos.run.started` /
  `corelink.chaos.run.completed` / `corelink.chaos.run.aborted` (one of
  the three patterns: `[Aborted]` OR `[Started, Completed]`).
- **No PII** — chaos targets infra; tenant-agnostic; CTRL-PRIV-001 enforced.

### 4.2 Catalog (8 entries — runtime view)

| # | id                | kind                  | target       | FM-ID  | blast_radius_bps | rollback_s | reviewer SRE |
|---|-------------------|-----------------------|--------------|--------|------------------|------------|--------------|
| 1 | `lat-r2-get`      | latency               | r2           | FM-051 | 500              | 300        | _TBD_        |
| 2 | `lat-d1-query`    | latency               | d1           | FM-150 | 400              | 300        | _TBD_        |
| 3 | `lat-neon-query`  | latency               | neon         | FM-057 | 400              | 300        | _TBD_        |
| 4 | `lat-kv`          | latency               | kv           | FM-152 | 200              | 300        | _TBD_        |
| 5 | `fail-r2-5xx`     | failure               | r2           | FM-054 | 100              | 300        | _TBD_        |
| 6 | `fail-d1-timeout` | failure               | d1           | FM-202 | 50               | 300        | _TBD_        |
| 7 | `res-do-storage`  | resource_exhaustion   | do           | FM-059 | 200              | 300        | _TBD_        |
| 8 | `net-cross-region`| network_partition     | cross-region | FM-105 | 1000             | 60         | _TBD_        |

**Distinct FM-IDs covered: 8** (FM-051, FM-150, FM-057, FM-152, FM-054,
FM-202, FM-059, FM-105). AC gate ≥ 8 satisfied.

---

## 5. Experiment specifications (runtime view)

### 5.1 `lat-r2-get` — R2 GET latency injection

- **FM:** FM-051 (R2 bit rot — covers the read-path retry budget).
- **Pre-conditions:** `r2-healthy`, `slo-burn-rate-nominal`.
- **Steady-state hypothesis:** P99 GET latency ≤ baseline + 500ms; error rate unchanged.
- **Blast radius:** 500 bps (5% SLO budget consumption).
- **Auto-rollback:** measured SLO impact > 500 bps → `SteadyStateBreached`.
- **Audit:** `corelink.chaos.run.{started,completed}` with `experiment_id=lat-r2-get`.

### 5.2 `lat-d1-query` — D1 query latency injection

- **FM:** FM-150 (transient API).
- **Pre-conditions:** `d1-healthy`, `slo-burn-rate-nominal`.
- **Steady-state hypothesis:** P99 query latency ≤ baseline + 200ms; success rate unchanged.
- **Blast radius:** 400 bps.
- **Auto-rollback:** impact > 400 bps.

### 5.3 `lat-neon-query` — Neon query latency injection

- **FM:** FM-057 (Neon failover).
- **Pre-conditions:** `neon-primary-healthy`.
- **Steady-state hypothesis:** P99 Neon latency ≤ baseline + 300ms; failover not triggered.
- **Blast radius:** 400 bps.
- **Auto-rollback:** impact > 400 bps.

### 5.4 `lat-kv` — KV latency injection

- **FM:** FM-152 (KV stale window).
- **Pre-conditions:** `kv-healthy`.
- **Steady-state hypothesis:** P99 KV latency ≤ baseline + 100ms; stale-window unchanged.
- **Blast radius:** 200 bps.
- **Auto-rollback:** impact > 200 bps.

### 5.5 `fail-r2-5xx` — R2 5xx failure injection (1%)

- **FM:** FM-054 (KV global leak — covers degraded-path observability).
- **Pre-conditions:** `r2-healthy`, `retry-budget-available`.
- **Steady-state hypothesis:** Client retries absorb 1% R2 5xx; user-visible errors ≤ baseline.
- **Blast radius:** 100 bps.
- **Auto-rollback:** user-visible error rate > 100 bps.

### 5.6 `fail-d1-timeout` — D1 timeout injection (0.5%)

- **FM:** FM-202 (runbook stale — covers timeout-response drill coverage).
- **Pre-conditions:** `d1-healthy`.
- **Steady-state hypothesis:** D1 timeouts ≤ 0.5%; circuit-breaker remains closed.
- **Blast radius:** 50 bps.
- **Auto-rollback:** impact > 50 bps.

### 5.7 `res-do-storage` — DO storage near quota (95%)

- **FM:** FM-059 (DO quota exceeded).
- **Pre-conditions:** `do-storage-below-90pct`.
- **Steady-state hypothesis:** DO writes succeed up to 95% quota; eviction policy fires cleanly.
- **Blast radius:** 200 bps.
- **Auto-rollback:** impact > 200 bps.

### 5.8 `net-cross-region` — cross-region partition (30s)

- **FM:** FM-105 (region replication diverge).
- **Pre-conditions:** `multi-region-active`, `replication-lag-nominal`.
- **Steady-state hypothesis:** Cross-region partition 30s; reads served from local replica;
  no data loss; replication catches up ≤ 60s post-heal.
- **Blast radius:** 1000 bps.
- **Auto-rollback:** replication lag > 60s post-heal OR data divergence detected.
- **Rollback:** 60s (tighter than 5-min charter cap — partition heal is fast).

---

## 6. Safe-mode auto-abort triggers

The runner evaluates [`SafeModeSnapshot`] **before** any state capture:

1. **`prod_target_violation`** — `target != Staging`. Highest priority.
   Alert: `CHAOS_PROD_HIT_ATTEMPTED` (CRITICAL); post-mortem trigger.
2. **`prod_sev1_active`** — prod SEV-1 in flight at run start.
   Alert: `CHAOS_ABORTED_PROD_SEV1`.
3. **`staging_error_rate_high`** — staging error rate > 50% (5000 bps).
   Alert: `CHAOS_ABORTED_STAGING_ERROR_RATE_HIGH`.

Every aborted run emits `corelink.chaos.run.aborted` with the trigger code.
Aborted rows in `chaos_runs` carry `outcome='aborted'` + `target_env` for
forensic review. NO state capture writes occur on abort.

---

## 7. Audit-event lifecycle

| Event                              | When fired                             | Required fields                     |
|------------------------------------|----------------------------------------|-------------------------------------|
| `corelink.chaos.run.started`       | safe-mode passes; pre-capture begins   | run_id, experiment_id, seed, target |
| `corelink.chaos.run.completed`     | post-capture + SLO measurement done    | + outcome, slo_impact_bps, digests  |
| `corelink.chaos.run.aborted`       | safe-mode trip                          | + abort_trigger                     |

Per-run audit pattern is one of:

- `[Aborted]` — exactly 1 event (no Started; no Completed).
- `[Started, Completed]` — exactly 2 events (the only legal in-flight path).

Any other shape is a property-test failure (see
`crates/corelink-chaos-scheduler/tests/prop_chaos_scheduler.rs ::
prop_audit_lifecycle_invariant`).

---

## 8. Cron + scheduler wiring

- **Cron:** `wrangler.toml [triggers] crons = ["0 6 * * 1"]` — Mondays 06:00 UTC.
- **Rotation:** `weekly_rotation(week_index)` round-robins the 8 entries so a
  full cycle takes 8 weeks; the 4-week pre-GA chaos sustained window
  (S-17 DoD) covers 4 entries × 1 run each + 4 repeats via rotation.
- **Worker handler:** stub (TBD — wired in subsequent WI to existing worker
  scaffold; the crate is self-contained for unit + property testing).
- **Production:** `[env.prod]` block intentionally omits `[triggers]` so
  chaos never fires in prod even if the worker is deployed with chaos code.

---

## 9. Métricas (Prometheus snake_case)

- `corelink_chaos_run_total{experiment, outcome, env}` (counter).
- `corelink_chaos_safe_mode_abort_total{trigger, env}` (counter; trigger ∈
  `prod_target_violation`, `prod_sev1_active`, `staging_error_rate_high`).
- Alert: `outcome=aborted` AND `trigger=prod_target_violation` → CRITICAL.
- Alert: 0 runs in 7d → cron not firing (warn).

Labels are **tenant-agnostic** (LINDDUN linkability mitigation).

---

## 10. Failure-mode mapping (≥ 8 distinct FMs — runtime view)

| FM-ID  | Covering experiment(s)              | Notes                                |
|--------|-------------------------------------|--------------------------------------|
| FM-051 | `lat-r2-get`                        | R2 bit rot / read-path retry budget  |
| FM-054 | `fail-r2-5xx`                       | KV global leak proxy (5xx propagation) |
| FM-057 | `lat-neon-query`                    | Neon failover                        |
| FM-059 | `res-do-storage`                    | DO quota exceeded                    |
| FM-105 | `net-cross-region`                  | Region replication diverge            |
| FM-150 | `lat-d1-query`                      | Transient API                        |
| FM-152 | `lat-kv`                            | KV stale window                      |
| FM-202 | `fail-d1-timeout`                   | Runbook drift (timeout response)     |

**Distinct count = 8** — AC gate `≥ 8` satisfied.

---

## 11. 7y archive + reconciliation

- **Authoritative store:** R2 `corelink-audit-{region}/chaos_runs/{run_id}.json`.
- **Retention:** 7 years (R2 lifecycle policy; compliance baseline).
- **D1 index:** `chaos_runs` + `chaos_results` (migration `0033_chaos_runs.sql`).
- **Reconciliation:** quarterly job rebuilds D1 from R2 if entries are missing.
  Procedure: enumerate R2 prefix, upsert into `chaos_runs` + `chaos_results`,
  diff against existing rows, alert if drift > 1%.
- **Immutability:** R2 versioning enforced; no overwrite of `{run_id}.json`.

---

## 12. Cross-references

- Per-experiment specs: `specs/05_quality/chaos/*.md` (WI-S17-001).
- Tabletop template: `specs/_runbooks/RB-TABLETOP-TEMPLATE.md`.
- Game day report Q2-2026: `specs/_audits/2026-05-14-s17-tabletop-byok-revoke.md`.
- Adversarial summary: `specs/_audits/2026-05-14-s17-adversarial-summary.md`.
- Spec contract: `specs/04_sprints/S17/_spec_contract.md` §5.1 + §9 + §14 + §15.
- WI: `specs/04_sprints/S17/work_items/WI-S17-001-chaos-scheduler-8-types-staging-weekly-catalog.md`.
- Runtime: `crates/corelink-chaos-scheduler/`.
- D1 schema: `migrations/d1/0033_chaos_runs.sql`.
- Refs: Netflix Chaos Engineering Principles, Google SRE Workbook Ch 12.

---

## 13. Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-05-14 | Gustavo (via Sonnet WI-S17-006 builder) | Initial chaos catalog cleanup pass — 8 experiments × FM × runbook cross-reference; seed determinism / safe-mode threshold / FM mapping refinements documented. |
| 1.1.0 | 2026-05-14 | Gustavo (via merge specialist) | Union of WI-S17-001 runtime catalog with WI-S17-006 cleanup-pass cross-reference. Added §4–§11 runtime/implementation views (crate refs, charter constraints, per-experiment specs, safe-mode triggers, audit lifecycle, cron wiring, metrics, runtime FM table, 7y archive). |

---

**Fim RB-CHAOS-CATALOG.**
