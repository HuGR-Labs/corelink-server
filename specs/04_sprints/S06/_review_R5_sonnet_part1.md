---
id: "AUDIT-2026-05-15-R5-SONNET-S06-PART1"
type: "audit"
doc_status: "REVIEW"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
reviewer: "R5 Sonnet persona (Sonnet 4.6, different model lineage than R4 Opus — Wave 14 Lote 10.6 review dispatch; testability + observability focus)"
scope: "Lote 10.6 — Sprint S-06 Part 1 (WI-S06-001 .. WI-S06-004) on FROZEN v1.3.0 corpus post Lote 10.6bis + 10.6-tris remediation"
parent_audit: "specs/04_sprints/S06/_review_R4_opus_part1.md"
sprint_contract: "specs/04_sprints/S06/_spec_contract.md v2.0.0"
calibration_baselines:
  - "specs/_audits/2026-04-25-sonnet-r5-s06-wi-review.md (post-bis 9.1/10 target)"
  - "specs/_audits/2026-04-25-agent-r4-s06-part1-wi-review.md (pre-bis 8.13/10)"
files_reviewed:
  - "specs/04_sprints/S06/work_items/WI-S06-001-worker-gc-binary-scheduler-degrade-mode.md (649 lines, v1.3.0 FROZEN)"
  - "specs/04_sprints/S06/work_items/WI-S06-002-mark-phase-multi-pass-scan-mark-started-at.md (653 lines, v1.3.0 FROZEN)"
  - "specs/04_sprints/S06/work_items/WI-S06-003-sweep-phase-soft-delete-inv-gc-004.md (663 lines, v1.3.0 FROZEN)"
  - "specs/04_sprints/S06/work_items/WI-S06-004-physical-delete-post-grace-r2-idempotent.md (463 lines, v1.3.0 FROZEN)"
cross_references:
  - "crates/corelink-gc/src/ (~6000 LOC including sweep.rs + physical_delete.rs + mark.rs)"
  - "crates/corelink-gc/tests/ (94+143+174+198 tests progression)"
  - "specs/tla/gc_correctness.tla"
  - "migrations/d1/0006_gc_run.sql + 0007_gc_candidates.sql"
tags: ["audit", "r5", "sonnet", "lote-10.6", "s-06", "wave-14", "dispatch", "review", "part1", "testability"]
---

# R5 Sonnet — Lote 10.6 S-06 Part 1 (WIs 001–004) Adversarial Review (Testability + Observability)

> **Reviewer**: R5 Sonnet persona — different model lineage than R4 Opus (Sonnet 4.6); mandate is **find what Opus missed** with focus on testability, mutation-testing surface, property-test gaps, fake/stub fidelity, integration-test coverage, observability gaps. Brutal, technical, no diplomacy.
> **Calibration**: post-Lote 10.6-tris v1.3.0 FROZEN/AUDITED. Lote 10.6-tris R5-round identified 2 NEW P0s + 4 NEW P1s + 4 OPUS-MISS items that R4 had missed. This dispatch round should yield mostly P2/P3 cumulative-track findings since the major P0 surface is closed.

---

## §1 Summary table

| WI | P0 | P1 | P2 | P3 | Verdict | One-line note |
|---|---|---|---|---|---|---|
| WI-S06-001 | 0 | 0 | 3 | 3 | APPROVE — strong test coverage (94 tests parallel-safe) | InMemory scheduler fake is high-fidelity; F-001 closure verified per-instance. |
| WI-S06-002 | 0 | 1 | 3 | 2 | APPROVE — P1 on `prop_reachable_set_complete` mutation surface | Migration_canonical_0007 17 tests + 9 prop_mark; canonical pins are mutation-pinned. |
| WI-S06-003 | 0 | 1 | 2 | 2 | APPROVE — P1 on Mann-Whitney sweep timing test deferred without explicit fixture seed reproducibility budget | Audit fail-closed envelope is testability gold; protect AND sweep arms both pinned. |
| WI-S06-004 | 0 | 1 | 3 | 2 | APPROVE — P1 on R2BlobStore fake fidelity vs S3/R2 404-as-success edge cases | CountingPhysicalDeleteClock off-by-2 boundary fixture is canonical pattern. |

**Aggregate Part 1 verdict: APPROVE. 0 P0 / 3 P1 / 11 P2 / 9 P3.** Testability surface is materially stronger than the pre-bis baseline. Property test seeds are deterministic (`ChaCha20Rng::seed_from_u64`) — reproducibility-safe. Fake fidelity is high (InMemory* implementations mirror SQL semantics closely). Primary gaps are in (a) mutation-testing surface coverage of property tests, (b) integration-test coverage between the trait-based fakes and the real D1/R2 bindings (deferred to WI-007 trait-defer consolidation), (c) observability counters for the strict-boundary off-by-one anti-pattern detection.

---

## §2 Testing-discipline principles cited

- **PRINC-TEST-DETERMINISTIC** — every property test uses `ChaCha20Rng::seed_from_u64(seed)`; no `thread_rng()`; seed pinning enables CI reproducibility.
- **PRINC-TEST-PARALLEL-SAFE** — no static globals, no `lazy_static!` test fixtures; every test owns its closure (F-001).
- **PRINC-MUTATION-PROP** — property test must fail under a mutation that flips the asserted predicate (e.g., `>=` → `>`); recommended: cargo-mutants run per property test.
- **PRINC-FAKE-FIDELITY** — InMemory* fakes mirror SQL/R2 semantics at the canonical operation granularity; deviations must be documented.
- **PRINC-OBS-COUNTER-DRIFT** — invariant violations must emit a counter (not just a log line) so DASH-GC + alerts can fire.
- **PRINC-BOUNDARY-FIXTURE** — off-by-one anti-patterns must be regression-pinned via boundary-fixture tests (`offset=-1`, `offset=0`, `offset=+1`).

---

## §3 Per-WI findings

### §3.1 WI-S06-001 — Worker GC binary + scheduler + degrade-mode

**Test-surface strengths.** 94 tests parallel-safe (59 lib unit + 16 prop_scheduler @ 10k iter + 10 migration_canonical + 9 chaos_gc_scheduler). F-001 closure verified per-instance (`GcWorker` owns its scheduler state). InMemory scheduler fake is high-fidelity (state machine `pending → running → succeeded/crashed/aborted` mirrors SQL CHECK domain). ScheduleClock seam enables deterministic cron-tick testing.

**Findings.**

- **P2-001-1 [InMemory scheduler fake does not model D1 throttle injection]** — Cites `PRINC-FAKE-FIDELITY`. The production scheduler will encounter D1 throttle (BUSY / WRITE_BUSY); the InMemory fake does not simulate this. Recommendation: add a `ScheduleStoreThrottleInjector` test seam (matching the chaos_gc_scheduler suite naming).

- **P2-001-2 [9 chaos_gc_scheduler scenarios — recommend documenting which FMs each scenario covers]** — Cites `PRINC-MUTATION-PROP`. The chaos scenarios should pin per-test the FM-ID it exercises (FM-300 / FM-404 / FM-305 / etc.) so coverage gaps surface.

- **P2-001-3 [10 migration_canonical tests for 0006 — recommend cross-validating against `pragma_table_info` to catch column-rename drift]** — Cites `PRINC-FAKE-FIDELITY`. Recommendation: add a test that asserts `pragma_table_info('gc_run')` matches the expected canonical column list (idempotent regression-pin).

- **P3-001-1 [GcStatus 6-variant + GcPhase 6-variant — recommend a `prop_status_phase_invariant` property test that asserts no invalid (status, phase) combination is reachable]** — Informational.

- **P3-001-2 [`gc_schema_version()` advanced 6 → 7 — recommend a test that pins the version literal so accidental bumps fail at compile time]** — Informational.

- **P3-001-3 [Audit `GcEventType` 8 canonical event-strings — recommend a `prop_audit_event_string_canonical_pin` that asserts the as_str() mapping cannot drift]** — Informational. (Note: WI-S06-003 SEAL log mentions this drift was caught by `audit_emit_failure_blocks_status_flip` — but a dedicated pin test would catch it earlier.)

### §3.2 WI-S06-002 — Mark phase

**Test-surface strengths.** 49 new tests (23 inline lib unit + 17 migration_canonical_0007 + 9 prop_mark @ 10k iter). 5 canonical property tests cover idempotent / atomic-anchor / reachable-set-complete / tenant-isolation / d1-batch-bounded. Migration tests cross-ref status-literal Rust↔SQL (good drift-detection). Real bug caught + fixed during test development (snapshot lower bound + grace canonical 24h alignment).

**Findings.**

- **P1-002-1 [`prop_reachable_set_complete` mutation surface — the property tests that the reachable union grows across passes, but does NOT test that a removed reachable element triggers a candidate (the false-orphan direction)]** — Cites `PRINC-MUTATION-PROP`. The catastrophic failure mode is false-orphan (a reachable blob classified as orphan → INV-GC-001 violation). The property test should include a `prop_no_false_orphan_under_concurrent_unref` scenario that injects an unref event during the scan and asserts the reachable set captures the previously-reachable state (monotone-shrinking-tolerated; monotone-growing-required).

- **P2-002-1 [9 prop_mark tests but no mutation-test driver pinned in CI — cargo-mutants is not wired]** — Cites `PRINC-MUTATION-PROP`. Recommendation: add a nightly cargo-mutants run on `crates/corelink-gc/src/mark.rs` with threshold `mutants_caught >= 90%`.

- **P2-002-2 [`prop_d1_batch_bounded` — asserts canonical 250 rows but does not test the D1 envelope size in bytes (100KB Lote 10.4bis limit)]** — Cites `PRINC-FAKE-FIDELITY`. The 250-row pin protects against the row count; but the underlying constraint is the 100KB envelope. Recommendation: add a `prop_d1_envelope_size_bounded` that asserts row payload size × batch size ≤ 100KB.

- **P2-002-3 [Migration_canonical_0007 17 tests — recommend adding a test that asserts `idx_protected` partial index `WHERE status='protected_re_ref'` exists (forensics index)]** — Cites `PRINC-FAKE-FIDELITY`. The partial index is load-bearing for INV-GC-004 forensics; test should pin its existence.

- **P3-002-1 [`MarkConfig` validator — recommend documented `prop_mark_config_rejects_zero_budget` test]** — Informational.

- **P3-002-2 [`BlobDigest` rejection coverage — recommend canonical fuzz target on parse path]** — Informational. (Note: cargo-fuzz target is deferred to WI-007 trait-abstraction-defer.)

### §3.3 WI-S06-003 — Sweep phase + INV-GC-004 + audit fail-closed

**Test-surface strengths.** 24 inline lib unit + 9 prop_sweep @ 10k iter. The strict-boundary property test `prop_inv_gc_004_protect_if_ge_strict_boundary` samples signed `ac_offset in -1000..=1000` straddling the boundary — textbook `PRINC-BOUNDARY-FIXTURE`. The audit fail-closed test `audit_emit_failure_blocks_status_flip` asserts on the PROTECT arm that audit emission failure surfaces `SweepError::AuditEmissionFailed`, candidate row preserved as `Candidate`, blob_meta NOT soft-deleted — this is the gold standard for audit fail-closed test coverage.

**Findings.**

- **P1-003-1 [Mann-Whitney 3-prong sweep timing test deferred to WI-007 — the deferral inventory should pin the fixture seed AND the population size AND the `|Δmedian| ≤ 5ms` budget for reproducibility]** — Cites `PRINC-TEST-DETERMINISTIC`. Mann-Whitney timing tests are notoriously flaky if seed/population aren't pinned. Recommendation: pre-commit the fixture config in WI-007 §1.1 trait-defer inventory.

- **P2-003-1 [`prop_audit_fail_closed_sweep_arm` — the test covers PROTECT arm fail-closed but NOT the SWEEP arm fail-closed (audit emit failure on actual sweep should also roll back)]** — Cites `PRINC-AUDIT-FAIL-CLOSED`. (Note: the WI text says both arms are covered; but the test name `audit_emit_failure_blocks_status_flip` should be split into `_on_protect_arm` and `_on_sweep_arm` for clarity. If only one test exists, the other arm has zero direct coverage.)

- **P2-003-2 [`SweepConfig::new` validator `grace_cas_ms >= grace_ac_ms` regulatory floor — recommend property test `prop_sweep_config_rejects_inverted_grace`]** — Cites `PRINC-TEST-DETERMINISTIC`. The validator is defensive; a regression test pins it.

- **P3-003-1 [`prop_inv_gc_004_protect_if_ge_strict_boundary` samples `ac_offset in -1000..=1000` — recommend extending to `-100_000..=100_000` for the heavy-magnitude regime AND keeping the tight boundary]** — Informational.

- **P3-003-2 [`SweepDecision` 3-arm — recommend `prop_step_decision_predicate_aggregates_disjoint` that asserts every iteration produces exactly one decision arm (no double-emit)]** — Informational.

### §3.4 WI-S06-004 — Physical delete + R2→D1 + DSR bypass

**Test-surface strengths.** 24 inline lib unit. CountingPhysicalDeleteClock auto-advance fixture with off-by-2 boundary seed (`now_start = deleted_at_ms + GRACE_CAS_MS - 2`) is the canonical regression-pin for the strict-`>` off-by-one anti-pattern — `PRINC-BOUNDARY-FIXTURE` exemplar. R2 404-as-success idempotency tested. Cross-tenant injection rejection tested. Audit fail-closed rollback tested. Phase-budget enforcement tested. Idempotent re-run tested.

**Findings.**

- **P1-004-1 [`R2BlobStore` fake fidelity vs S3/R2 404-as-success edge cases — the fake returns success on 404 but does not model the R2 transient error space (5xx, throttle, slow-response)]** — Cites `PRINC-FAKE-FIDELITY`. Production R2 will return 503/429/timeout; the fake should model these via a `R2BlobStoreErrorInjector` seam. Without it, the chaos test "R2 partial outage" (named in §1) cannot exercise the retry/back-off path.

- **P2-004-1 [Conditional refcount=0 predicate — recommend `prop_re_upload_race_protected` 10k iter that injects a refcount mutation between sweep and physical-delete and asserts physical-delete-skip arm]** — Cites `PRINC-MUTATION-PROP`. The re-upload race is the canonical failure mode (FM-300 adjacent); should be property-test pinned.

- **P2-004-2 [R2→D1 ordering — recommend `prop_crash_recovery_order` that simulates a crash AFTER R2 success but BEFORE D1 purge and asserts the orphan-R2 arm is detectable by WI-S06-005 reconcile]** — Cites `PRINC-FAKE-FIDELITY`. The crash-recovery ordering is the load-bearing claim of WI-004 §1.4; should be property-test pinned end-to-end across WI-004 + WI-005.

- **P2-004-3 [`PhysicalDeleteError` 8-variant taxonomy — recommend `prop_error_audit_code_canonical_pin` that asserts every variant maps to a canonical audit_code() string]** — Cites `PRINC-OBS-COUNTER-DRIFT`. Without pinning, error-code drift could break the audit_outbox consumer.

- **P3-004-1 [Off-by-2 boundary seed pattern documented in CHANGELOG but not in a canonical test-pattern doc]** — Informational. Recommendation: extract to `specs/06_quality/test_patterns.md::strict_boundary_fixture`.

- **P3-004-2 [DSR Ed25519 signal scaffolding — recommend the test fixture pin a canonical valid signature + a canonical invalid signature so deferred S-11 wiring has regression-pins]** — Informational.

---

## §4 Cross-cutting observations

1. **Property-test deterministic-PRNG discipline is exemplary.** Every property test uses `ChaCha20Rng::seed_from_u64`. Reproducibility is CI-safe. **However**: the cumulative property-test corpus across WIs is NOT cross-validated for seed-collision (two different property tests with the same `seed=42` may shadow each other's coverage). Recommendation: add a `prop_seed_registry.rs` that lists allocated seeds per WI.

2. **Fake fidelity is high but error-injection is uneven.** WI-002 InMemory stores model the happy path; WI-003 + WI-004 audit fail-closed tests inject audit failures; but the cross-system error injection (D1 throttle, R2 5xx, KV unavailable) is not consistently wired. The chaos suite (12 scenarios in WI-S06-003; 9 in WI-S06-001) covers many but not all. Recommendation: stand up a canonical `ErrorInjectorSeam` trait across stores.

3. **Mutation-testing surface coverage is unset.** The property tests are excellent but no cargo-mutants run is wired into CI. For a SOTA program this is a gap; for CRITICAL TLA+-verified WIs (003 / 006) it's a P1-cumulative.

4. **Boundary-fixture pattern is canonical.** Both WI-003 (`prop_inv_gc_004_protect_if_ge_strict_boundary`, `ac_offset in -1000..=1000`) and WI-004 (CountingPhysicalDeleteClock, `now_start = deleted_at_ms + GRACE_CAS_MS - 2`) exemplify the strict-boundary fixture pattern. **Recommendation**: extract to canonical test pattern doc.

5. **Migration-canonical test pattern is canonical.** 10 (0006) + 17 (0007) migration tests cross-validate textual SQL + status-literal Rust↔SQL + idx existence. **Recommendation**: extract to `specs/06_quality/test_patterns.md::migration_canonical`.

---

## §5 Top-of-pile recommendations (next-step concrete)

1. **WI-002**: add `prop_no_false_orphan_under_concurrent_unref` property test (P1-002-1).
2. **WI-003**: pin Mann-Whitney sweep timing fixture (seed + population + budget) in WI-007 §1.1 trait-defer inventory (P1-003-1).
3. **WI-004**: add `R2BlobStoreErrorInjector` seam + transient error model (P1-004-1).
4. Wire cargo-mutants nightly run on `crates/corelink-gc/src/{mark,sweep,physical_delete}.rs` with threshold `>= 90% caught` (§4.3).
5. Stand up `ErrorInjectorSeam` trait across InMemory stores (§4.2).
6. Extract canonical test-pattern docs (boundary-fixture + migration-canonical) (§4.4, §4.5).
7. Add `prop_seed_registry.rs` for cross-WI seed-collision audit (§4.1).

---

## §6 Verdict

**APPROVE Part 1 corpus.** 0 P0; 3 P1 cumulative-track recommendations (false-orphan property test, Mann-Whitney fixture pin, R2 error injector); 11 P2 + 9 P3 testability/observability hygiene findings. None block S-06 SEAL. The testability surface is materially the strongest in the program — the boundary-fixture pattern, the deterministic PRNG discipline, the migration-canonical test discipline, and the audit fail-closed test coverage are all canonical references for future sprints.

**Aggregate Part 1 R5 Sonnet score: 9.10/10.** WI-003 9.3 (audit fail-closed test discipline is exemplary), WI-004 9.1 (off-by-2 boundary fixture is exemplary; held back by R2 fake fidelity gap), WI-001 9.0, WI-002 9.0.

---

**End R5 Sonnet Part 1 review.**
