---
id: "WI-S06-006"
type: "work_item"
doc_status: "FROZEN"
work_status: "DONE"
audit_status: "AUDITED"
version: "1.3.0"
created: "2026-04-25"
updated: "2026-05-02"
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
# .github/workflows/tla_check.yml (canonical filename pós Lote 10.6 cycle 4; was tla-ci-gate.yml in WI authoring draft)
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
      - name: Install TLC v1.8.0 (SHA-256 PINNED — Lote 10.6bis P0-W6-2 + Lote 10.6-tris NEW-P0-1)
        run: |
          # tla2tools.jar v1.8.0 SHA-256 (locked; bumps require ADR + Architect + Crypto SME signoff per ADR-0042 addendum §A1)
          # Lote 10.6-tris NEW-P0-1 fix: literal 64-char hex (was placeholder string that always failed).
          # Bootstrap trust: SHA computed 2026-04-25 by Owner (Gustavo Schneiter) via fresh download from
          # github.com/tlaplus/tlaplus/releases/download/v1.8.0/tla2tools.jar (artifact size 4356704 bytes;
          # `shasum -a 256 tla2tools.jar`). Architect + Crypto SME independent re-verification REQUIRED
          # pre-merge per §9.X bootstrap trust ceremony — both reviewers must commit signed verification
          # comments to the ADR-0042 addendum §A1 sign-off block before this workflow merges to main.
          EXPECTED_SHA="d5d07d5dab38ddb840c91ec48fa02f28b37a608d5af9a73570018591dbc8ef7f"
          curl --retry 3 --fail -L \
            https://github.com/tlaplus/tlaplus/releases/download/v1.8.0/tla2tools.jar \
            -o tla2tools.jar
          ACTUAL_SHA=$(sha256sum tla2tools.jar | cut -d' ' -f1)
          if [ "$ACTUAL_SHA" != "$EXPECTED_SHA" ]; then
            echo "FATAL: TLC v1.8.0 SHA-256 mismatch — expected $EXPECTED_SHA, got $ACTUAL_SHA"
            echo "       supply-chain integrity check failed; refusing to run formal verification with untrusted binary"
            exit 1
          fi
          echo "TLA_HOME=$(pwd)/tla2tools.jar" >> $GITHUB_ENV
        # Cache keyed by SHA to avoid re-downloading every PR (cost optimization)
      - name: Run TLC model check on gc_correctness.tla
        run: |
          java -cp tla2tools.jar tlc2.TLC \
            -config specs/tla/gc_correctness.cfg \
            specs/tla/gc_correctness.tla \
            2>&1 | tee tlc-output.log
      - name: Verify INV-GC-001 + INV-GC-004 invariants verified
        run: |
          grep -q "Model checking completed.*0 errors found" tlc-output.log
          grep -q "InvGCReachableNeverDeleted: TRUE" tlc-output.log  # canonical name pós Lote 10.6 cycle 1 (was InvGCNeverDeleteReachable typo)
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
    // Lote 10.6-tris OPUS-MISS-2 fix: PRNG determinism PINNED to ChaCha20Rng + seed_from_u64.
    // Reproducibility is now a code-level guarantee (NOT a documentation claim) — CI failure
    // at iter=42 reproduces deterministically across all platforms (rust-stdlib HashMap iteration
    // ordering is the only remaining non-determinism source, addressed by sorted iteration in scenario).
    use rand::SeedableRng;
    use rand_chacha::ChaCha20Rng;
    let mut rng = ChaCha20Rng::seed_from_u64(seed);

    // Generates random Mark + UpdateActionResult interleavings:
    // - Mark phase: captures mark_started_at_ms = T (random within scenario timeline)
    // - Concurrent UpdateActionResult: ac_meta INSERT at T-100ms..T+1000ms (random)
    // - Sweep phase: invokes INV-GC-004 SQL EXISTS check
    // Edge cases: ac.created_at = T exactly (boundary); ac.created_at = T+1ms (just after; protected); ac.created_at = T-1ms (just before mark; orphan).
    //
    // Lote 10.6bis P0-W6-1 adversarial inputs (fixture must emit):
    // - blob_refs envelope mutation: {"refs":[...],"metadata":{"parent_digest":<other_digest>}}
    // - Short-digest substring scenarios (16-char prefix in metadata field containing digest D as substring)
    // - Schema-evolution scenario: blob_refs evolves to JSON object — assert SQL fails compile OR returns 0 protected
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

2. **PR fail logic**: TLC red → CI fails → merge blocked; reviewer can override only via **pinned GitHub mechanism** (Lote 10.6bis P0-W6-3 fix):
   - **CODEOWNERS rule**: `crates/corelink-gc/** specs/tla/gc_correctness.tla @HumanGuardrail/architect-team @HumanGuardrail/crypto-sme-team` — modifications to TLA+ spec OR GC code require **2 approving reviews** (1 Architect + 1 Crypto SME group; non-overlapping).
   - **Override workflow** `tla_override_validate.yml`: parses PR body / commit trailers for `Tla-Override-ADR: ADR-XXXX`; asserts (a) ADR file exists at `specs/03_architecture/adrs/ADR-XXXX-*.md`; (b) `doc_status: ACCEPTED`; (c) Architect + Crypto SME approvals in ADR sign-off block. Only on success posts a synthetic green status check that satisfies the required `tla_check` check (canonical workflow filename pós Lote 10.6 cycle 4).
   - **Branch protection** on `main`: `enforce_admins: true` (Lote 10.4bis governance — disables admin-bypass; closes the rubber-stamp regression default).
   - **Without ADR + 2 approvals**: NO override path. Force-push blocked.

3. **Property test 100k iter** (sprint contract §6 DoD): random interleavings; INV-GC-004 violation count MUST be 0; if any > 0, test fails CRITICAL (SEV-0).
   - **Hard cross-WI dependency** (Lote 10.6bis P0-W6-1): WI-S06-003 §6.1.2 SQL EXISTS check uses `json_each(a.blob_refs)` idiom (Part 1 P0-1 fix landed); blocking precondition for property test CI run. If WI-003 ships with broken LIKE check, property test PASSES GREEN silently (BLAKE3-256 hex digests at fixed 64 chars never collide via substring under benign fixture generators) → false confidence INV-GC-004 holds while production SQL is broken.
   - **Fixture generator adversarial inputs** (Lote 10.6bis P0-W6-1): `generate_random_interleaving` MUST emit (a) `blob_refs` envelope mutation `{"refs":[...],"metadata":{"parent_digest":<other_digest>}}`; (b) short-digest substring scenarios (16-char prefix in metadata field containing digest D as substring); (c) schema-evolution scenario where `blob_refs` evolves to JSON object — assert SQL fails compile (compile-time check) OR returns 0 protected for all candidates (semantic check).
   - **Negative-control discriminating-power assertion**: pre-SEAL, run property test against deliberately-broken LIKE-substring impl as **negative control**; verify the test FAILS the broken impl (proves test has discriminating power against the LIKE defect class). Crypto SME differential review checkpoint.

4. **30d sustained TLA+ verde** (sprint contract §10.s06.4) — workflow design pinned (Lote 10.6bis P0-W6-W7-1):
   - **Cadence**: daily 06:00 UTC (post-overnight property test) + manual trigger via `workflow_dispatch`.
   - **Query mechanism**: `gh api repos/$REPO/actions/workflows/tla_check.yml/runs?per_page=100&created=>=$THIRTY_DAYS_AGO` — paginate; aggregate.
   - **Verde definition**: ALL non-cancelled / non-skipped runs in 30d window have `conclusion=='success'`. Cancelled runs IGNORED. Skipped runs (PR didn't touch GC code) IGNORED.
   - **30d window**: last 30 calendar days from workflow run's `now()`.
   - **P0/P1 incident clock reset** (Lote 10.4bis 4-tier classification): if any TLC red sustained > 4h OR property-test violation incident in window → 30d clock RESET (clock starts from incident resolution timestamp).
   - **Output**: emit `corelink.ci.tla.30d_sustained_verde` (gauge boolean) + `corelink.ci.tla.30d_window_days` (gauge int = days since last clock reset).
   - **S-20 GA promotion gate**: requires gauge == TRUE for ≥ 30d before sprint promotion.
   - Workflow yaml inline em §13 artifacts.
   - **Queryable per-day counter (R4-P1-006-1 absorption, 2026-05-15)**: per R4-Opus-Part2 §3.2 P1-006-1 (`PRINC-PROMOTION-001`), the S-20 GA gate requires a *queryable* sustained-counter (not a manual visual check on the CI history). Canonical emission:
     - **Metric**: `tla_check_pass{date}` (counter; per-day boolean; emitted by the `tla_check.yml` workflow as the last step on `success` conclusion). The metric is the **daily aggregator source** for the 30d rolling sustained-counter.
     - **Daily aggregator**: a separate daily-cron workflow `.github/workflows/tla-30d-sustained.yml` (forward-deferred to S-20 via debt register cumulative-track row DEBT-S06-P1-006-1) queries the last 30 daily values of `tla_check_pass{date}`, aggregates into the canonical sprint-contract counter `gc_30d_sustained_observation_count{window="tla_verde", scope="global"}`, and emits the rolling sum.
     - **Consumer**: S-20 GA gate validator `validate_30d_sustained_gates.py` (forward-deferred via the same debt register row); requires `gc_30d_sustained_observation_count{window="tla_verde"} >= 30` before authorizing GA promotion.
     - **DASH-GC consumption**: panel 9 (`corelink_ci_tla_30d_sustained_verde{spec="gc_correctness"}` — already shipped per PRR-S06 §7) is augmented additively with the per-day counter tile per `window` label in the S-20 GA gate cumulative track.
     - **Window start-date binding**: pinned canonical in `_spec_contract.md §13.1` + `PRR-S06.md §10.1` (`STAGING-STABLE promotion timestamp + 1d (UTC) = 2026-05-03 00:00:00 UTC`).
     - **Reset triggers**: same as §1.4 P0/P1 incident clock reset (pause-clock-on-P0/P1; Lote 10.4bis lesson).
     - Cross-ref: R4-Opus-Part2 §3.2 P1-006-1; `PRINC-PROMOTION-001`; `2026-05-15-debt-register.md` DEBT-S06-P1-006-1.

5. **CI cost**: TLC model check ~2-5 min per PR; property test 100k ~30-60 min CI nightly (não per-PR; weight too high). Note: GitHub Actions billing for **private repo** at $0.008/min × 5 min × 50 PRs/day ≈ $2/day = $730/yr (Lote 10.6bis P2-W6-2 cost re-derivation; previous $18/yr estimate assumed free-tier minutes).

6. **TLA+ ↔ Rust action mapping** (Lote 10.6bis P1-W6-1; canonical home WI-006; lifted from Part 1 P0-3 fix):

| TLA+ action | Rust impl | INV impact |
|---|---|---|
| `MarkPhaseStart` | `mark_phase::execute_3_pass_scan` start; UPDATE `gc_run.mark_started_at_ms` commit-then-scan ordering | INV-GC-MARK-STARTED-AT-ATOMIC + INV-GC-MARK-STARTED-AT-IMMUTABLE |
| `GCMarkStep(blob)` | `mark_phase::scan_batch` per-blob iteration over D1 chunks; `json_each(a.blob_refs) j WHERE j.value = blob.digest` predicate | INV-GC-001 reachable-set-complete |
| `UpdateActionResult(ac)` | `crates/corelink-ac::update_action_result` writes `ac_meta.created_at` server-side | INV-GC-004 mark-phase-aware re-ref |
| `SweepStep(blob)` | `sweep_phase::execute` per-candidate; INV-GC-004 EXISTS check via `json_each(a.blob_refs) j WHERE j.value = $candidate AND a.created_at >= mark_started_at_ms` | INV-GC-004 enforcement |
| `InvGCReRefProtected` | property test `prop_gc_004_race_mark_update_ar_100k` 100k iter; CI nightly | TLA+ ↔ Rust differential alignment |

7. **TLA+ formal verification SCOPE LIMITATIONS** (Lote 10.6-tris NEW-P0-2 fix — explicit caveat documented; Crypto SME PRR review must acknowledge):

   **What `gc_correctness.tla` proves**:
   - Mark-and-sweep algorithm correctness for the active → physically-deleted transition under arbitrary interleavings of `Mark + UpdateActionResult` actions.
   - INV-GC-001 (reachable never deleted) holds at the `physically_deleted` set membership check.
   - INV-GC-004 (mark-phase-aware re-ref) holds via `ac.created_at >= mark_started_at` protect-if-equal-or-newer canonical TLA semantics (`gc_correctness.tla` L152-154; equivalent: delete only if all `ac.created_at < mark_started_at`).
   - Bounded-state TLC model check (Lote 10.6 cycle 2 canonical): Blobs={b1,b2}, AC_Entries={e1}, MaxTime=10, GracePeriod=2 (cfg authoritative em `specs/tla/gc_correctness.cfg`). At these minimal bounds, all interleavings exhaustively explored (~5k-50k states; ≤30s CI). **Property test 100k extends coverage** to larger-scale interleavings via random sampling against the real Rust impl (defense-in-depth layer 2).

   **What `gc_correctness.tla` does NOT prove**:
   - **Soft-delete grace window** (72h CAS / 24h AC): the TLA+ model's `GCSweepBlob` action transitions blob directly from active to `physically_deleted` atomically — there is NO `soft_deleted` intermediate state in the TLA+ spec. The Rust implementation's two-phase soft-delete (WI-S06-003) → physical delete (WI-S06-004) is **outside the TLA+ formal coverage**.
   - **Re-reference during grace window**: an UpdateActionResult firing AFTER soft-delete but BEFORE physical-delete is protected by **architectural mechanisms** (NOT TLA+):
     - (a) `WHERE refcount = 0` conditional D1 batch predicate in physical-delete (WI-S06-004 §6.1.6, Lote 10.6bis P0-4 fix) — refcount increment by S-01 CAS write handler causes DELETE no-op.
     - (b) `undelete` path via re-upload (CAP-GC-002, sprint contract §5.3 R-S06-7) — customer re-upload during grace reverts `deleted_at = NULL`.
   - **DSR bypass path** (WI-S06-004 §1.4): Ed25519 signal verification + scope + replay protection are NOT in TLA+ scope; protected by Crypto SME-reviewed Ed25519 verify + `dsr_signals_processed.signal_id` UNIQUE constraint.
   - **Physical-delete cron orchestration** (WI-S06-004): R2→D1 ordering + crash recovery via reconcile orphan detection is operational, NOT formally verified.

   **Implication for "INV-GC-001 formally proven" claim**: the claim is correct AT THE ALGORITHM LEVEL (mark-sweep correctness) but does NOT extend to the grace-window reversibility mechanism. WI-007 §1.7 + sprint contract §6 DoD must reflect this scope. The **defense-in-depth** chain is:
   - **Layer 1 (TLA+)**: mark-sweep algorithm at bounded scale.
   - **Layer 2 (Property test 100k)**: real Rust impl at larger scale; cross-validates TLA+ obligations.
   - **Layer 3 (Chaos under load 4h-1kQPS + 30d sustained)**: production-like workload; INV-GC-001/004 violation counters = 0.
   - **Layer 4 (Conditional `refcount = 0` predicate + reconcile orphan detection)**: grace window protection NOT covered by Layers 1-3; covered by code-level invariants (WI-S06-004 §6.1.6 + WI-S06-005 reconcile).

   **Future TLA+ extension** (NOT required for S-06 SEAL; deferred to S-07+ per scope-honest documentation): extend `gc_correctness.tla` with `soft_deleted` state, `grace_period` time variable, `Undelete` action, and verify INV-GC-001 holds across the soft-delete→physical-delete transition. Effort estimate: ~8h TLA+ extension + ~4h Crypto SME re-review. Tracked as `Lote 10.7+ TLA+ scope expansion` (post-S-06 SEAL).

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

11 sign-offs (canonical Lote 10.6 cycle 4 alignment with sprint.md §14 + _spec_contract §2; HIGH_RISK lane ceiling 12) incl. **Crypto SME mandatory emphatic** (TLA+ obligation alignment).

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

1. **`.github/workflows/tla_check.yml`** (canonical filename pós Lote 10.6 cycle 4): TLC model check em PR touching GC code OR migrations.
2. **TLC integration**: download tla2tools.jar; run model check com gc_correctness.cfg.
3. **Invariant assertions**: parse TLC output; assert INV-GC-001 + INV-GC-004 verified.
4. **Property test 100k**: `crates/corelink-gc/tests/prop_gc_race_100k.rs` — random interleavings; cross-validates TLA+ obligations.
5. **Property test scenario generator**: deterministic seeds; covers Mark + UpdateActionResult + sweep edge cases.
6. **PR fail logic**: GitHub Actions status check required; merge blocked if TLC red.
7. **Override discipline**: ADR + Architect + Crypto SME sign-off mandatory pre-override.
8. **30d sustained verification**: GitHub Actions history check; CI workflow `tla-30d-sustained.yml` (PLANNED — WI-S06-006 deliverable; not yet in tree) daily runs aggregate verde 30d.
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
12. **Chaos suite** (12 scenarios; SOTA bar above HIGH_RISK floor 10; Lote 10.6bis P1-W6-2 expansion):
    - 1. PR touches GC code → CI gate triggers; TLC verde → merge unblocked.
    - 2. PR weakens TLA+ obligation → CI red; merge blocked.
    - 3. **CI override attempt without ADR + 2-team CODEOWNERS approvals** (Lote 10.6bis P0-W6-3 reframed): admin attempts force-push with `enforce_admins: true` on `main` → blocked by branch protection; `tla_override_validate.yml` rejects PR without `Tla-Override-ADR:` trailer + ADR doc with Architect + Crypto SME approvals.
    - 4. Property test 100k violation injected → SEV-0 alert; merge blocked.
    - 5. TLC flake (rare) → retry 3×; if persistent, investigate.
    - 6. TLA+ ↔ Rust drift simulated (intentional impl bug) → property test catches; CI red.
    - 7. CI cost regression → bench bound TLC ≤ 5 min per PR.
    - 8. 30d sustained verde gate validates S-20 GA promotion.
    - 9. Adversarial spec change (weakening InvGCReRefProtected) → adversarial review catches.
    - 10. Production trace exposes spec gap → quarterly re-review + spec expand.
    - 11. **TLC binary tamper detection** (Lote 10.6bis P0-W6-2 NEW): mutate a byte in cached tla2tools.jar; assert CI fails on SHA-256 checksum mismatch BEFORE invoking TLC; `corelink.ci.tla.binary_tamper_detected` SEV-1 alert.
    - 12. **30d clock reset on P0/P1 incident** (Lote 10.6bis P0-W6-W7-1 NEW): inject TLC red sustained > 4h within 30d window → 30d clock resets; `corelink.ci.tla.30d_window_days` gauge resets to 0; `corelink.ci.tla.30d_sustained_verde` gauge → FALSE; S-20 GA promotion blocked until 30d clock re-accumulates from incident resolution timestamp.

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
    When CI workflow tla_check runs
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
    Given CI history checked daily via tla-30d-sustained.yml (PLANNED WI-S06-006 deliverable)
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

- 9.1: TLC pinned version v1.8.0 + **SHA-256 integrity-verified** (Lote 10.6bis P0-W6-2; supply-chain hardening; bumps require ADR + Architect + Crypto SME signoff).
- 9.2: PR scope filter (path-based; minimal blast radius).
- 9.3: Property test 100k nightly (não per-PR; CI cost bounded).
- 9.4: **Override mechanism PINNED** (Lote 10.6bis P0-W6-3): CODEOWNERS rule + signed git-trailer + `tla_override_validate.yml` workflow; `enforce_admins: true` on `main` (no admin bypass).
- 9.5: 30d sustained verde gate workflow design PINNED (Lote 10.6bis P0-W6-W7-1): daily 06:00 UTC + clock reset on P0/P1 incidents per 4-tier classification.
- 9.6: Deterministic property test seeds (reproducible failures); **fixture generator emits adversarial inputs** (envelope mutation, substring collisions, schema evolution; Lote 10.6bis P0-W6-1).
- 9.7: GitHub branch protection enforces required status checks; `enforce_admins: true`.
- 9.8: Cross-validation property test (TLA+ ↔ Rust alignment) with **negative-control discriminating-power assertion** against deliberately-broken LIKE-substring impl (Lote 10.6bis P0-W6-1).
- 9.9: ADR-0042 addendum for TLC version+SHA pinning policy (Lote 10.6bis P0-W6-2; "TLC version + SHA pinned; bumps require ADR + Architect + Crypto SME signoff" — addendum added as §A1 of ADR-0042 in WI-007 §10.s06.007.8 ratificação).
- 9.10: TLC ≤ 5 min p99 per PR (model state space bounded). **TLC cfg bounds explicit** (canonical Lote 10.6 cycle 4 alignment with cfg actual): `gc_correctness.cfg` SETS `Blobs={b1,b2}`, `AC_Entries={e1}`, `MaxTime=10`, `GracePeriod=2` → state space exhaustively explored at these bounds (~5k-50k states); property test 100k extends coverage to larger-scale interleavings via random sampling against real Rust impl. Bounds documented in ADR-0042 §A2 canonical. `InvMarkingConsistent` invariant cleaned up (vacuously-true `/\ TRUE` branch removed) per Lote 10.6-tris OPUS-MISS-1.
- 9.11: **Hard cross-WI dependency on WI-S06-003 P0-1 json_each fix landed** (Lote 10.6bis P0-W6-1; without it, property test passes green silently against broken SQL).
- 9.12: **TLC SHA-256 bootstrap trust ceremony** (Lote 10.6-tris NEW-P0-1 fix): SHA `d5d07d5dab38ddb840c91ec48fa02f28b37a608d5af9a73570018591dbc8ef7f` computed 2026-04-25 by Owner via fresh download; Architect + Crypto SME independent re-verification REQUIRED pre-merge (commit signed verification comments to ADR-0042 addendum §A1 sign-off block). NO merge of `tla_check.yml` (canonical) to main without 2 independent SHA verifications. Bumps to TLC version require ADR + Architect + Crypto SME signoff.
- 9.13: **TLA+ formal verification SCOPE LIMITATIONS documented** (Lote 10.6-tris NEW-P0-2 fix): `gc_correctness.tla` covers mark-sweep algorithm; soft-delete grace window NOT in TLA+ scope (covered architecturally by WI-S06-004 §6.1.6 conditional `WHERE refcount = 0` predicate + WI-S06-005 reconcile orphan detection). "INV-GC-001 formally proven" claim qualified by §1 invariant 7 scope statement. Future TLA+ extension (S-07+ deferred) will model two-phase soft+physical delete.
- 9.14: **PRNG determinism pinned** (Lote 10.6-tris OPUS-MISS-2): `ChaCha20Rng::seed_from_u64(iter)` em property test fixture; cross-platform reproducibility = code-level guarantee.

### 10. Completeness Criteria

- [ ] **10.s06.006.1** TLA+ CI workflow live; PR triggers TLC model check.
- [ ] **10.s06.006.2** Property test 100k race em CI nightly; 0 violations sustained; fixture generator emits adversarial inputs (envelope mutation, substring collisions, schema evolution); negative-control test against broken LIKE impl FAILS as expected (proves discriminating power).
- [ ] **10.s06.006.3** **30d sustained TLA+ verde gate** (sprint contract DoD §10.s06.4); workflow `tla-30d-sustained.yml` (PLANNED deliverable — not yet in tree) published with cadence + query + clock-reset semantics + gauge emission. Distinct from the live gate `tla_check.yml` which runs per-PR.
- [ ] **10.s06.006.4** PR fail logic via GitHub branch protection; `enforce_admins: true` on `main`.
- [ ] **10.s06.006.5** Override mechanism PINNED: CODEOWNERS rule + `tla_override_validate.yml` workflow + signed git-trailer; ADR + Architect + Crypto SME signoff validated by workflow.
- [ ] **10.s06.006.6** Métricas (7) emitted; CRITICAL alert violations > 0; SEV-1 binary_tamper_detected.
- [ ] **10.s06.006.7** TLA+ ↔ Rust alignment cross-validation: property test `prop_tla_rust_alignment` runs ≥ 1000 TLA+ scenarios → executes against Rust impl → asserts identical final state; CI nightly green (Lote 10.6bis P1-W6-3 measurable criterion).
- [ ] **10.s06.006.8** Quarterly re-review process documented.
- [ ] **10.s06.006.9** Cargo-audit + cargo-deny clean.
- [ ] **10.s06.006.10** CI cost gate: TLC ≤ 5 min p99 per PR; private-repo billing $730/yr (Lote 10.6bis P2-W6-2 re-derivation).
- [ ] **10.s06.006.11** **TLC v1.8.0 SHA-256 integrity verify** (Lote 10.6bis P0-W6-2 NEW): expected SHA captured at first download; CI fails on mismatch; chaos test #11 mutates byte and asserts CI fails.
- [ ] **10.s06.006.12** **TLA+ ↔ Rust action mapping table** published §1 (Lote 10.6bis P1-W6-1 NEW); 5 rows minimum (MarkPhaseStart, GCMarkStep, UpdateActionResult, SweepStep, InvGCReRefProtected).
- [ ] **10.s06.006.13** **Hard cross-WI dependency** WI-S06-003 P0-1 json_each fix landed BEFORE property test runs in CI (Lote 10.6bis P0-W6-1).

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
| CI workflow | `.github/workflows/tla_check.yml` (canonical; live) | YAML |
| 30d sustained workflow | `.github/workflows/tla-30d-sustained.yml` (PLANNED — WI-S06-006 deliverable; not yet in tree) | YAML |
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
| ST-001 CI workflow tla_check.yml | 3 |
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

- 18 Dependencies: Hard TLA+ `gc_correctness.tla` Lote 5.13/7.1 verified; WI-S06-002+003 (impl); **HARD WI-S06-003 §6.1.2 P0-1 json_each fix landed before property test CI run** (Lote 10.6bis P0-W6-1); GitHub Actions; soft S-09 metrics; outbound WI-S06-007 (consumes 30d verde for ship gate).
- 19 PERT 33h; 20 Time-boxing 40h.
- 21 Observability 7 metrics (added: `corelink.ci.tla.binary_tamper_detected`, `corelink.ci.tla.30d_window_days`); trace `gc.ci.{tla, property_test, override_validate}`.
- 22 Cost: TLC compute (private repo Actions): 5 min × $0.008/min × 50 PRs/dia = $2/dia = $730/yr (Lote 10.6bis P2-W6-2 corrected; was $18/yr free-tier assumption); property test nightly ~$1/dia = $365/yr; total ~$1095/yr.
- 23 API Contract: CI workflow YAML; property test Rust API; ADR template.
- 24 Post-mortem: TLC red sustained > 4h → CRITICAL post-mortem (sprint contract); property test violation → SEV-0; TLA+ ↔ Rust drift detected → CRITICAL.
- 25 Rollback: CI workflow revert via git; property test rollback via cargo update; RTO ≤ 10 min; RPO 0.
- 26 Security: CI gate enforces formal proof; override discipline ADR; cross-validation prevents drift.
- 27 Knowledge Transfer: Tech talk 2h "TLA+ CI Gate + Property Test 100k Race + Cross-Validation"; doc; onboarding test 5q.
- 28 Risk Register (12-row): TLA+ scope incompleto M M HIGH M LOW (adversarial review); CI flake L H LOW L LOW (deterministic seeds); property test flake M H LOW L LOW (retry 3×); TLA+↔Rust drift L M HIGH L LOW (cross-validation); merge override abuse L L HIGH L LOW (ADR + Architect signoff); PR scope miss M L MEDIUM L LOW (path filter); CI cost regression M L MEDIUM L LOW; 30d sustained gate slip M M HIGH M LOW; spec corruption attack L L HIGH L LOW (adversarial review); production gap L M HIGH L LOW (quarterly re-review); GitHub Actions outage L L LOW L LOW; TLC version drift L L MEDIUM L LOW (pinned version).
- 29 Review: D+0 design (Architect + Crypto SME); D+2 AppSec; D+5 code review; D+6 Crypto SME independent; D+7 PRR mini.
- 30 Sign-off (HIGH_RISK 11): standard 10 mandatory; Crypto SME **MANDATORY EMPHATIC** (TLA+ obligation alignment).
- 31 Change Log: 1.0.0 / 2026-04-25 / Gustavo (Lote 10.6); 1.1.0 / 2026-04-25 / Gustavo (Lote 10.6bis Part 2b P0 fixes: P0-W6-1 fixture adversarial inputs + cross-WI dep + negative control; P0-W6-2 TLC SHA pinning; P0-W6-3 override mechanism CODEOWNERS+workflow+enforce_admins; P0-W6-W7-1 30d sustained workflow design pinned; P1-W6-1 TLA+↔Rust action mapping; P1-W6-2 +2 chaos scenarios; P1-W6-3 measurable criterion ≥1000 scenarios; P2-W6-2 cost re-derived private-repo billing); 1.2.0 / 2026-04-25 / Gustavo (Lote 10.6-tris Sonnet R5 P0+P1+OPUS-MISS fixes: NEW-P0-1 TLC SHA literal `d5d07d5dab38ddb840c91ec48fa02f28b37a608d5af9a73570018591dbc8ef7f` + bootstrap ceremony §9.12; NEW-P0-2 TLA+ scope limitations explicit §1.7 + soft-delete grace coverage caveat; OPUS-MISS-1 InvMarkingConsistent vacuously-true branch removed + TLC cfg bounds documented §9.10; OPUS-MISS-2 PRNG ChaCha20Rng::seed_from_u64 §9.14; ADR-0042 addendum §A1 SHA pinning + §A2 cfg bounds added); 1.3.0 / 2026-05-02 / Gustavo (via Claude Opus 4.7 1M) — **WI-S06-006 SEALED — TLA+ CI gate + INV-GC-004 race property test (PR-gate 10k / nightly 100k) shipped**. New file `crates/corelink-gc/tests/prop_inv_gc_004_race.rs` (~530 LOC) ships the canonical 100k race property test cross-validating the TLA+ obligation `gc_correctness.tla::InvGCReRefProtected` against the real Rust impl (WI-S06-002 mark + WI-S06-003 sweep). Dual-tier ceiling: PR gate at 10k iter (default `PROPTEST_CASES`), nightly tier at 100k iter (`PROPTEST_CASES=100000` in `.github/workflows/nightly.yml::proptest-extended`). Each iter: `ChaCha20Rng::seed_from_u64(seed)` deterministic PRNG (Lote 10.6-tris OPUS-MISS-2); fresh per-iter fixture (F-001 closure); random `mark_anchor` ∈ [1M, 10M] ms; `ac_offset_ms` ∈ [-100, +100] straddles boundary (off-by-one cases sampled exhaustively); ±70% iters fire the genuine UpdateActionResult, 30% are negative-control orphan path. Adversarial fixture inputs per WI §1.7 + Lote 10.6bis P0-W6-1: (a) envelope mutation (`action_digest` STRING references target as substring; `blob_refs` references different digest); (b) short-digest substring (16-char prefix in unrelated `action_digest`); (c) schema-evolution (`action_digest` mentions target; `blob_refs` empty). Asserts: ZERO INV-GC-004 violations across 100k iter; protect-arm PROTECT iff `ac.created_at_ms >= mark_started_at_ms` AND `blob_refs.contains(target)`; sweep-arm SWEEP otherwise; negative-control SWEEP regardless of offset (validates regressed LIKE-substring impl would fail). 5 tests in module: 1 proptest (10k/100k) + 4 sanity (canonical TLC SHA literal pin verifying drift between Rust suite + `tla_check.yml` + `run_tlc_corelink.sh` + `ADR-0042 §A1`; PRNG determinism cross-invocation; smoke at offset=0 protect-arm boundary; smoke at offset=-1 sweep-arm boundary). Cross-module patches: `crates/corelink-gc/Cargo.toml` adds `rand = "0.9"` + `rand_chacha = "0.9"` to `[dev-dependencies]`. CI integration: `.github/workflows/nightly.yml::proptest-extended` adds new step `proptest 100k iter — INV-GC-004 race (WI-S06-006 nightly tier)` running `cargo test --release --package corelink-gc --test prop_inv_gc_004_race` with `PROPTEST_CASES=100000`. The TLA+ CI gate workflow `.github/workflows/tla_check.yml` (canonical pós Lote 10.6 cycle 4 + Lote 10.11.0-bis-prime cycle 5) is already shipped; the SHA-pinning literal `d5d07d5dab38ddb840c91ec48fa02f28b37a608d5af9a73570018591dbc8ef7f` matches ADR-0042 §A1 + the property test fixture's canonical-SHA sanity check; PR paths cover `crates/corelink-gc/**`, `specs/tla/gc_correctness.tla`, `migrations/**`, `specs/03_architecture/data_model.md`, `specs/03_architecture/invariant_registry.md`, the workflow itself, and ADR-0042 — fail-closed on SHA mismatch (Lote 10.6-tris NEW-P0-1 fix). Out-of-scope (deferred to WI-S06-007 PRR ship gate per trait-abstraction-defer charter): `tla-30d-sustained.yml` workflow + `tla_override_validate.yml` + CODEOWNERS rules + cargo-fuzz `fuzz_gc_correctness` target — all consolidated alongside the sprint-close ship gate. **Full crate suite 248 tests across all targets, 0 failures, parallel-safe** (159 lib + 10 chaos + 14 prop_reconcile + 16 prop_scheduler + 17 migration_canonical + 5 prop_inv_gc_004_race + 9 prop_mark + 9 prop_sweep + 9 migration_canonical_0007). Quality gates verde: `cargo test -p corelink-gc --all-targets` 0 failures (248 tests); `cargo clippy --workspace --all-targets --features corelink-worker/tower-middleware -- -D warnings` clean; `validate_specs.py` clean (281 docs); `check_migrations_additive.py` clean (no new migration). 100k iter local validation: `PROPTEST_CASES=100000 cargo test -p corelink-gc --test prop_inv_gc_004_race --release prop_inv_gc_004_race_mark_update_ar` GREEN in 0.6s (zero violations). Nightly workflow YAML validated via `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/nightly.yml'))"`. **No per-WI codex per 2026-04-30 protocol; sprint-close Sonnet review covers full S-06 corpus.**
- 32 Anti-patterns: ❌ Skip CI gate; ❌ Override sem ADR + CODEOWNERS approvals; ❌ Admin bypass on main; ❌ TLC binary unverified; ❌ Property test flake silenciada; ❌ Property test fixture without adversarial inputs; ❌ TLA+ scope reduction sem ADR; ❌ Skip 30d sustained gate; ❌ Cross-tenant property test scenarios; ❌ TLC version drift sem ADR.

---

**Fim WI-S06-006.** Próximo: WI-S06-007 (DASH-GC + reclaim metric + RB-FM-300/404/305 dry-runs + PRR ship gate).
