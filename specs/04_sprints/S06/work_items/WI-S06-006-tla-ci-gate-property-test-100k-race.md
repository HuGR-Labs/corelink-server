---
id: "WI-S06-006"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-25"
updated: "2026-04-25"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-011", "FF-HR-005", "FF-HR-009"]
parent: "S-06"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "INVARIANT-REGISTRY"
  - "RESILIENCE-PATTERNS"
  - "OBSERVABILITY-MODEL"
tags: ["wi", "s06", "gc", "tla", "ci-gate", "property-test", "100k-race", "high-risk", "formal"]
---

# WI-S06-006 — TLA+ CI Gate Integration (`gc_correctness.tla` model check verde required pre-merge) + Property Test 100k Iter Race Mark + UpdateActionResult Interleavings (TLA+ ↔ Rust Cross-Validation) + PR Fail Logic + 30d Sustained Verification Gate

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-06](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S06-006 |
| Título | TLA+ `specs/tla/gc_correctness.tla` CI gate (TLC model check verde required pre-merge for PR touching `crates/corelink-gc` OR `data_model` blob_meta/ac_meta); Rust property test 100k iter race Mark + UpdateActionResult interleavings (cross-validates TLA+ obligations); PR fail logic em GitHub Actions; 30d sustained TLA+ verde gate (sprint contract DoD §10.s06.4 + critério promoção) |
| Sprint | S-06 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-011 (formal verification baseline; load-bearing), FF-HR-005 (controle integridade), FF-HR-009 (defense-in-depth: TLA+ + property test + chaos under load = 3 layers de evidence) |

## 1. Intent

CAP-GC-008: TLA+ CI gate é **formal verification baseline**; PR que toca GC code OR D1 schema (`blob_meta`/`ac_meta`) **MUST** trigger TLC model check; if TLC red → PR blocked. Property test 100k race em Rust **cross-validates** TLA+ obligations (Mark + UpdateActionResult interleavings; INV-GC-001 + INV-GC-004):

```yaml
# .github/workflows/tla-ci-gate.yml
name: TLA+ CI Gate

on:
  pull_request:
    paths:
      - 'crates/corelink-gc/**'
      - 'specs/tla/gc_correctness.tla'
      - 'migrations/**blob_meta**'
      - 'migrations/**ac_meta**'

jobs:
  tla-model-check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install TLC (TLA+ tools)
        run: |
          wget https://github.com/tlaplus/tlaplus/releases/download/v1.8.0/tla2tools.jar
          echo "TLA_HOME=$(pwd)/tla2tools.jar" >> $GITHUB_ENV
      - name: Run TLC model check on gc_correctness.tla
        run: |
          java -cp tla2tools.jar tlc2.TLC \
            -config specs/tla/gc_correctness.cfg \
            specs/tla/gc_correctness.tla \
            2>&1 | tee tlc-output.log
      - name: Verify INV-GC-001 + INV-GC-004 invariants verified
        run: |
          grep -q "Model checking completed.*0 errors found" tlc-output.log
          grep -q "InvGCNeverDeleteReachable: TRUE" tlc-output.log
          grep -q "InvGCReRefProtected: TRUE" tlc-output.log
      - name: Cross-validate Rust property test alignment
        run: |
          cargo test --release --test prop_gc_race_100k -- --ignored
```

```rust
// File: crates/corelink-gc/tests/prop_gc_race_100k.rs

#![forbid(unsafe_code)]

use proptest::prelude::*;
use rand::seq::SliceRandom;

/// Property test 100k iter race Mark + UpdateActionResult interleavings.
/// Cross-validates TLA+ `gc_correctness.tla` `InvGCReRefProtected` obligation.
/// Sprint contract DoD §6: "property test em Rust cobrindo race Mark+UpdateActionResult 100k iterations".
#[test]
#[ignore]  // run via `cargo test --release --test prop_gc_race_100k -- --ignored` (CI nightly)
fn prop_gc_004_race_mark_update_ar_100k() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut violations = 0u64;

    for iter in 0..100_000 {
        let scenario = generate_random_interleaving(iter);
        let result = runtime.block_on(execute_gc_scenario(scenario));

        match result {
            // Sweep correctly protected reachable blob (INV-GC-004 enforced):
            ScenarioResult::ProtectedReRef { mark_started_at_ms, ac_created_at_ms } => {
                assert!(ac_created_at_ms >= mark_started_at_ms, "Strict >= boundary");
            }
            // Sweep correctly soft-deleted confirmed orphan:
            ScenarioResult::SoftDeleted { mark_started_at_ms, no_ac_after_mark } => {
                assert!(no_ac_after_mark, "No ac.created_at >= mark_started_at_ms");
            }
            // CRITICAL: INV-GC-004 violation (reachable blob soft-deleted):
            ScenarioResult::Violation { mark_started_at_ms, ac_created_at_ms } => {
                violations += 1;
                eprintln!(
                    "VIOLATION iter {iter}: mark_started_at_ms = {mark_started_at_ms}; \
                     ac_created_at_ms = {ac_created_at_ms} (>= mark_started_at_ms but blob deleted)"
                );
            }
        }
    }

    assert_eq!(
        violations, 0,
        "INV-GC-004 violations detected: {violations} of 100k iterations; \
         TLA+ obligation `InvGCReRefProtected` violated; CRITICAL — block merge"
    );
}

fn generate_random_interleaving(seed: u64) -> Scenario {
    // Generates random Mark + UpdateActionResult interleavings:
    // - Mark phase: captures mark_started_at_ms = T (random within scenario timeline)
    // - Concurrent UpdateActionResult: ac_meta INSERT at T-100ms..T+1000ms (random)
    // - Sweep phase: invokes INV-GC-004 SQL EXISTS check
    // Edge cases: ac.created_at = T exactly (boundary); ac.created_at = T+1ms (just after; protected); ac.created_at = T-1ms (just before mark; orphan).
    // ...
}

async fn execute_gc_scenario(scenario: Scenario) -> ScenarioResult {
    // Reuses real Rust impl from WI-S06-002 (mark) + WI-S06-003 (sweep).
    // Cross-validates against TLA+ spec semantics.
    // ...
}
```

**Cripto-driven invariants enforced**:

1. **TLA+ `gc_correctness.tla` already verified** (Lote 5.13 + 7.1 fixes). This WI integrates CI gate enforcement.

2. **PR fail logic**: TLC red → CI fails → merge blocked; reviewer can override only via ADR + Architect+Crypto SME sign-off (consistente Lote 10.4bis ADR governance lesson).

3. **Property test 100k iter** (sprint contract §6 DoD): random interleavings; INV-GC-004 violation count MUST be 0; if any > 0, test fails CRITICAL (SEV-0).

4. **30d sustained TLA+ verde** (sprint contract §10.s06.4): post-sprint observation period; CI history verde 30d; documented em §13 timeline (concurrent S-07/S-08 sprints).

5. **CI cost**: TLC model check ~2-5 min per PR; property test 100k ~30-60 min CI nightly (não per-PR; weight too high).

## 2. Narrative (HIGH_RISK ≥ 300 palavras)

TLA+ CI gate é **single source of formal proof** that GC algorithm is correct. Without CI gate, TLA+ spec is documentation; with CI gate, it's **enforced contract**. Property test 100k race em Rust cross-validates TLA+ obligations actually hold em real impl. Defense-in-depth:

1. **TLA+ formal verification** (CI gate): exhaustive state-space exploration; proves INV-GC-001/004 hold under ALL interleavings model considers.
2. **Property test 100k iter**: random sampling em real Rust impl; cross-validates TLA+ assumptions translate to code.
3. **Chaos test 30d staging** (sprint contract DoD): real workload com concurrent Mark + UpdateAR + sweep; INV-GC-001/004 0 violations sustained.

Three layers; if any one detects violation, defense responds.

HIGH_RISK em N dimensões:

1. **TLA+ scope incompleto** (model não cobre cenário real): Mitigação: adversarial review por Architect + AppSec + Crypto SME; quarterly re-review com production traces; expand spec se gap detected.

2. **CI flake** (TLC indeterministic): Mitigação: TLC seed fixado; deterministic exploration; if flake suspected, retry 3× via CI policy.

3. **Property test 100k flake** (rare): Mitigação: deterministic seed via `iter` param; reproducible failures; chaos test re-runs.

4. **TLA+ ↔ Rust drift** (TLA+ spec models X, Rust impl does Y): cross-validation property test detects; chaos test #3 simulates intentional drift.

5. **Merge override discipline**: Architect+Crypto SME + ADR mandatory (lesson Lote 10.4bis ADR governance); rubber-stamp prevention.

6. **PR scope sensitivity**: only PR touching `crates/corelink-gc` OR `blob_meta`/`ac_meta` migration triggers CI gate; minimal blast radius.

7. **Sprint contract DoD §10.s06.4 30d sustained**: post-sprint observation period; concurrent S-07/S-08; if TLC red anytime in 30d, sprint promotion blocked.

8. **Cost gate**: TLC ~2-5 min × M PRs/day = bounded daily cost; property test 100k nightly ~$1/dia.

**Atacante adversarial scenarios**:

- **CI bypass attempt** (force-push, sub-PR): GitHub branch protection requires status checks; cannot bypass.
- **TLA+ spec corruption** (PR modifies spec to weaken obligations): adversarial review catches; ADR required.
- **Property test seed manipulation**: deterministic seeds preserved em git history.

**Risk justification HIGH_RISK**:

- **FF-HR-011**: formal verification baseline.
- **FF-HR-005**: integridade.
- **FF-HR-009**: defense-in-depth 3 layers.

13 sign-offs incl. **Crypto SME mandatory emphatic** (TLA+ obligation alignment).

## 3. Customer Impact & Journey

**Persona 1 — Customer**: invisible (CI infrastructure); customer-visible only via SLO-CORRECT-GC sustained metric (forward S-09).

**Persona 2 — Engineer pushing GC PR**: TLC verde required; if red, PR blocked; investigation triggered.

**Persona 3 — Architect/Crypto SME**: reviews TLA+ spec changes; approves ADR for spec modifications; reviews property test 100k results monthly.

**SLA addendum**:
- TLC CI gate ≤ 5 min per PR (model check).
- Property test 100k ≤ 60 min nightly CI.
- 30d sustained verde required pre-S-20 GA promotion.
- TLA+ scope review quarterly + on-demand if production trace exposes gap.

## 4. Capability Mapping

- **CAP-GC-008** (TLA+ CI gate) — IMPLEMENTA primary.
- Trace: `invariant_registry.md INV-GC-001/004 + §3.4` + `specs/tla/gc_correctness.tla` (Lote 5.13).

## 5. Tipo

CI infrastructure + formal verification gate; HIGH_RISK; FF-HR-011 + FF-HR-005 + FF-HR-009.

## 6. Escopo (compact)

### 6.1 In-scope

1. **`.github/workflows/tla-ci-gate.yml`**: TLC model check em PR touching GC code OR migrations.
2. **TLC integration**: download tla2tools.jar; run model check com gc_correctness.cfg.
3. **Invariant assertions**: parse TLC output; assert INV-GC-001 + INV-GC-004 verified.
4. **Property test 100k**: `crates/corelink-gc/tests/prop_gc_race_100k.rs` — random interleavings; cross-validates TLA+ obligations.
5. **Property test scenario generator**: deterministic seeds; covers Mark + UpdateActionResult + sweep edge cases.
6. **PR fail logic**: GitHub Actions status check required; merge blocked if TLC red.
7. **Override discipline**: ADR + Architect + Crypto SME sign-off mandatory pre-override.
8. **30d sustained verification**: GitHub Actions history check; CI workflow `tla-30d-sustained.yml` daily runs aggregate verde 30d.
9. **Property test CI integration**: nightly CI `cargo test --release --test prop_gc_race_100k -- --ignored`.
10. **Métricas**:
    - `corelink.ci.tla.runs_total{result}` (counter).
    - `corelink.ci.tla.duration_ms` (histogram; SLO ≤ 5 min p99).
    - `corelink.ci.property_test_100k.runs_total{result}` (counter; CRITICAL alert se result=violation).
    - `corelink.ci.property_test_100k.violations_total` (alert > 0; SEV-0).
    - `corelink.ci.tla.30d_sustained_verde` (gauge boolean).
11. **Property tests** (Rust):
    - `prop_gc_004_race_mark_update_ar_100k` (CRITICAL; sprint contract §6 DoD).
    - `prop_gc_001_reachable_never_deleted_100k` (CRITICAL; cross-validates INV-GC-001).
    - `prop_tla_rust_alignment`: TLA+ spec scenarios → execute em Rust; assert same final state.
12. **Chaos suite** (sprint contract HIGH_RISK ≥ 10):
    - 1. PR touches GC code → CI gate triggers; TLC verde → merge unblocked.
    - 2. PR weakens TLA+ obligation → CI red; merge blocked.
    - 3. CI override attempt without ADR → blocked by branch protection.
    - 4. Property test 100k violation injected → SEV-0 alert; merge blocked.
    - 5. TLC flake (rare) → retry 3×; if persistent, investigate.
    - 6. TLA+ ↔ Rust drift simulated (intentional impl bug) → property test catches; CI red.
    - 7. CI cost regression → bench bound TLC ≤ 5 min per PR.
    - 8. 30d sustained verde gate validates S-20 GA promotion.
    - 9. Adversarial spec change (weakening InvGCReRefProtected) → adversarial review catches.
    - 10. Production trace exposes spec gap → quarterly re-review + spec expand.

### 6.2 Out-of-scope

- TLA+ spec authoring (already done Lote 5.13 + 7.1 fixes).
- Production observability (S-09 forward).
- Customer-facing SLO dashboard (S-16 forward).

## 7. Anti-Scope

- ❌ Skip CI gate enforcement (formal verification baseline).
- ❌ Override TLC red without ADR + Architect + Crypto SME sign-off.
- ❌ Property test 100k flake silenced (always investigate; deterministic seeds).
- ❌ TLA+ scope reduction without ADR.
- ❌ Skip 30d sustained gate pre-S-20 GA.
- ❌ Cross-tenant property test scenarios (per-tenant strict).

## 8. Acceptance Criteria (Gherkin) (compact 8 scenarios)

```gherkin
Feature: TLA+ CI gate + property test 100k race

  Scenario: PR touches GC code → CI gate triggers
    Given PR modifies crates/corelink-gc/src/sweep.rs
    When CI workflow tla-ci-gate runs
    Then TLC model check executes ≤ 5 min
    And INV-GC-001 + INV-GC-004 verified TRUE
    And status check green; merge unblocked

  Scenario: PR weakens TLA+ obligation → CI red
    Given PR modifies gc_correctness.tla weakening InvGCReRefProtected
    When CI runs TLC
    Then TLC reports counterexample; CI red
    And merge blocked; review required

  Scenario: Property test 100k violation → SEV-0
    Given property test prop_gc_004_race_mark_update_ar_100k
    When run
    Then violations_total = 0 (always); if > 0, SEV-0 alert; merge blocked
    And TLA+ obligation `InvGCReRefProtected` cross-validated

  Scenario: Override attempt without ADR
    Given developer tries to force-merge with TLC red
    When GitHub branch protection check
    Then merge blocked
    And only ADR + Architect + Crypto SME signoff allows override

  Scenario: TLC flake retry
    Given TLC indeterministic failure
    When CI workflow retries up to 3×
    Then if all 3 fail, mark as persistent; investigation triggered

  Scenario: 30d sustained verde gate
    Given CI history checked daily via tla-30d-sustained.yml
    When 30d window passes with all CI runs verde
    Then metric `corelink.ci.tla.30d_sustained_verde` = TRUE
    And S-20 GA promotion unblocked

  Scenario: TLA+ ↔ Rust drift detected
    Given Rust impl drifts from TLA+ spec semantics
    When property test 100k runs
    Then drift detected; violation count > 0
    And SEV-0 alert; investigate impl-vs-spec divergence

  Scenario: Quarterly re-review
    Given production trace exposes spec gap
    When Architect + Crypto SME quarterly review
    Then spec expanded; new TLA+ obligations added
    And ADR documents change
```

## 9-32 (compact)

### 9. Design Decisions

- 9.1: TLC pinned version v1.8.0 (deterministic).
- 9.2: PR scope filter (path-based; minimal blast radius).
- 9.3: Property test 100k nightly (não per-PR; CI cost bounded).
- 9.4: Override via ADR + Architect + Crypto SME signoff (Lote 10.4bis governance).
- 9.5: 30d sustained verde gate post-sprint observation (sprint contract §10.s06.4).
- 9.6: Deterministic property test seeds (reproducible failures).
- 9.7: GitHub branch protection enforces required status checks.
- 9.8: Cross-validation property test (TLA+ ↔ Rust alignment).
- 9.9: ADR-0042 (WI-001) covers; no new ADR.
- 9.10: TLC ≤ 5 min p99 per PR (model state space bounded).

### 10. Completeness Criteria

- [ ] **10.s06.006.1** TLA+ CI workflow live; PR triggers TLC model check.
- [ ] **10.s06.006.2** Property test 100k race em CI nightly; 0 violations sustained.
- [ ] **10.s06.006.3** **30d sustained TLA+ verde gate** (sprint contract DoD §10.s06.4).
- [ ] **10.s06.006.4** PR fail logic via GitHub branch protection.
- [ ] **10.s06.006.5** Override discipline ADR + Architect + Crypto SME signoff.
- [ ] **10.s06.006.6** Métricas (5) emitted; CRITICAL alert violations > 0.
- [ ] **10.s06.006.7** TLA+ ↔ Rust alignment cross-validation.
- [ ] **10.s06.006.8** Quarterly re-review process documented.
- [ ] **10.s06.006.9** Cargo-audit + cargo-deny clean.
- [ ] **10.s06.006.10** CI cost gate: TLC ≤ 5 min p99 per PR.

### 11. DoD

- [ ] CI workflow live; PR gate functional; property test nightly green; 30d sustained gate operational; Architect + Crypto SME independent review; PRR mini.

### 12. Invariants Validated

- **INV-GC-001** (CRITICAL, registry §3.4 + TLA+): formally verified via TLC.
- **INV-GC-004** (CRITICAL, registry §3.4 + TLA+ InvGCReRefProtected): formally verified.
- **INV-GC-CI-GATE-ENFORCED** (HIGH, NEW promovida §3.17): CI gate blocks merge se TLC red.
- **INV-GC-PROPERTY-TEST-CROSS-VALIDATED** (HIGH, NEW): TLA+ ↔ Rust alignment 100k iter.
- **INV-GC-30D-SUSTAINED-VERIFICATION** (HIGH, NEW): 30d CI verde gate pre-S-20 GA.

### 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| CI workflow | `.github/workflows/tla-ci-gate.yml` | YAML |
| 30d sustained workflow | `.github/workflows/tla-30d-sustained.yml` | YAML |
| Property test 100k | `crates/corelink-gc/tests/prop_gc_race_100k.rs` | Rust |
| Cross-validation tests | `crates/corelink-gc/tests/prop_tla_rust_alignment.rs` | Rust |
| Override ADR template | `specs/03_architecture/adrs/ADR-template-tla-override.md` | Markdown |

### 14. Quality Standards

- Standards: zero unsafe; rustdoc 100%; coverage ≥ 95%; latency CI gate ≤ 5 min; SAST clean; 5 metrics; cost gate per-PR ≤ $0.000001 (TLC compute).

### 15. Chaos Experiments (10)

§6.1.12.

### 16. PRR

Mini-PRR Architect + **Crypto SME mandatory emphatic** (TLA+ obligation).

### 17. Sub-tasks

| ID | h |
|---|---|
| ST-001 CI workflow tla-ci-gate.yml | 3 |
| ST-002 TLC integration + invariant assertions | 3 |
| ST-003 PR scope filter (path-based) | 1 |
| ST-004 GitHub branch protection config | 1 |
| ST-005 Property test 100k race scenario generator | 5 |
| ST-006 Property test execution + assertion | 3 |
| ST-007 Cross-validation TLA+ ↔ Rust | 3 |
| ST-008 30d sustained workflow | 2 |
| ST-009 Override discipline ADR template | 2 |
| ST-010 Métricas emit (5 metrics) | 1.5 |
| ST-011 Quarterly re-review process doc | 1.5 |
| ST-012 Crypto SME independent review | 4 |

Total Optimistic: ~30h. PERT: ~33h.

### 18-32 (compact final)

- 18 Dependencies: Hard TLA+ `gc_correctness.tla` Lote 5.13/7.1 verified; WI-S06-002+003 (impl); GitHub Actions; soft S-09 metrics; outbound WI-S06-007 (consumes 30d verde for ship gate).
- 19 PERT 33h; 20 Time-boxing 40h.
- 21 Observability 5 metrics; trace `gc.ci.{tla, property_test}`.
- 22 Cost: TLC ~$0.000001 per PR × 50 PRs/dia = ~$0.05/dia = **$18/yr**; property test nightly ~$1/dia = $365/yr; total ~$385/yr.
- 23 API Contract: CI workflow YAML; property test Rust API; ADR template.
- 24 Post-mortem: TLC red sustained > 4h → CRITICAL post-mortem (sprint contract); property test violation → SEV-0; TLA+ ↔ Rust drift detected → CRITICAL.
- 25 Rollback: CI workflow revert via git; property test rollback via cargo update; RTO ≤ 10 min; RPO 0.
- 26 Security: CI gate enforces formal proof; override discipline ADR; cross-validation prevents drift.
- 27 Knowledge Transfer: Tech talk 2h "TLA+ CI Gate + Property Test 100k Race + Cross-Validation"; doc; onboarding test 5q.
- 28 Risk Register (12-row): TLA+ scope incompleto M M HIGH M LOW (adversarial review); CI flake L H LOW L LOW (deterministic seeds); property test flake M H LOW L LOW (retry 3×); TLA+↔Rust drift L M HIGH L LOW (cross-validation); merge override abuse L L HIGH L LOW (ADR + Architect signoff); PR scope miss M L MEDIUM L LOW (path filter); CI cost regression M L MEDIUM L LOW; 30d sustained gate slip M M HIGH M LOW; spec corruption attack L L HIGH L LOW (adversarial review); production gap L M HIGH L LOW (quarterly re-review); GitHub Actions outage L L LOW L LOW; TLC version drift L L MEDIUM L LOW (pinned version).
- 29 Review: D+0 design (Architect + Crypto SME); D+2 AppSec; D+5 code review; D+6 Crypto SME independent; D+7 PRR mini.
- 30 Sign-off (HIGH_RISK 13): standard 12 mandatory; Crypto SME **MANDATORY EMPHATIC** (TLA+ obligation alignment).
- 31 Change Log: 1.0.0 / 2026-04-25 / Gustavo (Lote 10.6).
- 32 Anti-patterns: ❌ Skip CI gate; ❌ Override sem ADR; ❌ Property test flake silenciada; ❌ TLA+ scope reduction sem ADR; ❌ Skip 30d sustained gate; ❌ Cross-tenant property test scenarios; ❌ TLC version drift sem ADR.

---

**Fim WI-S06-006.** Próximo: WI-S06-007 (DASH-GC + reclaim metric + RB-FM-300/404/305 dry-runs + PRR ship gate).
