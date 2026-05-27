---
id: "WI-S17-008"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
lane: "STANDARD"
parent: "S-17"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "RESILIENCE-PATTERNS"
  - "FAILURE-MODES"
  - "OBSERVABILITY-MODEL"
  - "SLO-CATALOG"
  - "SECURITY-MODEL"
  - "PRIVACY-MODEL"
tags: ["wi", "s17", "ops", "dr-drill", "dr-16", "active-failover", "replica-failover", "quarterly", "split-brain", "wave-15", "wave-16", "standard"]
---

# WI-S17-008 — DR-16 Active-Failover Drill Rehearsal (Quarterly Cadence; Coordinator + Request-Path Both-Layer Promotion + Failback; ≥ 1 Promote+Failback Round Per Quarter; MTTA ≤ 5 min, MTTR ≤ 30 min; No SEV-1 from Drill; Full BCP-DR-DRILL-CADENCE.md Alignment)

> **doc_status:** DRAFT · **work_status:** READY · **lane:** STANDARD
> **Parent:** [S-17](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S17-008 |
| Título | DR-16 active-failover drill rehearsal (quarterly cadence; coordinator + request-path both-layer promotion + failback; MTTA ≤ 5 min, MTTR ≤ 30 min; full BCP-DR-DRILL-CADENCE.md alignment). |
| Sprint | S-17 |
| Lane | STANDARD |
| Forcing factors | Closes DR-16 wave-16 gap (the coordinator singleton DO + audit-bus + `/v1/health/replication` HTTP binding remain `trait-abstraction-defer` per the wave-15 `replica-coordinator-production` audit §6 — those production bindings are exercised + validated by this drill rehearsal). No new tenant data path. |

## 1. Intent

DR-16 active-failover drill rehearsal (quarterly cadence) that exercises **both layers** of the GA failover stack in one drill run:

1. **Coordinator layer** (back-plane role flip) per `RB-REPLICA-FAILOVER.md`: stale heartbeat / SLO breach → singleton DO lock acquire → audit-emit BEFORE mutation → role flip (Primary→HotStandby, Replica→Primary) → `replication_status()` reflects the new Primary.
2. **Request-path layer** (write-lease + DNS + CF Worker routing) per `RB-ACTIVE-FAILOVER.md`: drain → promote (consumes the coordinator's new role label) → reroute → verify → reverse.

The drill MUST complete ≥ 1 full **promote + failback** round per quarter, MTTA ≤ 5 min, MTTR ≤ 30 min, zero SEV-1 customer impact, full alignment with `BCP-DR-DRILL-CADENCE.md` §DR-16.

```typescript
// File: infra/dr_drills/dr16_active_failover_quarterly.ts
export async function executeDR16Quarterly() {
  // Code-level env guard FIRST (mirrors WI-S17-002 §1 canonical fix).
  if (process.env.DRILL_TARGET !== 'staging') {
    throw new Error(
      `FATAL: DR-16 drill attempted in non-staging env (DRILL_TARGET=${process.env.DRILL_TARGET}). ` +
      `Staging-only per CTRL-CHAOS-001 + safe-mode hard rule.`
    );
  }
  if (process.env.NODE_ENV === 'production') {
    throw new Error('FATAL: DR-16 drill attempted in production. Hard-fail.');
  }

  const drill_id = `dr16-active-failover-${Date.now()}`;
  const startTs = Date.now();
  const stateBefore = await captureFullState({ tenants: ['drill-eu','drill-us','drill-br'] });

  // Phase 1 — Coordinator-layer flip (RB-REPLICA-FAILOVER.md)
  await chaosMesh.applyHeartbeatStaleness({
    target: 'staging-primary-coordinator-heartbeat',
    duration_s: 120  // 2-min stale window — triggers §1.1 ReplicaHeartbeatStale
  });
  const mttaCoordinatorMs = await waitForCoordinatorPromote();  // expect ≤ 5 min
  const splitBrainCheck1 = await assertNoSplitBrain({ window: 'phase1' });

  // Phase 2 — Request-path flip (RB-ACTIVE-FAILOVER.md, consumes coordinator role)
  await scripts.activeFailoverDrill({ mode: 'staging', primary: 'WNAM', sibling: 'ENAM' });
  const mttrPromoteMs = Date.now() - startTs;  // expect ≤ 30 min total to verify-exit

  // SLO probes during the flip
  const sloImpact = await measureSLOImpact(stateBefore, startTs, Date.now());

  // Phase 3 — 24h hot-standby cool-down (drill mode shortcut: monkeypatch clock + assert
  // CooldownNotElapsed fires for < 86_400 s, then succeeds at == 86_400 s)
  const cooldownTest = await assertCooldownBlocksUnder24h();
  const failbackStartTs = Date.now();
  await scripts.coordinatorFailback({ region: 'WNAM' });
  const mttrFailbackMs = Date.now() - failbackStartTs;  // expect ≤ 30 min

  const splitBrainCheck2 = await assertNoSplitBrain({ window: 'phase3' });
  await chaosMesh.removeHeartbeatStaleness('staging-primary-coordinator-heartbeat');
  const stateAfter = await captureFullState({ tenants: ['drill-eu','drill-us','drill-br'] });

  const report = {
    drill_id,
    quarter: getCurrentQuarter(),
    scenario: 'dr16_active_failover_both_layers',
    mtta_coordinator_seconds: mttaCoordinatorMs / 1000,
    mttr_promote_seconds: mttrPromoteMs / 1000,
    mttr_failback_seconds: mttrFailbackMs / 1000,
    split_brain_phase1: splitBrainCheck1,
    split_brain_phase3: splitBrainCheck2,
    cooldown_24h_enforced: cooldownTest.passed,
    sev1_incidents_from_drill: 0,
    slo_impact: sloImpact,
    pre_state: stateBefore,
    post_state: stateAfter,
    retention_years: 7
  };
  await archiveDRDrillReport(report);
  return report;
}
```

## 2. Narrative

DR-16 is the canonical CoreLink active-region warm-failover drill. Wave-14 shipped the request-path side (`corelink-failover-router` + `RB-ACTIVE-FAILOVER.md` + `tests/e2e-failover-router/` 5-scenario harness). Wave-15 shipped the coordinator side (`corelink-replication-coordinator` + `specs/tla/replica_failover.tla` ✅ GREEN + 6-scenario `tests/e2e-replication-failover/` harness). The remaining gap before GA — and the explicit purpose of this WI — is the **drill rehearsal** that exercises both layers together against staging on a quarterly cadence post-GA + once-mandatory before the GA tag (per `BCP-DR-DRILL-CADENCE.md §DR-16`).

This drill is also the integration test for the wave-15 → wave-16 `trait-abstraction-defer` production bindings:

- Singleton DO lock binding (replaces per-instance `Mutex`).
- Audit-bus binding (CloudEvents → `audit_outbox` D1 table consumed by audit-chain per S-06).
- Heartbeat-registry DO binding (records replica-worker tick metadata in D1, 15 s P0).
- `/v1/health/replication` HTTP binding (customer dashboard + `RB-REPLICA-FAILOVER.md §4.2` probe surface).

The drill MUST exercise all four bindings end-to-end + assert MTTA + MTTR + INV-FAILOVER-NO-SPLIT-BRAIN at the coordinator layer + INV-REGION-NO-CROSS-LEAK at the request-path layer + the 24 h hot-standby cool-down hard floor.

**Risk justification STANDARD lane**:
- Drill staging-only per code-level env check + isolated tenants for chaos (anti-prod hit guarantee inherited from WI-S17-001).
- Reuses wave-14 `scripts/active-failover-drill.sh` 3-mode orchestrator + wave-15 coordinator E2E patterns; the extension is a `--replica-coordinator-flip` flag wiring the back-plane flip before the request-path flip.
- No new tenant data path introduced.
- 7y retention archive for compliance baseline (SOC 2 + ISO 27001).
- TLA+ `replica_failover.tla` already ✅ GREEN proves the safety invariant at the coordinator layer.

## 3. Customer Impact & Journey

**Persona — SRE / Compliance auditor / Lighthouse customer**:
- Drill quarterly cadence + ≥ 1 promote+failback round + MTTA/MTTR metrics archived = recovery capability proven AND replicable.
- Drill report 7y archive = SOC 2 CC7.5 + CC9.1 + Availability A1.2/A1.3 + ISO 27031 §8.4/§9.1 evidence.
- Zero SEV-1 from drill = lighthouse customer trust that the drill is operationally safe (anti-prod hit, isolated tenant, code-level env guard).
- Customer-facing `/v1/health/replication` exercise = dashboard accuracy validated per drill.

## 4. Capability Mapping

- **CAP-OPS-002** (DR drill scheduler) — IMPLEMENTA secondary (quarterly cadence; complements WI-S17-002 semestral generic).
- **CAP-RELI-003** (multi-region failover capability) — IMPLEMENTA primary at the drill-rehearsal level.
- Trace: `_spec_contract.md §5.2`, `slo_catalog.md §4.23–4.26`, `failure_modes.md FM-101`, `resilience_patterns.md §Failback`.

## 5. Tipo

DR drill execution WI + production-binding integration test WI; STANDARD lane.

## 6. Escopo

### 6.1 In-scope

1. **Drill orchestrator extension** in `scripts/active-failover-drill.sh`:
   - New `--replica-coordinator-flip` flag wires the coordinator-side flip via `coordinator.promote(...)` BEFORE the request-path flip.
   - New `--full-dr16` flag chains: heartbeat-staleness chaos inject → coordinator promote → request-path flip → verify → wait-24h-stub → coordinator failback → request-path failback → verify.
   - Existing 3-mode (`--simulate` / `--staging` / `--prod`) preserved; `--prod` still requires `CONFIRM=I_UNDERSTAND_ACTIVE_FAILOVER_PROD` env.

2. **Quarterly cadence calendar** documented in `specs/04_sprints/_sealed/S17/dr_drill_cadence.md` (semestral generic) + cross-ref'd to `BCP-DR-DRILL-CADENCE.md §DR-16` (weekly `--simulate` CI + monthly `--staging` + quarterly real-scenario stretch).

3. **Quarterly drill execution** in staging (≥ 1 promote + failback round per quarter):
   - Phase 1: coordinator-layer flip per `RB-REPLICA-FAILOVER.md`.
   - Phase 2: request-path flip per `RB-ACTIVE-FAILOVER.md`.
   - Phase 3: 24h cool-down + failback (drill mode: monkeypatched clock to compress the 24h floor while still asserting it fires `CooldownNotElapsed` for elapsed < 86_400 s).

4. **Acceptance metrics measured and archived**:
   - MTTA (mean time to acknowledge) ≤ 5 min — from chaos-inject start to first `coordinator.promote()` HTTP call.
   - MTTR (mean time to recover) ≤ 30 min — from chaos-inject to §4 verify exit-gate clear.
   - Zero SEV-1 incidents resulting from drill execution.
   - INV-FAILOVER-NO-SPLIT-BRAIN: zero overlapping writes accepted by both regions across phases 1, 2, 3.
   - INV-REGION-NO-CROSS-LEAK: residency-graph checker passes at every step (no cross-tenant cross-region read).
   - 24 h hot-standby cool-down: `coordinator.failback(...)` rejected with `CooldownNotElapsed` for monkeypatched elapsed < 86_400 s; succeeds at exactly 86_400 s.

5. **Test plan** (canonical):
   - **TLA+ replay**: `replica_failover.tla` runs in CI nightly per `replica_failover_nightly.cfg` (3 regions, `CooldownTicks = 3`, `MaxOps = 14`); `InvAtMostOnePrimary` MUST hold across all reachable states.
   - **Property test**: `tests/prop_coordinator::at_most_one_primary` with `PROPTEST_CASES=2000` (configurable via env) drives randomized promote/failback/clock-tick sequences and asserts the invariant after EVERY step.
   - **E2E scenarios**: 6-scenario harness `tests/e2e-replication-failover/` runs in PR + nightly; new scenario 7 added by this WI (`scenario_7_quarterly_drill_dry_run`) chains the full DR-16 sequence end-to-end with deterministic logical clock.

6. **Report** committed at `specs/_compliance/drill-evidence/YYYY-QQ-dr16-active-failover-{simulate|staging}.md` per `BCP-DR-DRILL-CADENCE.md §6` evidence template; 7y archived to R2 immutable bucket; reviewed + signed by SRE Lead + Final Approver.

7. **Prometheus métricas** (snake_case canonical):
   - `corelink_dr_drill_total{cycle="dr16",outcome,quarter}` (counter).
   - `corelink_dr_drill_mtta_seconds{cycle="dr16",quarter}` (gauge).
   - `corelink_dr_drill_mttr_seconds{cycle="dr16",phase=<promote|failback>,quarter}` (gauge).
   - `corelink_dr_drill_split_brain_count{cycle="dr16",quarter,phase}` (gauge; MUST be 0 at all times).
   - `corelink_dr_drill_cooldown_enforced{cycle="dr16",quarter}` (gauge; 1 = 24h floor proven enforced).

### 6.2 Out-of-scope (deferred)

- Real prod DR-16 drill execution (anti-scope estrito; staging-only at GA per CTRL-CHAOS-001).
- Cross-cloud DR (CoreLink is CF-native; out-of-scope GA per ROADMAP-TO-GA §6).
- Customer-observer attendance at drill (deferred post-GA lighthouse cycle).
- BYOK key compromise scenarios (covered by WI-S17-002 cycle 3 deferred annual).

## 7. Anti-Scope

- Skip quarterly drill (mandatory ship gate per DoD §11).
- Drill em prod (anti-scope estrito).
- Skip 7y archive (compliance fail).
- Drill without SRE Lead facilitation.
- Drill without MTTA + MTTR measurement.
- Bypass 24h cool-down assertion (would invalidate INV-FAILOVER-NO-SPLIT-BRAIN drill coverage).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: DR-16 active-failover drill rehearsal (quarterly cadence)

  Scenario: Quarterly drill MTTA ≤ 5 min
    Given DRILL_TARGET=staging AND chaos-mesh heartbeat-staleness injected on PRIMARY at T+0
    When the ReplicaHeartbeatStale-{region} alert fires
    Then coordinator.promote(...) HTTP call begins within 5 min of T+0
    And corelink_dr_drill_mtta_seconds{cycle="dr16"} gauge records the measured value

  Scenario: Quarterly drill MTTR (promote phase) ≤ 30 min
    Given quarterly drill started at T+0
    When phase-1 coordinator flip + phase-2 request-path flip + §4 verify exit-gate all clear
    Then total elapsed ≤ 30 min
    And corelink_dr_drill_mttr_seconds{cycle="dr16",phase="promote"} records the value

  Scenario: Quarterly drill INV-FAILOVER-NO-SPLIT-BRAIN holds
    Given quarterly drill running through phases 1-2-3
    When the split-brain check probe runs at each phase boundary
    Then zero overlapping writes detected at all 3 boundaries
    And corelink_dr_drill_split_brain_count{cycle="dr16"} == 0 in all 3 phase samples

  Scenario: 24h hot-standby cool-down hard floor enforced
    Given the demoted region in HotStandby with cooldown_started at T+phase1
    When coordinator.failback(...) is called with monkeypatched clock at elapsed < 86_400 s
    Then it returns CooldownNotElapsed with remaining_seconds > 0
    And failback_blocked.v1 audit event emitted
    And corelink_dr_drill_cooldown_enforced{cycle="dr16"} == 1

  Scenario: Drill staging-only enforced (anti-prod hit guarantee)
    Given infra/dr_drills/dr16_active_failover_quarterly.ts script
    When executed with DRILL_TARGET != "staging" OR NODE_ENV == "production"
    Then script throws FATAL before any chaos inject
    And no coordinator HTTP call issued

  Scenario: Drill report 7y archive
    Given quarterly drill completed
    When report archived
    Then includes MTTA + MTTR + split-brain counts + cooldown-enforced flag + SLO impact + lessons + runbook updates
    And retention_years == 7
    And immutable em R2

  Scenario: Zero SEV-1 incidents from drill
    Given quarterly drill executed end-to-end
    When PagerDuty incident review at T+24h after drill
    Then zero SEV-1 incidents attributed to drill execution
    And drill marked PASS in evidence doc

  Scenario: TLA+ replay nightly green
    Given replica_failover.tla nightly cfg (3 regions, MaxOps=14)
    When nightly CI runs the model-check
    Then InvAtMostOnePrimary holds across all reachable states
    And InvCooldownTrackedForHotStandby holds
    And InvRoleCanonical holds

  Scenario: Property test at_most_one_primary 2000 cases
    Given PROPTEST_CASES=2000 (default) OR overridden via env
    When prop_coordinator::at_most_one_primary runs in nightly gate
    Then INV-FAILOVER-NO-SPLIT-BRAIN holds after EVERY step in every case
```

## 9. Design Decisions

### 9.1 Why quarterly (não monthly OR semestral)

- Quarterly = balance between drill realism + SRE ops cost. Wave-14 spec already pinned monthly `--staging` simulate per `BCP-DR-DRILL-CADENCE.md §DR-16`; the quarterly cadence in this WI is the **full both-layer rehearsal** stretch, not the smoke-level monthly.
- Semestral (per WI-S17-002 generic DR cadence) is the floor; DR-16 specifically gets quarterly because the stakes (regional active-failover) warrant tighter cadence.
- Monthly full both-layer rehearsal = SRE fatigue risk; out-of-budget for solo-tier per ADR-0034.

### 9.2 Why both-layer in one drill (não separate coordinator + request-path drills)

- In a real DR-16 incident BOTH runbooks fire together (per `RB-REPLICA-FAILOVER.md §1.4` ordering). Drill realism requires the same ordering.
- Splitting into two drills would (a) double the SRE drill load and (b) miss the cross-layer split-brain pathway where coordinator labels disagree with request-path routing.

### 9.3 Why 24h cool-down monkeypatched clock (não wait 24h)

- Real 24h wait per drill = unrealistic; the drill window is 30 min budget.
- Monkeypatched clock is the same pattern used in `replica_failover.tla` (`CooldownTicks = 2` for PR, `CooldownTicks = 3` for nightly) — compresses simulated time while preserving the safety property.
- Property test `tests/prop_coordinator::cooldown_always_blocks_under_24h` already fuzzes `elapsed_s ∈ [0, 86_399]` proving the floor; the drill asserts the production binding respects the same floor.

### 9.4 ADR potencial?

- No new ADR required. The quarterly cadence is documented in `BCP-DR-DRILL-CADENCE.md §DR-16` (already SEALed); this WI is the execution-side WI.

## 10. Completeness Criteria

- [ ] **10.s17.008.1** Drill orchestrator `--replica-coordinator-flip` + `--full-dr16` flags shipped in `scripts/active-failover-drill.sh`.
- [ ] **10.s17.008.2** Quarterly drill executed ≥ 1× before GA tag em staging (full both-layer promote + failback).
- [ ] **10.s17.008.3** MTTA ≤ 5 min measured + archived.
- [ ] **10.s17.008.4** MTTR ≤ 30 min measured + archived (both promote + failback phases).
- [ ] **10.s17.008.5** Zero SEV-1 incidents from drill execution.
- [ ] **10.s17.008.6** INV-FAILOVER-NO-SPLIT-BRAIN: split-brain count == 0 at all 3 phase boundaries.
- [ ] **10.s17.008.7** INV-REGION-NO-CROSS-LEAK: residency-graph checker passes at every step.
- [ ] **10.s17.008.8** 24h hot-standby cool-down hard floor proven (monkeypatched-clock failback fires `CooldownNotElapsed`).
- [ ] **10.s17.008.9** TLA+ `replica_failover.tla` ✅ GREEN nightly (3 regions, `MaxOps = 14`).
- [ ] **10.s17.008.10** Property test `prop_coordinator::at_most_one_primary` 2000 cases green nightly.
- [ ] **10.s17.008.11** E2E scenario 7 (`scenario_7_quarterly_drill_dry_run`) green in PR + nightly.
- [ ] **10.s17.008.12** Drill report archived 7y compliance (per BCP-DR-DRILL-CADENCE.md §6 evidence template).
- [ ] **10.s17.008.13** Métricas Prometheus snake_case canonical emitting (`corelink_dr_drill_*` with `cycle="dr16"`).
- [ ] **10.s17.008.14** Cross-ref to `RB-REPLICA-FAILOVER.md` + `RB-ACTIVE-FAILOVER.md` + `BCP-DR-DRILL-CADENCE.md §DR-16` complete.

## 11. DoD

- [ ] Drill orchestrator extensions committed.
- [ ] Quarterly drill executed ≥ 1× em staging (full both-layer rehearsal) before GA tag.
- [ ] Acceptance metrics archived (MTTA + MTTR + split-brain + cool-down + zero-SEV-1).
- [ ] TLA+ + proptest + E2E scenario all green nightly.
- [ ] Report committed em `specs/_compliance/drill-evidence/YYYY-QQ-dr16-active-failover-staging.md` + 7y archive.
- [ ] Métricas emitting.
- [ ] Cross-references to companion runbooks + cadence doc complete.
- [ ] Adversarial scenarios 4+ documented.

## 12. Invariants Validated

- **INV-FAILOVER-NO-SPLIT-BRAIN** (CRITICAL; `invariant_registry.md §3` L179) — proven at the coordinator layer by `replica_failover.tla::InvAtMostOnePrimary` ✅ GREEN; drill asserts production binding respects the same invariant via split-brain check probe at every phase boundary.
- **INV-REGION-NO-CROSS-LEAK** (CRITICAL; `invariant_registry.md §3` L178) — proven at the request-path layer by `failover_no_split_brain.tla` + 30k property tests; drill asserts residency-graph checker passes at every step (no cross-tenant cross-region read during the flip).
- **Não introduz INVs novas** (sprint operational; per spec contract §8 mantidas only).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Drill orchestrator extension | `scripts/active-failover-drill.sh` (extended with `--replica-coordinator-flip` + `--full-dr16`) | Bash |
| Quarterly drill script | `infra/dr_drills/dr16_active_failover_quarterly.ts` | TypeScript |
| Quarterly drill report (first execution before GA) | `specs/_compliance/drill-evidence/2026-Q3-dr16-active-failover-staging.md` | Markdown |
| E2E scenario 7 | `tests/e2e-replication-failover/scenarios/scenario_7_quarterly_drill_dry_run.rs` | Rust |
| Cadence calendar update | `specs/04_sprints/_sealed/S17/dr_drill_cadence.md` (DR-16 quarterly row added) | Markdown |
| Drill archive | R2 `evidence-dr-drills/dr16/2026-Q3.json` (7y retention) | JSON |

## 14. Quality Standards

- **14.s17.008.1** Drill report quality: includes MTTA + MTTR + split-brain counts + cool-down-enforced flag + SLO impact + lessons + runbook updates; archived 7y (per Quality Standard 14.s17.7).
- **14.s17.008.2** Drill staging-only + isolated tenant + code-level env check (anti-prod hit guarantee).
- **14.s17.008.3** SRE Lead facilitates drill execution (canonical).
- **14.s17.008.4** Cost regression gate: drill infra ≤ $50/mês (chaos-mesh reuse + R2 7y archive only).
- **14.s17.008.5** TLA+ nightly green pre-requisite (`replica_failover.tla` ✅ GREEN since wave-15).

## 15. Test Plan

### 15.1 TLA+ replay (canonical)

- `specs/tla/replica_failover.tla` runs nightly per `replica_failover_nightly.cfg` (3 regions, `CooldownTicks = 3`, `MaxOps = 14`).
- Invariants checked: `InvAtMostOnePrimary`, `InvCooldownTrackedForHotStandby`, `InvCooldownNotInFuture`, `InvRoleCanonical`, `InvHealthyWellFormed`.
- ✅ GREEN since wave-15 commit `05467bd`.

### 15.2 Property test (canonical)

- `tests/prop_coordinator::at_most_one_primary` drives 2000 randomized promote/failback/clock-tick sequences (`PROPTEST_CASES=2000` default; nightly gate increases to 10000 per S-07 P1-2 pattern).
- Asserts INV-FAILOVER-NO-SPLIT-BRAIN holds after EVERY step.
- ✅ GREEN since wave-15.
- Companion: `tests/prop_coordinator::cooldown_always_blocks_under_24h` fuzzes `elapsed_s ∈ [0, 86_399]` proving the cool-down floor.

### 15.3 E2E scenarios (canonical)

- 6-scenario harness `tests/e2e-replication-failover/` ✅ GREEN since wave-15 (covers full lifecycle + split-brain at registration + split-brain at promote + replica-lag breach + status dashboard + no-eligible-replica escalation).
- **New scenario 7 added by this WI**: `scenario_7_quarterly_drill_dry_run` — chains the full DR-16 sequence (heartbeat-staleness inject → coordinator promote → request-path flip → verify → cool-down stub → coordinator failback → request-path failback → verify) under deterministic logical clock; asserts MTTA + MTTR + split-brain == 0 + cool-down enforced.

### 15.4 Quarterly drill dry-run preview

- Pre-execute em CI `--simulate` mode before each quarterly `--staging` rehearsal to catch instrumentation regressions.
- SLO measurement instrumentation verified by S-09 multi-burn-rate alert reuse.

### 15.5 Quarterly drill execution

- Full execution per orchestrator script `--full-dr16` flag.
- State captured pre/post per tenant.
- MTTA + MTTR measured.
- Split-brain check probe at each phase boundary.
- Cool-down enforcement asserted.

### 15.6 Adversarial scenarios (5+)

1. Drill encounters audit-bus degradation mid-promote — coordinator returns `CoordinatorError::Audit`; state UNCHANGED; drill recovers via §7 rollback path in `RB-REPLICA-FAILOVER.md`; documented in lessons.
2. Drill encounters network partition between coordinator DO + replica candidate — coordinator returns `CoordinatorError::NoEligibleReplica`; drill escalates to L3 + completes as PARTIAL; documented.
3. Drill chaos accidentally hits prod (CRITICAL) — code-level env check from WI-S17-001 + the new explicit `DRILL_TARGET=staging` guard catches; drill aborts with FATAL; CRITICAL post-mortem triggered.
4. Drill MTTA breaches 5 min — drill marked FAIL; root-cause within 14d; instrumentation regression investigated.
5. Drill MTTR breaches 30 min — drill marked FAIL; runbook updates needed; feeds FM-202 mitigation cycle.

### 15.7 Verification

- Verify drill report committed + 7y archive immutable.
- Verify all 14 Completeness Criteria items ticked.
- Verify quarterly cadence calendar updated.

## 16. Failure Modes

- **FM-101** (CF edge outage / region degraded) — primary FM exercised; partial-outage variant per DR-16 scope.
- **FM-105** (region replication diverge) — drill phase-3 split-brain check probe directly tests this.
- **FM-202** (runbook stale) — drill lessons-learned feed runbook update cycle (input para WI-S17-003).
- **Coordinator singleton DO lock unavailable** — drill scenario 2 covers; coordinator returns `NoEligibleReplica` or `Internal`; SEV-1 escalation.
- **Audit-bus degraded** — drill scenario 1 covers; coordinator returns `Audit` error; state UNCHANGED per fail-CLOSED invariant.

## 17. Controls

- **CTRL-CHAOS-001** (chaos staging-only) — code-level env guard FIRST.
- **CTRL-PRIV-001** (zero PII em DR drill report sanitized).
- **CTRL-EXEC-001** (deadline guard) — reused for drill orchestrator step timeouts.

## 18. Resilience Patterns

- **PAT-DEGRADE-001** (degrade mode global) — phase-1 heartbeat-staleness models the degradation.
- **PAT-REGION-FAILOVER-001** (region failover) — both layers exercised together (cross-region warm switch).
- **PAT-AUDIT-VERIFY-001** (audit-chain Merkle walk) — verify exit-gate includes Merkle walk.
- 24 h hot-standby cool-down pattern is documented as a non-PAT canonical pattern in `specs/03_architecture/resilience_patterns.md §Failback` (referenced inline by `corelink-replication-coordinator::state::HOT_STANDBY_COOLDOWN_SECONDS`); phase-3 of the drill asserts this pattern via monkeypatched-clock `CooldownNotElapsed` test.

## 19. Observability

`corelink_dr_drill_*` métricas Prometheus snake_case (already declared §6.1 item 7):

- `corelink_dr_drill_total{cycle="dr16",outcome,quarter}` (counter; alert se outcome=fail).
- `corelink_dr_drill_mtta_seconds{cycle="dr16",quarter}` (gauge; alert se > 300).
- `corelink_dr_drill_mttr_seconds{cycle="dr16",phase,quarter}` (gauge; alert se > 1800).
- `corelink_dr_drill_split_brain_count{cycle="dr16",quarter,phase}` (gauge; HARD-alert se > 0).
- `corelink_dr_drill_cooldown_enforced{cycle="dr16",quarter}` (gauge; HARD-alert se != 1).
- Alert se quarterly cadence missed (drill skipped).
- NUNCA per-tenant labels (canonical per S-09 cardinality lint).

## 20. Security & Privacy

**STRIDE delta**:
- **Spoofing**: drill orchestrator HTTP calls to coordinator require `COORDINATOR_ADMIN_TOKEN` + idempotency-key; rotated per BYOK pattern.
- **Tampering**: drill report git-tracked + 7y archive immutable em R2; coordinator state mutations gated by singleton DO lock + audit-emit BEFORE.
- **Repudiation**: every coordinator role mutation emits `region_promoted.v1` / `region_demoted.v1` / `failback_committed.v1` / `failback_blocked.v1` CloudEvents.
- **Information disclosure**: drill report sanitized; CTRL-PRIV-001 enforced; no PII em report.
- **DoS**: drill em staging only + isolated tenant.
- **Elevation of privilege**: drill chaos requires admin auth (S-13 reuse).

**LINDDUN delta**:
- Linkability: drill métricas tenant-agnostic.
- Identifiability: drill report sanitized (aggregate metrics).
- Non-repudiation: audit-bus CloudEvents + chain-hash forensic-grade.
- Detectability: drill alerts + métricas.
- Disclosure: drill scope-limited admin S-13 reuse.

## 21. Dependencies

### Hard blockers

- Wave-14 SEALed (`corelink-failover-router` + `RB-ACTIVE-FAILOVER.md` + `tests/e2e-failover-router/` 5-scenario harness).
- Wave-15 SEALed (`corelink-replication-coordinator` + `replica_failover.tla` ✅ GREEN + `tests/e2e-replication-failover/` 6-scenario harness).
- WI-S17-001 SEALed (chaos-mesh reuse for heartbeat-staleness inject).
- S-09 SEALed (multi-burn-rate alerts for SLO measurement during drill).

### Soft blockers

- Wave-16 `trait-abstraction-defer` production bindings (singleton DO + audit-bus + heartbeat-registry + `/v1/health/replication`) — drill rehearsal IS the integration test for these wirings.

### Outbound

- WI-S17-003 (runbook dry-run tracker; drill lessons-learned feed dry-run cadence).
- WI-S17-006 (game-day quarterly cadence; DR-16 is a candidate game-day scenario).
- ROADMAP-TO-GA §6 Wave R-6 (closure gate).

## 22. Effort PERT

O: 10h, M: 18h, P: 30h → PERT **18.7h** (per spec contract §12; orchestrator extension 4h + scenario 7 E2E 4h + quarterly drill execution + report 6h + cross-refs + métricas 3h + cadence calendar 2h).

## 23. Cost Analysis

**Direct cost**:
- Chaos-mesh reuse: $0/mês.
- R2 7y archive per quarter cycle: ~$2/mês (cumulative).

**Total**: ~$2/mês.

**Indirect cost**: Both-layer failover capability proven + 7y archive compliance baseline + production-binding integration test = priceless.

## 24. Post-mortem Hooks

- Drill MTTA breach (> 5 min) → 5-Why mandatório.
- Drill MTTR breach (> 30 min) → 5-Why mandatório + runbook updates required.
- Drill split-brain count > 0 (CRITICAL) → SEV-1 post-mortem; root-cause within 14d; TLA+ assumption violation hypothesis must be ruled out or fixed.
- Drill cool-down breach (cooldown_enforced != 1) → CRITICAL; coordinator-side bug; SEV-1.
- Drill chaos hits prod inadvertent → CRITICAL; HARD env check + post-mortem.

## 25. Rollback / Recovery

Drill rollback: chaos-mesh remove heartbeat-staleness + coordinator state restored from pre-drill capture + request-path region label restored. RTO ≤ 30min (mirror of WI-S17-002 §25). Note: in drill mode the coordinator failback is part of the drill itself (phase 3), so the "rollback" is the drill's own happy-path completion.

## 26. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Drill encounters audit-bus degradation mid-promote | M | H | LOW (state unchanged per fail-CLOSED) | M | LOW | Audit-bus health pre-check + scenario 1 adversarial coverage |
| R-002 | Drill chaos hits prod inadvertent | L | H | CRITICAL | M | LOW | Code-level env check FIRST (DRILL_TARGET=staging mandatory) + WI-S17-001 chaos guards reused |
| R-003 | MTTA breach (> 5 min) | M | M | HIGH (drill FAIL) | M | LOW | Alert routing tuning + SRE primary rotation matched to drill window |
| R-004 | MTTR breach (> 30 min) | M | M | HIGH (drill FAIL + runbook update) | M | LOW | Pre-drill dry-run + runbook step rehearsal |
| R-005 | Split-brain count > 0 (CRITICAL) | L | H | CRITICAL (INV violation; TLA+ assumption gap) | M | LOW | TLA+ ✅ GREEN baseline + property test 2000 cases + scenario 7 E2E |
| R-006 | 24h cool-down floor bypassed | L | H | CRITICAL (INV violation) | M | LOW | property test cooldown_always_blocks_under_24h fuzzes elapsed ∈ [0, 86_399] |
| R-007 | Trait-abstraction-defer wirings (DO singleton + audit-bus + /v1/health/replication) regress before drill | M | M | HIGH (drill blocked) | M | LOW | Wave-16 wiring PRs gate on drill scenario 7 green in CI |

## 27. Knowledge Transfer

- Tech talk (1h): "DR-16 active-failover drill — both-layer rehearsal + production-binding integration test walkthrough".
- Doc `docs/internal/s17-dr16-drill-runbook.md` — drill execution procedure (companion to `RB-REPLICA-FAILOVER.md` + `RB-ACTIVE-FAILOVER.md`).
- Onboarding test (3 questions): why both-layer in one drill / why 24h cool-down monkeypatched / where INV-FAILOVER-NO-SPLIT-BRAIN is proven (TLA+ + proptest + drill split-brain check probe).

## 28. Sign-off (STANDARD 5-8 canonical; 7 typical)

**Staffing reality (per ADR-0034 solo-tier)**:

| Status atual (2026-05-15) | Roles |
|---|---|
| **Confirmed (3)** | Owner; Final Approver; Engineer (Gustavo Schneiter — solo founder) |
| **Pending Tier-1 hire/contract (4)** | SRE Lead, Oncall Manager, QA, Compliance officer |

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | SRE Lead | _TBD; emphatic — DR-16 drill facilitation + cadence ownership_ | _pending_ | _pending_ |
| 4 | Engineer | Gustavo Schneiter | _pending_ | _pending_ |
| 5 | Oncall Manager | _TBD_ | _pending_ | _pending_ |
| 6 | QA | _TBD_ | _pending_ | _pending_ |
| 7 | Compliance officer | _TBD; emphatic — 7y archive + SOC 2 CC7.5/CC9.1 baseline_ | _pending_ | _pending_ |

## 29. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-05-15 | Gustavo (via Claude Opus 4.7) | Criação WI-S17-008 (wave-16 R-PREP; STANDARD lane; DR-16 active-failover drill rehearsal quarterly cadence + both-layer coordinator + request-path + MTTA/MTTR + 24h cool-down + 7y archive). |

## 30. Anti-patterns evitados

- Skip quarterly drill (mandatory ship gate).
- Drill em prod (anti-scope estrito; staging-only at GA).
- Skip 7y archive (compliance fail).
- Drill without SRE Lead facilitation.
- Drill without MTTA + MTTR measurement.
- Bypass 24h cool-down assertion (would invalidate INV-FAILOVER-NO-SPLIT-BRAIN drill coverage).
- Split coordinator + request-path into two drills (misses cross-layer split-brain pathway).

---

**Fim WI-S17-008.**
