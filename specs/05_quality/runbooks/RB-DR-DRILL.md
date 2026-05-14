---
id: "RB-DR-DRILL"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.1.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "wi-s17-002", "dr-drill", "semestral", "cf-region-outage", "rto", "rpo", "staging-only"]
---

# RB-DR-DRILL — DR Drill Semestral (CF Region Outage Cycle 1)

> **WI:** WI-S17-002 | **Crate:** `corelink-dr-drill` | **CF Cron:** `0 6 1 1,7 *` (Jan 1 + Jul 1 @ 06:00 UTC) | **SLO:** RTO ≤ 1800s, RPO ≤ 60s | **Env:** staging-only at GA (anti-prod hit guarantee)

## Contexto

DR drill semestral cadence + cycle 1 CF region outage simulate é mandatory ship gate (per WI §6 DoD). Drill executa em staging com isolated tenant; mede RTO + RPO contra ceilings canônicos; produz report archived 7y compliance.

> **Hard rule (Lote 10.17 P0):** env-check é a PRIMEIRA linha de qualquer drill execution. `require_staging` rejeita `Production` immediately. Sem env-check = bypassable = post-mortem CRITICAL trigger.

## Pre-flight checklist (T-7d antes do drill)

- [ ] SRE Lead confirmed identified em report header.
- [ ] Isolated chaos tenant provisioned (`tenant_id = isolated_chaos_tenant`).
- [ ] Staging multi-burn-rate alerts (S-09) verified emitting.
- [ ] Failover router (`corelink-failover-router`) acyclic graph asserted via integration test.
- [ ] Customer notification template prepared (drill window communicated to enterprise tenants em staging mode).
- [ ] Backup/restore patterns rehearsed (cycle 2 placeholder; cycle 1 não toca D1 primary).

## Execução (T-0 — semestral cadence cron-triggered)

### Step 1 — Env guard

```rust
use corelink_dr_drill::{require_staging, DrillEnv};
require_staging(DrillEnv::Staging)?;  // hard-fail se Production
```

Em caso de erro `ProdEnvForbidden` → **ABORT IMMEDIATELY** + post-mortem CRITICAL.

### Step 2 — Schedule drill run

```rust
let scheduler = InMemoryDrillScheduler::new();
let run = scheduler.schedule(
    format!("dr-drill-cycle-1-{}", chrono::Utc::today()),
    DrillCycle::CfRegionOutage,
    DrillEnv::Staging,
    now_ms,
    Region::Weur,   // canonical staging primary
)?;
// run.status == DrillStatus::Scheduled
```

D1: insere row em `dr_drill_runs` (status=scheduled).

### Step 3 — Pre-drill state capture

- Snapshot D1 schema state (counts per table, last-write timestamps).
- Snapshot R2 hot blob inventory (`corelink-replica-worker` aggregation 30d window).
- Snapshot multi-burn-rate alert state (error-budget remaining per SLO).
- Archive snapshot em R2 `evidence-dr-drills/cycle-1/{drill_id}/pre.json`.

### Step 4 — Inject synthetic outage

```rust
let sim = InMemoryCfRegionOutageSimulator::new();
let outcome = sim.simulate(
    DrillEnv::Staging,
    Region::Weur,
    measured_rto_seconds,    // medido em real time (production wiring)
    measured_rpo_seconds,    // = replication_lag_p99 at outage onset
)?;
```

CF Worker handler: marca `Region::Weur` como `synthetic_outage`; routes reads + writes para sibling (`Region::Sam` per `ResidencyGraph`).

Transition: `scheduler.update_status(drill_id, DrillStatus::InProgress)?`.

### Step 5 — Measure during drill window (60 min sustained)

- Watch `corelink_edge_5xx_rate{region=weur}` — deve spike e then drop após failover.
- Watch `corelink_failover_overhead_seconds{p99}` — deve ≤ 50ms (SLO from WI-S14-003).
- Watch `corelink_region_health_status{region=weur}` — deve transition Healthy → Degraded → Down.
- Measure RTO (failover engaged ts − outage_start_ts).
- Measure RPO (replication lag p99 at outage_start_ts).

### Step 6 — Clear synthetic outage

```rust
sim.clear()?;
```

CF Worker handler: remove `synthetic_outage` flag; traffic resumes na Region::Weur primary.

### Step 7 — Post-drill state capture + validate

- Snapshot post-drill state (same structure as pre).
- Diff pre vs post: writes must have completed on sibling com no data loss.
- Assert `outcome.within_slo == true`.
- Transition status: `Completed` if within SLO; `Failed` se SLO violado; `Aborted` se safety guard fired.

### Step 8 — Compose report

Report mandatory sections (per WI §6.3):

1. **Pre-drill state** — full state capture summary.
2. **Drill timeline** — events ts + actor (SRE Lead facilitator).
3. **SLO impact measured** — `slo_impact.error_budget_consumed_pct`, `error_count`, `latency_p99_ms`.
4. **Lessons learned** — positives + negatives.
5. **Runbook updates needed** — feeds FM-202 mitigation cycle (input para WI-S17-003 dry-run cadence).
6. **Retention** — 7y archive (per Quality Standard 14.s17.7).

Commit em `specs/_audits/2026-XX-XX-dr-drill-cycle-1.md`.
Archive em R2 `evidence-dr-drills/cycle-1/{drill_id}/report.json` com lifecycle policy = 7 years.

### Step 9 — Emit metrics

```text
corelink_dr_drill_total{cycle="1", outcome="completed"} += 1
corelink_dr_drill_slo_impact_seconds{cycle="1", slo="rto"} = measured_rto_seconds
corelink_dr_drill_slo_impact_seconds{cycle="1", slo="rpo"} = measured_rpo_seconds
```

> **CRITICAL:** NUNCA per-tenant labels em DR drill metrics (INV-OBS-CARDINALITY-BUDGET).

## Adversarial scenarios (per WI §15)

1. **Cycle 1 encontra unrecoverable failure** → drill em staging only + isolated tenant + report findings → fix em sprint subsequente. Não ship-block GA.
2. **Report omits lessons learned** → SRE Lead PR review reinforce + template enforce.
3. **SLO measurement instrumentation broken** (S-09 multi-burn-rate alerts fail) → pre-execute dry-run preview catches.
4. **Chaos hits prod inadvertent** (CRITICAL) → `require_staging` env guard catches FIRST; alert + abort + post-mortem CRITICAL.

## Rollback / Recovery

- `sim.clear()` removes synthetic outage markings.
- Failover router auto-resumes primary reads quando region health returns to `Healthy`.
- State restore from pre-drill capture (R2 archive) se discrepancy detected.
- RTO target ≤ 30min for rollback.

## SLO + Métricas

| Métrica | Tipo | Labels | Notas |
|---|---|---|---|
| `corelink_dr_drill_total` | counter | cycle, outcome | alert se outcome=fail |
| `corelink_dr_drill_slo_impact_seconds` | gauge | cycle, slo | review per cycle |
| `corelink_dr_drill_cadence_missed_total` | counter | cycle | alert se semestral cycle skipped |

Dashboard: `dashboards/dr-drill.json` (DASH-DR-DRILL).

## Cycles 2 / 3 (deferred annual at GA via waiver opt)

- **Cycle 2** (D1 primary loss + restore from backup): annual cadence; placeholder script `infra/dr_drills/cycle_2_d1_primary_loss.ts` shipped.
- **Cycle 3** (BYOK key compromise + crypto-erase + customer notification): annual; reuses S-14 BYOK + crypto-erase patterns.

ADR mandatory: `specs/_decisions/ADR-XXXX-dr-drill-cycles-2-3-deferred.md` com expiry next sprint review.

## Sign-off post-drill

| Role | Responsibility | Sign-off required |
|---|---|---|
| SRE Lead | Drill facilitator + RTO/RPO validation | ✅ |
| Final Approver | Report commit + ADR review | ✅ |
| Compliance Officer | 7y archive immutability + audit trail | ✅ (pending Tier-1 hire) |

## References

- WI-S17-002 — DR drill scheduler semestral + 1 cycle CF region outage.
- `crates/corelink-dr-drill` — Rust crate (scheduler trait + simulator).
- `corelink-failover-router` — `ResidencyGraph` reuse for sibling routing.
- `corelink-replica-worker` — replication lag SLO baseline for RPO computation.
- S-09 SEALED — multi-burn-rate alerts for SLO impact measurement.
- ADR-0034 — staffing solo-tier (SRE Lead role pending Tier-1 hire).

---

**Fim RB-DR-DRILL.**
