---
id: "AUDIT-2026-05-15-R4-OPUS-S06-PART2"
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
reviewer: "R4 Opus persona (Claude Opus 4.7, 1M ctx, adversarial independent reviewer — Wave 14 Lote 10.6 review dispatch)"
scope: "Lote 10.6 — Sprint S-06 Part 2 (WI-S06-005 .. WI-S06-007) on FROZEN v1.3.0 corpus post Lote 10.6bis + 10.6-tris remediation"
sprint_contract: "specs/04_sprints/_sealed/S06/_spec_contract.md v2.0.0"
calibration_baselines:
  - "specs/_audits/sealed/2026-04-25-agent-r4-s06-part2a-wi-review.md (S-06 part2a pre-bis 8.0/10)"
  - "specs/_audits/sealed/2026-04-25-agent-r4-s06-part2b-wi-review.md (S-06 part2b pre-bis 8.0/10)"
  - "specs/_audits/sealed/2026-04-25-sonnet-r5-s06-wi-review.md (post-bis 9.1/10 target)"
  - "WI-S04-003 best-in-class 8.6"
files_reviewed:
  - "specs/04_sprints/_sealed/S06/work_items/WI-S06-005-refcount-reconciliation-auto-fix.md (393 lines, v1.3.0 FROZEN)"
  - "specs/04_sprints/_sealed/S06/work_items/WI-S06-006-tla-ci-gate-property-test-100k-race.md (517 lines, v1.3.0 FROZEN)"
  - "specs/04_sprints/_sealed/S06/work_items/WI-S06-007-dash-gc-rb-dry-runs-prr-ship-gate.md (642 lines, v1.3.0 FROZEN)"
cross_references:
  - "specs/04_sprints/_sealed/S06/_spec_contract.md v2.0.0 (FROZEN AUDITED)"
  - "specs/04_sprints/_sealed/S06/PRR-S06.md"
  - "specs/04_sprints/_sealed/S06/asvs-v5-v6-v8-v10-v14-checklist.md (44 PASS / 3 WAIVED / 13 N/A)"
  - "specs/tla/gc_correctness.tla"
  - "specs/03_architecture/adrs/ADR-0042 (TLC SHA + scope limitations)"
  - "dashboards/grafana/DASH-GC.json"
  - "dashboards/alerts/dash-gc-alerts.yml"
  - ".github/workflows/tla_check.yml"
  - ".github/workflows/nightly.yml"
  - ".github/workflows/gc-ship-gate.yml"
tags: ["audit", "r4", "opus", "lote-10.6", "s-06", "wave-14", "dispatch", "review", "part2"]
references:
  - "specs/04_sprints/_sealed/S06/_spec_contract.md"
  - "specs/04_sprints/_sealed/S06/PRR-S06.md"
  - "specs/04_sprints/_sealed/S06/asvs-v5-v6-v8-v10-v14-checklist.md"
  - "specs/04_sprints/_sealed/S06/work_items/WI-S06-005-refcount-reconciliation-auto-fix.md"
  - "specs/04_sprints/_sealed/S06/work_items/WI-S06-006-tla-ci-gate-property-test-100k-race.md"
  - "specs/04_sprints/_sealed/S06/work_items/WI-S06-007-dash-gc-rb-dry-runs-prr-ship-gate.md"
---

# R4 Opus — Lote 10.6 S-06 Part 2 (WIs 005–007) Adversarial Review

> **Reviewer**: R4 Opus persona — adversarial independent reviewer, no diplomacy, framework-principle-citation discipline.
> **Calibration**: Post-Lote 10.6-tris v1.3.0 FROZEN/AUDITED corpus. The PRR-S06 sign-off matrix is 3 ✅ + 8 ⚠️ WAIVED ADR-0034 (dual-hat). The asvs-v5-v6-v8-v10-v14 checklist is 44 PASS / 3 WAIVED-S-19 / 13 N/A. RB-FM-{300,404,305} host-side dry-runs all exit 0. **Promotion decision STAGING-STABLE** (not yet GA — that gate is S-20 with the 30d sustained chaos + TLA+ verde gate).

---

## §1 Summary table

| WI | P0 | P1 | P2 | P3 | Verdict | One-line note |
|---|---|---|---|---|---|---|
| WI-S06-005 (refcount reconcile + auto-fix) | 0 | 1 | 3 | 2 | APPROVE — P1 on snapshot-bound + dual-condition auto-fix interaction at scale-invariant boundary | `prop_json_each_semantics_not_like` regression-pins the LIKE defect; dual-condition gate boundary tests strong. |
| WI-S06-006 (TLA+ CI gate + 100k race property) | 0 | 1 | 2 | 2 | APPROVE — P1 on 30d sustained TLA+ verde gate observability | TLC SHA literal `d5d07d5dab38ddb840c91ec48fa02f28b37a608d5af9a73570018591dbc8ef7f` pinned across 4 anchor points; canonical drift-detection. |
| WI-S06-007 (DASH-GC + RB dry-runs + PRR ship gate) | 0 | 2 | 3 | 2 | APPROVE-CONDITIONAL — P1 on PRR 8/11 WAIVED ADR-0034 dual-hat scope + P1 on 30d post-sprint observation gate timeline binding | 10 panels + 11 alert rules + 4-tier SEV classification; STAGING-STABLE promotion correct; GA gate deferred to S-20 ✓. |

**Aggregate Part 2 verdict: APPROVE-CONDITIONAL.** 0 P0 / 4 P1 / 8 P2 / 6 P3. The P1 surface is concentrated on (a) the 30d sustained observation gate timeline binding, (b) the PRR 8/11 WAIVED dual-hat sign-off scope, and (c) the snapshot-bound + auto-fix interaction at scale-invariant boundary. None are SEAL-blockers (corpus is already FROZEN); all are recommendations for the S-20 GA gate observability + PRR cumulative integrity track.

---

## §2 Framework principles cited

- **PRINC-INV-001** — INV-GC-001 / INV-GC-003 / INV-GC-004 CRITICAL TLA+ obligations.
- **PRINC-TLA-001** — `gc_correctness.tla` model check verde required pre-merge; SHA pinning detects TLC binary drift.
- **PRINC-PRR-001** — HIGH_RISK lane requires 10–12 sign-offs; dual-hat waivers require ADR justification (ADR-0034).
- **PRINC-AUDIT-FAIL-CLOSED** — reconcile audit emit BEFORE refcount mutation; emit failure rolls back D1 batch.
- **PRINC-CHARTER-TRAIT-DEFER** — real D1 json_each SQL + atomic batch + production Cron DO alarm wiring + 100k nightly + cargo-fuzz are deferred to WI-007 consolidation per charter.
- **PRINC-PROMOTION-001** — STAGING-STABLE → GA promotion requires 30d sustained chaos + 7d refcount drift sustained + 30d sustained TLA+ verde (sprint contract §10.s06.4 + §10.s06.5; S-20 gate).
- **PRINC-OBS-DASH-CANONICAL** — DASH-GC 10 canonical panels + 11 alert rules with 4-tier SEV classification.

---

## §3 Per-WI findings

### §3.1 WI-S06-005 — Daily refcount reconciliation + dual-condition auto-fix + orphan-R2 detection

**Strengths.** (a) Canonical `json_each` JSON-aware membership is now load-bearing (Part 2a P0-1 fix); `prop_json_each_semantics_not_like` is the regression-pin (10k iter) that would catch any reintroduction of the substring LIKE; (b) **Dual-condition auto-fix gate** `drift_count ≤ 5 AND drift_percent ≤ 0.0001` (Part 2a P0-6 scale-invariant) — the explicit boundary tests `count=5 fires / count=6 rejects / percent=0.0001 fires / percent=0.000_101 rejects` are textbook SOTA; (c) **Conditional UPDATE anti-ping-pong predicate** `WHERE refcount = stored_refcount` (P0-7 chaos #11) — concurrent UpdateAR winner does NOT get overwritten by reconcile; (d) **Snapshot bound** `created_at_ms < snapshot_at_ms` with `snapshot_at_ms = phase_start + 1` (P1-6 fix) — post-snapshot writes excluded; (e) **Audit fail-closed envelope** (emit BEFORE refcount mutation; emit failure → `ReconcileError::AuditEmissionFailed` + preserved refcount) — PRINC-AUDIT-FAIL-CLOSED ✓; (f) **Orphan-R2 detection arm** `r2_present=false AND deleted_at_ms.is_none()` → never mutate refcount (would amplify inconsistency); deferred reclaim to S-09 task. 30 inline + 14 prop_reconcile tests.

**Findings.**

- **P1-005-1 [snapshot-bound + dual-condition auto-fix interaction at scale-invariant boundary]** — Cites `PRINC-INV-001`. The snapshot bound is `phase_start + 1ms` and the per-tenant scale-invariant auto-fix is gated by `drift_percent ≤ 0.0001` (0.01%). For very-large tenants (e.g., 10M blobs) the 0.01% percent floor (1000 records) overshoots the `drift_count ≤ 5` absolute floor by 200×, so the dual-AND gate degenerates to "absolute-floor-only" effectively. This is the *correct* behavior (the absolute floor is the safety net), but the WI does not explicitly document the asymptotic regime where the percent gate becomes vacuous. Recommendation: add a §1 invariant table row "At scale ≥ 50k blobs the gate is effectively absolute-floor-dominated; percent gate is the small-tenant safety net".

- **P2-005-1 [`gc_drift_pending` retry table deferred to WI-007 trait-abstraction-defer but the retry-on-throttle semantics not specified]** — Cites `PRINC-CHARTER-TRAIT-DEFER`. The narrative defers the table to WI-007 consolidation; the retry semantics (max retries, back-off, after-N escalation) should be pre-specified.

- **P2-005-2 [`dsr_signals_processed` JOIN deferred to S-11 wiring — the reconcile contract over a partially-DSR-erased blob set is not explicit]** — Cites `PRINC-INV-001`. When a tenant has DSR-erased blobs (R2 deleted + D1 row purged with audit retained), the reconcile arm should never count them; the JOIN deferral should pre-declare the exclusion predicate.

- **P2-005-3 [SEV-1 alert per-tenant > 1% drift may fire false positives on small tenants (< 100 blobs)]** — Cites `PRINC-OBS-DASH-CANONICAL`. A tenant with 50 blobs and 1 drift hits 2% — SEV-1. Recommendation: add a tenant-size minimum gate before SEV-1 fires (e.g., `tenant_blob_count >= 100` AND `drift_percent > 1%`).

- **P3-005-1 [`SevLevel` `#[non_exhaustive]` 3-arm — SEV-0 not modeled (reserved for INV-GC-001 violation in production); recommend documenting the SEV-0 surface explicitly even if it lives in WI-007 alert rules]** — Informational.

- **P3-005-2 [`AUTO_FIX_MAX_RECORDS = 5` and `AUTO_FIX_MAX_PERCENT = 0.0001` are canonical pins but the rationale for those exact values is not cited (NIST? Postgres VACUUM? empirical)]** — Informational. Cite the source.

### §3.2 WI-S06-006 — TLA+ CI gate + 100k race property test

**Strengths.** (a) `tla_check.yml` workflow is canonical with fail-closed on TLC SHA mismatch (Lote 10.6-tris NEW-P0-1 fix); the SHA literal `d5d07d5dab38ddb840c91ec48fa02f28b37a608d5af9a73570018591dbc8ef7f` is pinned across 4 anchor points (workflow + run_tlc_corelink.sh + ADR-0042 §A1 + property test fixture sanity check) — **canonical drift-detection**; (b) `prop_inv_gc_004_race.rs` (~530 LOC) ships dual-tier ceiling (PR-gate 10k / nightly 100k); deterministic PRNG `ChaCha20Rng::seed_from_u64(seed)` (Lote 10.6-tris OPUS-MISS-2 absorbed); (c) Adversarial fixture inputs cover envelope mutation + short-digest substring + schema-evolution (the regression direction that would be silently broken by a future LIKE-substring impl); (d) PR paths cover `crates/corelink-gc/**`, `specs/tla/gc_correctness.tla`, `migrations/**`, data_model.md, invariant_registry.md, the workflow itself, and ADR-0042 — comprehensive trigger surface. 5 tests in module: 1 proptest + 4 sanity (canonical TLC SHA pin verification + PRNG determinism + smoke at offset=0 / offset=-1).

**Findings.**

- **P1-006-1 [30d sustained TLA+ verde gate observability — the consumer (S-20 GA gate) requires a queryable metric `tla_ci_green_days_sustained` but the WI does not specify how that counter is emitted/maintained]** — Cites `PRINC-PROMOTION-001`. The 30d sustained gate is a critério promoção (§14); without a queryable counter the GA gate is a manual visual check on the CI history. Recommendation: emit `tla_check_pass{date}` counter or wire a daily aggregator into DASH-GC.

- **P2-006-1 [TLA+ scope-limitation disclosure (soft-delete grace window NOT in TLA+) is in ADR-0042 §A3 — but the WI does NOT cross-reference the §A3 disclosure in the §1 narrative]** — Cites `PRINC-TLA-001`. The disclosure is canonical; the WI should pin a one-line "TLA+ scope: ∅ {soft-delete grace window} → covered architecturally by WI-004 + WI-005 — ADR-0042 §A3" cross-ref.

- **P2-006-2 [`tla-30d-sustained.yml` workflow + `tla_override_validate.yml` deferred to WI-007 trait-abstraction-defer — neither workflow exists yet]** — Cites `PRINC-CHARTER-TRAIT-DEFER`. This is correct per charter; the deferral inventory in WI-007 §1.1 should be cross-cited.

- **P3-006-1 [TLA+ counterexample replay tooling — when TLC fails, the trace should be auto-attached to the failing PR]** — Informational.

- **P3-006-2 [Property test `mark_anchor` range `[1M, 10M] ms` — the rationale for that range vs production `mark_started_at_ms` (unix-ms epochs ~ 1.7T) is not documented; not a bug since the TLA+ invariant is offset-driven not absolute-timestamp-driven, but worth noting]** — Informational.

### §3.3 WI-S06-007 — DASH-GC + RB dry-runs + PRR ship gate

**Strengths.** (a) DASH-GC ships 10 canonical panels (`dashboards/grafana/DASH-GC.json`) matching the §1 enumeration; (b) 11 alert rules in `dash-gc-alerts.yml` with explicit 4-tier SEV-0/SEV-1/SEV-2/SEV-3 classification (Lote 10.4bis pattern); (c) **3 RB-FM dry-run scripts** (`rb_fm_300_dry_run.sh`, `rb_fm_404_dry_run.sh`, `rb_fm_305_dry_run.sh`) ship as host-side harnesses with chaos magnitudes pinned per Lote 10.6bis P0-W7-4 (0.5% per-tenant drift / `mark_started_at_ms+1ms` boundary / 7d cron-disabled + 100 GiB orphan); (d) PRR matrix is 11 sign-offs, 3 ✅ APPROVED + 8 ⚠️ WAIVED ADR-0034 (dual-hat); Crypto SME non-waivable seat satisfied via WI-S06-006 SEAL substantive review per Lote 10.4-tris P0-R5-005 precedent (defensible); (e) Promotion decision STAGING-STABLE — correct (not GA — that's S-20); (f) asvs-v5-v6-v8-v10-v14 checklist 44 PASS / 3 WAIVED-S-19 / 13 N/A; (g) Internal pentest report `2026-05-02-pentest-s06-internal.md` zero HIGH/CRITICAL across 6 attack surfaces; (h) `gc-ship-gate.yml` aggregate ship-gate fan-in for branch protection.

**Findings.**

- **P1-007-1 [PRR 8/11 WAIVED ADR-0034 dual-hat scope — the WAIVED set should be enumerated in the §1 narrative WITH the dual-hat reviewer name per row, not aggregated as "8 ⚠️"]** — Cites `PRINC-PRR-001`. HIGH_RISK lane sign-off review is the load-bearing trust ceremony. ADR-0034 dual-hat is allowed but each WAIVED row must be auditable individually (single reviewer hat 1 = X, hat 2 = Y, conflict-of-interest screen = Z). Recommendation: explode the 8 WAIVED rows in PRR-S06.md with per-row dual-hat justification.

- **P1-007-2 [30d post-sprint observation gate timeline binding — the §6 DoD says "30d staging sustained zero falso positivo" is post-sprint concurrent with S-07/S-08; but the WI does not specify the START-date of the 30d window (sprint-close ceremony date? PRR sign-off date? STAGING-STABLE promotion date?)]** — Cites `PRINC-PROMOTION-001`. The S-20 GA gate consumes this 30d window; without a pinned start-date the gate is ambiguous. Recommendation: pin start-date = "STAGING-STABLE promotion timestamp" and add a queryable `gc_observation_window_start_at` metric.

- **P2-007-1 [10-panel DASH-GC enumeration does not include a per-tenant "auto-fix invocation rate" panel — auto-fix is a security-adjacent action (reconcile mutates refcount) and operator visibility on auto-fix invocations is load-bearing]** — Cites `PRINC-OBS-DASH-CANONICAL`. Recommendation: panel 11 `auto_fix_invocations_per_tenant_per_day`.

- **P2-007-2 [11 alert rules but SEV-0 alert for INV-GC-001 violation is not enumerated — the spec contract §18 says "INV-GC-001 violation detected → CRITICAL post-mortem + customer notification + potential ANPD/DPC notification" but no alert rule fires that post-mortem]** — Cites `PRINC-INV-001`. Recommendation: SEV-0 alert `inv_gc_001_violation_total > 0` → page Security lead + Architect + DPO.

- **P2-007-3 [Customer-visible `bytes_reclaimed_last_30d` metric is published per WI-007 + S-16 forward — the canonical privacy boundary (this metric is per-tenant; cross-tenant aggregation must not leak)]** — Cites `PRINC-TENANT-CTX`. Recommendation: pin a tenant-scoped query contract in §1.

- **P3-007-1 [Customer SLA addendum + release notes + feature overview docs are scaffolds (finalisation at S-19) — recommend listing the open items per scaffold]** — Informational.

- **P3-007-2 [cargo-fuzz targets (`fuzz_sweep_decision`, `fuzz_reconcile_decision`, `fuzz_physical_delete_decision`, `fuzz_gc_correctness`) deferred to S-20 GA gate per charter — recommend explicit S-20 cross-link]** — Informational.

---

## §4 Cross-cutting observations

1. **PRR sign-off audit chain.** WI-S06-007 is the canonical PRR ship gate for S-06; the WAIVED 8/11 dual-hat sign-offs are the audit-fragile surface. **The Crypto SME non-waivable seat satisfied via WI-S06-006 SEAL substantive review (Lote 10.4-tris P0-R5-005 precedent) is defensible but precedent-stretching** — for a CRITICAL TLA+ obligation (INV-GC-004) the program would benefit from a separate Crypto SME sign-off on the WI-S06-006 100k race property test rather than rolling it into the SEAL substantive review. **Not a P0**; the corpus is already SEALED; flag for the cross-sprint PRR hygiene cumulative track.

2. **30d sustained observation gate timeline binding.** The S-20 GA gate consumes 3 sustained windows from S-06: 30d TLA+ verde, 30d chaos sustained, 7d refcount drift sustained. All three need pinned start-dates AND queryable counters. Currently only the chaos and TLA+ verde have CI workflows; the 7d drift sustained is implicit in `dash-gc-alerts.yml` SEV-2 threshold but not a queryable sustained-counter.

3. **DSR erasure interaction across WI-004 + WI-005.** WI-004 wires the Ed25519 DPO-signed signal scaffolding; WI-005 reconcile arm acknowledges DSR-erased blobs but defers the `dsr_signals_processed` JOIN to S-11. The WI-004 ↔ WI-005 ↔ S-11 forward-signal contract should have a single canonical doc (e.g., `specs/05_security/dsr_gc_interaction.md`) so the cross-WI invariant is auditable in one place.

4. **Asymptotic regime documentation.** The dual-condition auto-fix gate (count ≤ 5 AND percent ≤ 0.0001) is correctly defensive but the asymptotic dominance shift (percent-floor → absolute-floor at scale ≥ 50k blobs) is not explicitly documented; future operators will mis-tune the percent if they think it's the primary gate.

---

## §5 Top-of-pile recommendations (next-step concrete)

1. **WI-007 PRR-S06.md**: explode the 8 WAIVED rows with per-row dual-hat justification (P1-007-1).
2. **WI-007 §1**: pin 30d observation window start-date = STAGING-STABLE promotion timestamp + queryable counter (P1-007-2).
3. **WI-006 §1**: emit `tla_check_pass{date}` counter or daily aggregator wired into DASH-GC (P1-006-1).
4. **WI-005 §1**: document asymptotic dominance shift in dual-condition auto-fix gate (P1-005-1).
5. Add panel 11 `auto_fix_invocations_per_tenant_per_day` to DASH-GC (P2-007-1).
6. Add SEV-0 alert `inv_gc_001_violation_total > 0` (P2-007-2).
7. Extract canonical `specs/05_security/dsr_gc_interaction.md` cross-WI invariant doc (§4.3).

---

## §6 Verdict

**APPROVE-CONDITIONAL Part 2 corpus.** All 3 WIs are FROZEN AUDITED and the SEAL ceremony is correct. The 4 P1 findings are NON-blocking for S-06 SEAL but are **MUST-FIX before S-20 GA gate consumption** (specifically: PRR dual-hat audit chain explosion, 30d observation gate timeline binding, sustained-verde queryable counter, asymptotic regime documentation). The 8 P2 findings are recommendations for the DASH-GC cumulative track and the cross-sprint hygiene track. The 6 P3 findings are informational.

**Aggregate Part 2 score (calibrated against post-bis 9.1 target): 9.10/10.** WI-005 9.1, WI-006 9.2 (the TLC SHA pin discipline is exemplary), WI-007 9.0 (held back marginally by the PRR 8/11 WAIVED scope and the 30d gate timeline ambiguity).

**Aggregate S-06 R4 Opus score across Part 1 + Part 2 (7 WIs): 9.13/10.** First crossing of the 9.0+ SOTA target in the program; Lote 10.6-tris projection (9.1) ratified.

---

**End R4 Opus Part 2 review.**
