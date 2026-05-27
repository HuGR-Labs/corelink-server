---
id: "AUDIT-2026-05-15-R5-SONNET-S06-PART2"
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
scope: "Lote 10.6 — Sprint S-06 Part 2 (WI-S06-005 .. WI-S06-007) on FROZEN v1.3.0 corpus post Lote 10.6bis + 10.6-tris remediation"
parent_audit: "specs/04_sprints/_sealed/S06/_review_R4_opus_part2.md"
sprint_contract: "specs/04_sprints/_sealed/S06/_spec_contract.md v2.0.0"
calibration_baselines:
  - "specs/_audits/sealed/2026-04-25-sonnet-r5-s06-wi-review.md (post-bis 9.1/10 target)"
  - "specs/_audits/sealed/2026-04-25-agent-r4-s06-part2a-wi-review.md (pre-bis 8.0/10)"
  - "specs/_audits/sealed/2026-04-25-agent-r4-s06-part2b-wi-review.md (pre-bis 8.0/10)"
files_reviewed:
  - "specs/04_sprints/_sealed/S06/work_items/WI-S06-005-refcount-reconciliation-auto-fix.md (393 lines, v1.3.0 FROZEN)"
  - "specs/04_sprints/_sealed/S06/work_items/WI-S06-006-tla-ci-gate-property-test-100k-race.md (517 lines, v1.3.0 FROZEN)"
  - "specs/04_sprints/_sealed/S06/work_items/WI-S06-007-dash-gc-rb-dry-runs-prr-ship-gate.md (642 lines, v1.3.0 FROZEN)"
cross_references:
  - "crates/corelink-gc/src/reconcile.rs (~1700 LOC + tests)"
  - "crates/corelink-gc/tests/prop_inv_gc_004_race.rs (~530 LOC; 10k PR / 100k nightly)"
  - "dashboards/grafana/DASH-GC.json (10 panels)"
  - "dashboards/alerts/dash-gc-alerts.yml (11 rules, 4-tier SEV)"
  - "scripts/rb_fm_{300,404,305}_dry_run.sh"
  - "specs/_audits/sealed/2026-05-02-pentest-s06-internal.md (zero HIGH/CRITICAL)"
  - "specs/_audits/sealed/2026-05-02-adversarial-s06.md (40 scenarios)"
  - ".github/workflows/tla_check.yml + nightly.yml + gc-ship-gate.yml"
tags: ["audit", "r5", "sonnet", "lote-10.6", "s-06", "wave-14", "dispatch", "review", "part2", "testability", "observability"]
---

# R5 Sonnet — Lote 10.6 S-06 Part 2 (WIs 005–007) Adversarial Review (Testability + Observability)

> **Reviewer**: R5 Sonnet persona — different lineage from R4; mandate `find what Opus missed` with focus on integration-test coverage, observability gaps, test seam fidelity, mutation testing, chaos coverage, dashboard alert correctness.
> **Calibration**: post-Lote 10.6-tris v1.3.0 FROZEN/AUDITED. Promotion decision STAGING-STABLE. 40 cumulative adversarial scenarios shipped (`2026-05-02-adversarial-s06.md`); internal pentest zero HIGH/CRITICAL (`2026-05-02-pentest-s06-internal.md`).

---

## §1 Summary table

| WI | P0 | P1 | P2 | P3 | Verdict | One-line note |
|---|---|---|---|---|---|---|
| WI-S06-005 (reconcile + dual-condition gate) | 0 | 1 | 3 | 2 | APPROVE — P1 on integration-test crossing WI-004 R2→D1 ordering arm × WI-005 orphan-R2 detection arm | 14 prop_reconcile tests including `prop_json_each_semantics_not_like` (LIKE-defect regression pin); audit fail-closed on all 3 mutation arms. |
| WI-S06-006 (TLA+ CI gate + 100k race) | 0 | 1 | 3 | 2 | APPROVE — P1 on RB-FM dry-run scripts using host-side harness (no CI integration assertions) | TLC SHA pin literal cross-validated across 4 anchor points; PRNG `ChaCha20Rng::seed_from_u64` deterministic. |
| WI-S06-007 (DASH-GC + RB dry-runs + PRR) | 0 | 2 | 4 | 3 | APPROVE — P1 on alert rule SEV-0 INV-GC-001 detection AND P1 on customer-visible reclaim metric tenant-isolation test | 10 panels + 11 alert rules + 3 RB dry-run scripts; ship-gate workflow fan-in. |

**Aggregate Part 2 verdict: APPROVE. 0 P0 / 4 P1 / 10 P2 / 7 P3.** Observability surface is materially solid (10 DASH-GC panels + 11 alert rules + 4-tier SEV classification). Primary gaps are (a) cross-WI integration test coverage for the R2→D1 ↔ orphan-R2 detection arm, (b) RB-FM-{300,404,305} dry-run scripts ship as bash harnesses without CI failure-mode assertions, (c) the SEV-0 alert for INV-GC-001 violation in production is implicit but not explicitly enumerated, (d) customer-visible reclaim metric tenant-isolation is not test-pinned.

---

## §2 Testing-discipline principles cited

- **PRINC-TEST-DETERMINISTIC** — deterministic PRNG; reproducible seeds.
- **PRINC-INTEGRATION-CROSS-WI** — cross-WI integration tests required when one WI's invariant is enforced by another WI's mechanism (e.g., WI-004 ordering ↔ WI-005 orphan detection).
- **PRINC-ALERT-FIRE-TEST** — alert rules require synthetic test fixtures that fire the alert in dry-run mode.
- **PRINC-OBS-COUNTER-DRIFT** — invariant violations must emit counters distinct from logs.
- **PRINC-CHAOS-FM-MAPPING** — every FM listed in `failure_modes.md` must map to at least one chaos test scenario.
- **PRINC-RB-DRY-RUN-ASSERTION** — RB dry-run scripts must assert on success/failure observable signal (exit-code + log-string + audit row), not just exit-code.
- **PRINC-PRR-COVERAGE** — PRR sign-off matrix is the load-bearing trust ceremony; each row must be auditable per-reviewer with conflict-of-interest screen.

---

## §3 Per-WI findings

### §3.1 WI-S06-005 — Daily refcount reconciliation + dual-condition auto-fix

**Test-surface strengths.** 30 inline lib unit + 14 prop_reconcile @ 10k iter (9 canonical proptest + 5 sanity). The canonical coverage: `prop_no_drift_no_mutation` (idempotent) + `prop_auto_fix_bounded_dual_condition` (gate boundary tests) + `prop_json_each_semantics_not_like` (LIKE-defect regression pin) + `prop_tenant_isolation` + `prop_step_decision_aggregates` + `prop_audit_emit_per_decision_arm` + `prop_snapshot_bound_excludes_post_snapshot_writes` + `prop_auto_fix_scale_invariant` (10/100/1k/10k scale) + `prop_idempotent_re_run`. Heavy-scale tests cap at 256 iter for PR-gate runtime budget (clean engineering trade-off). 100k nightly variant wired alongside WI-006.

**Findings.**

- **P1-005-1 [Integration-test crossing WI-004 R2→D1 ordering arm × WI-005 orphan-R2 detection arm — no end-to-end test exists that drives WI-004 mid-flight crash + WI-005 reconcile and asserts orphan-R2 detection fires]** — Cites `PRINC-INTEGRATION-CROSS-WI`. WI-004 ordering (R2 first, then D1) is the load-bearing claim; WI-005 orphan-R2 detection is the reconcile arm. The contract is cross-WI: if R2 succeeds then crash before D1 purge → reconcile arm fires `OrphanR2Detected`. No unified test exists across the WI boundary. Recommendation: stand up `tests/integration_orphan_r2_detection.rs` that drives this end-to-end.

- **P2-005-1 [`prop_audit_emit_per_decision_arm` — coverage that emit happens per arm, but NOT that emit-failure on each arm rolls back per arm separately]** — Cites `PRINC-AUDIT-FAIL-CLOSED`. The WI-005 audit fail-closed envelope is claimed; the test should cover emit-failure × 5 arms (NoDrift / AutoFixed / PausedForManualReview / SkippedSoftDeleted / OrphanR2Detected). NoDrift arm doesn't mutate so emit-failure rollback isn't load-bearing; AutoFixed + PausedForManualReview + OrphanR2Detected ARE.

- **P2-005-2 [`SevLevel::Sev1` per-tenant > 1% drift — no synthetic alert-firing test in `dash-gc-alerts.yml`]** — Cites `PRINC-ALERT-FIRE-TEST`. Alert rules ship with thresholds but no synthetic fixture that drives drift > 1% and asserts the SEV-1 page fires.

- **P2-005-3 [`prop_auto_fix_scale_invariant` runs at 10/100/1k/10k blob scales — should run at 100k + 1M scales too (per-tenant SOTA tenants are 1M-blob class)]** — Cites `PRINC-TEST-DETERMINISTIC`. Recommendation: nightly tier 1M scale variant.

- **P3-005-1 [`ReconcileConfig::AUTO_FIX_MAX_RECORDS = 5` — recommend documenting decision rationale in a test comment + cite Part 2a P0-6 fix origin]** — Informational.

- **P3-005-2 [Orphan-R2 detection defers reclaim to S-09 — recommend the audit emit field-set is pre-validated against the S-09 reclaim consumer schema]** — Informational.

### §3.2 WI-S06-006 — TLA+ CI gate + 100k race property test

**Test-surface strengths.** Property test `prop_inv_gc_004_race.rs` (~530 LOC) ships with dual-tier (10k PR / 100k nightly). PRNG `ChaCha20Rng::seed_from_u64(seed)` deterministic. Adversarial fixture inputs (envelope mutation + short-digest substring + schema-evolution) cover the regression direction. 5 tests: 1 proptest + 4 sanity (canonical TLC SHA pin verification + PRNG determinism cross-invocation + smoke at offset=0 protect-arm boundary + smoke at offset=-1 sweep-arm boundary).

**Findings.**

- **P1-006-1 [RB-FM-{300,404,305} dry-run scripts ship as host-side bash harnesses with `cargo-driven drift-detectable` claim but the CI integration is missing — there is no GH Actions workflow that runs these scripts on a sustained-staging trigger]** — Cites `PRINC-RB-DRY-RUN-ASSERTION`. The scripts exit 0 locally; the spec contract DoD §6 says "RB-FM-300/404/305 dry-run executado em staging (EVT-017)" — but staging-execution is currently a manual-run posture. Recommendation: wire scripts into `gc-ship-gate.yml` or a separate `rb-dry-run-monthly.yml` workflow.

- **P2-006-1 [`prop_inv_gc_004_race_mark_update_ar` — 100k iter local validation says GREEN in 0.6s; recommend a CI step that pins this performance budget (assertion: `100k iter completes < 60s` to catch perf regressions)]** — Cites `PRINC-OBS-COUNTER-DRIFT`. Test-suite runtime regression is a leading indicator of impl regression.

- **P2-006-2 [Adversarial fixture inputs (envelope mutation + short-digest substring + schema-evolution) — the test fires but does NOT assert per-adversarial-input the count of UpdateAR firings; without that, a regression where the adversarial input silently is filtered would not be caught]** — Cites `PRINC-MUTATION-PROP`. Recommendation: add per-fixture-input counter assertions.

- **P2-006-3 [TLC SHA pin literal cross-validated across 4 anchor points — recommend `validate_tlc_sha_drift.py` CI script that lint-greps the literal across all 4 files]** — Cites `PRINC-OBS-COUNTER-DRIFT`. Manual drift will eventually leak.

- **P3-006-1 [4 sanity tests (PRNG determinism + smoke boundaries + TLC SHA pin) — recommend extracting to canonical sanity-fixture template]** — Informational.

- **P3-006-2 [TLA+ counterexample (when TLC fails) — recommend auto-attach trace to failing PR]** — Informational.

### §3.3 WI-S06-007 — DASH-GC + RB dry-runs + PRR ship gate

**Test-surface strengths.** 10 panels in `DASH-GC.json`; 11 alert rules in `dash-gc-alerts.yml` with 4-tier SEV classification; 3 RB-FM dry-run scripts ship; PRR matrix with 11 sign-offs; ASVS checklist 44 PASS / 3 WAIVED / 13 N/A; internal pentest zero HIGH/CRITICAL across 6 attack surfaces; cumulative adversarial scenarios 40 across WIs 001–006; `gc-ship-gate.yml` workflow fan-in. JSON + YAML parse smoke clean.

**Findings.**

- **P1-007-1 [Alert rule SEV-0 for INV-GC-001 violation in production — not explicitly enumerated in `dash-gc-alerts.yml` 11 rules]** — Cites `PRINC-OBS-COUNTER-DRIFT`. Sprint contract §18 says "INV-GC-001 violation detected → CRITICAL post-mortem + customer notification + potential ANPD/DPC notification". This is the highest-severity trigger in the program; without an explicit SEV-0 alert rule firing on `inv_gc_001_violation_total > 0`, the trigger is buried in implicit metric watching. Recommendation: add rule SEV-0-RULE-1 to `dash-gc-alerts.yml`.

- **P1-007-2 [Customer-visible `bytes_reclaimed_last_30d` business metric — published per WI-007 + S-16 forward — no tenant-isolation test pinned]** — Cites `PRINC-INTEGRATION-CROSS-WI`. Cross-tenant aggregation must not leak; recommendation: `prop_bytes_reclaimed_tenant_isolation` 10k iter.

- **P2-007-1 [10 DASH-GC panels — none explicitly track `auto_fix_invocation_rate_per_tenant`]** — Cites `PRINC-OBS-COUNTER-DRIFT`. Auto-fix is a security-adjacent mutation; operator visibility on its rate is load-bearing.

- **P2-007-2 [11 alert rules — synthetic alert-firing tests (driving each rule in a fixture and asserting page) are not enumerated]** — Cites `PRINC-ALERT-FIRE-TEST`. Recommendation: stand up `tests/alert_fire_synthetic.rs` exercising each rule end-to-end.

- **P2-007-3 [3 RB-FM dry-run scripts — chaos magnitudes pinned (0.5% drift / `mark_started_at_ms+1ms` / 7d cron-disabled + 100 GiB orphan) but no per-script success assertion beyond exit-code]** — Cites `PRINC-RB-DRY-RUN-ASSERTION`. Recommendation: each script asserts on (a) audit row count, (b) drift detection counter, (c) reconcile arm fire count.

- **P2-007-4 [Adversarial 40 scenarios — `2026-05-02-adversarial-s06.md` enumerates but no automated regression rerun on a quarterly basis]** — Cites `PRINC-CHAOS-FM-MAPPING`. Recommendation: a `quarterly-adversarial-rerun.yml` workflow.

- **P3-007-1 [PRR-S06.md 11 sign-offs — recommend the matrix is `validate_prr_signoff.py`-enforced]** — Informational. (Note: `validate_prr_signoff.py` is in the trait-defer-deferred list per WI-007 spec — flag this as a leading indicator for the S-20 gate.)

- **P3-007-2 [`gc-ship-gate.yml` aggregate fan-in — recommend the fan-in includes `validate_specs.py` + `validate_inv_promotion.py` + `check_migrations_additive.py` + JSON/YAML parse smoke for DASH-GC.json + dash-gc-alerts.yml — list each step explicitly in the workflow]** — Informational.

- **P3-007-3 [ASVS 44 PASS / 3 WAIVED-S-19 / 13 N/A — recommend the 3 WAIVED rows pin the S-19 forward-ticket IDs explicitly]** — Informational.

---

## §4 Cross-cutting observations

1. **Cross-WI integration test gap.** The most consequential observability gap is the cross-WI integration test that drives WI-004 mid-flight crash + WI-005 reconcile orphan-R2 detection. Unit-level coverage is excellent but the WI boundary is where impl bugs hide. **Recommendation**: stand up `tests/integration/` directory with cross-WI fixtures.

2. **RB-FM dry-run scripts are host-side without CI assertion.** The 3 scripts ship and exit 0 locally, but the staging-execution gate (sprint contract §6 DoD) is currently a manual posture. **Recommendation**: nightly + monthly CI integration via a `rb-dry-run-monthly.yml` workflow with assertion on audit-row + drift-counter + reconcile-arm-fire signals.

3. **Alert rule synthetic-fire coverage.** 11 alert rules ship but none have synthetic-fire fixtures asserting the page actually fires under load. The SEV-0 INV-GC-001-violation alert is missing entirely. **Recommendation**: alert-fire synthetic test suite + add SEV-0 rule.

4. **Customer-visible metric privacy boundary.** `bytes_reclaimed_last_30d` is published per-tenant; cross-tenant leak surface is not test-pinned. **Recommendation**: `prop_bytes_reclaimed_tenant_isolation`.

5. **Mutation-testing surface unset (also flagged in Part 1 §4.3).** Same recommendation applies to reconcile + race property test: cargo-mutants nightly.

6. **TLC SHA drift detection is canonical-by-design but not validator-enforced.** 4 anchor points (workflow + run_tlc + ADR + property fixture) — needs a `validate_tlc_sha_drift.py` script.

---

## §5 Top-of-pile recommendations (next-step concrete)

1. **WI-005**: stand up `tests/integration_orphan_r2_detection.rs` (P1-005-1).
2. **WI-006**: wire RB-FM dry-run scripts into `rb-dry-run-monthly.yml` workflow (P1-006-1).
3. **WI-007**: add SEV-0 alert rule for `inv_gc_001_violation_total > 0` (P1-007-1).
4. **WI-007**: add `prop_bytes_reclaimed_tenant_isolation` 10k iter (P1-007-2).
5. **WI-007**: add `auto_fix_invocation_rate_per_tenant` panel to DASH-GC (P2-007-1).
6. **WI-007**: stand up alert-fire synthetic test suite (P2-007-2).
7. Wire cargo-mutants nightly + `validate_tlc_sha_drift.py` lint (§4.5, §4.6).

---

## §6 Verdict

**APPROVE Part 2 corpus.** 0 P0; 4 P1 cumulative-track recommendations (cross-WI integration, RB CI integration, SEV-0 alert, tenant isolation test); 10 P2 + 7 P3 observability hygiene findings. None block S-06 SEAL (corpus is FROZEN AUDITED + STAGING-STABLE). The R5 testability + observability surface confirms the R4 Opus structural verdict: corpus is at 9.0+ SOTA target. The 30d sustained gates (S-20 GA consumption) need the queryable counters + synthetic alert-fire fixtures + cross-WI integration tests landed before S-20 can ratify.

**Aggregate Part 2 R5 Sonnet score: 9.07/10.** WI-005 9.2 (testability is exemplary: 14 prop_reconcile + dual-condition + LIKE-defect pin), WI-006 9.1 (TLA+ ↔ Rust cross-validation + TLC SHA pin discipline), WI-007 8.9 (held back by alert-fire synthetic coverage gap + RB CI integration gap + SEV-0 alert rule missing).

**Aggregate S-06 R5 Sonnet score across Part 1 + Part 2 (7 WIs): 9.09/10.** Lote 10.6-tris projection (9.1) substantially ratified; cumulative-track recommendations (~7 P1s across the two reviews) are all non-blocking for S-06 SEAL but are MUST-FIX before S-20 GA gate consumption of the 30d sustained observation window.

---

**End R5 Sonnet Part 2 review.**
