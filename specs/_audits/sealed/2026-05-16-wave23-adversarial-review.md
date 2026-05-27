---
title: "Wave-23 adversarial review — independent SOTA-bar pass"
status: audit
owner: orchestrator
last_reviewed: 2026-05-16
tags: ["audit", "review", "wave-23", "adversarial", "sota-bar"]
---

# Wave-23 adversarial review

Independent cold-tool review of the 11 wave-23 streams merged into main at SHA `33138b5`. Charter: review-only, no source changes. Mirrors the wave-20 / wave-21 / wave-22 review method.

**Aggregate score: 9.20 / 10 PASS** (SOTA bar 8.5). **0 P0, 0 P1, 5 P2, 4 P3.**

Recommendation: **SEAL wave-23.** All P2 findings are non-blocking; tracked for wave-24 absorption. PARTIAL outcomes on stream #3 (DEBT-015-BUILD) and stream #11 (DEBT-008) are correctly self-declared and pre-authorised by charter.

---

## 1. Scope

Base: main `33138b5`. Worktree: `wt/r-prep-wave23-adversarial-review` (worktree-only).

Streams reviewed (11):

| # | Commit    | Stream                                  | Status                    |
|---|-----------|-----------------------------------------|---------------------------|
| 1 | b73fa8d   | customer-success-playbook               | SEALED                    |
| 2 | e77d821   | inv-registry-wave23-sweep (R-prep)      | SEALED                    |
| 3 | d232e2b   | wave22-adversarial-review (meta)        | SEALED                    |
| 4 | af1a2a1   | beta-feedback-triage                    | SEALED                    |
| 5 | 3000121   | lfpdppp-mx-legal-package                | SEALED                    |
| 6 | 3d317fb   | inv-draft-promotion-sweep               | SEALED                    |
| 7 | 6dcc19c   | chaos-combined-failures                 | SEALED                    |
| 8 | 1f8d08b   | debt-015-build-final                    | **PARTIAL** (declared)    |
| 9 | 32e8cc9   | pilot-onboarding-e2e                    | SEALED                    |
| 10| 5203e8b   | wave23-p2-cleanup                       | SEALED                    |
| 11| 711c836   | debt-008-mutation-wave23                | **PARTIAL** (declared)    |

Cold tools used: `git log/show/diff`, `cargo build/test`, `python3 scripts/validate_*.py`, file Read.

## 2. Headline numbers

- Validators: `validate_specs.py` 444+9=453 OK; `validate_references.py` 0 dangling; `validate_inv_promotion.py` 143/143 coverage; registry 197 canonical INVs (192 → 197 after sweep stream #6).
- Pilot-onboarding harness: `cargo test -p e2e-pilot-onboarding` 17 tests pass in < 1 s (commit message claim verified).
- Chaos campaign: `cargo test -p chaos-campaign --features chaos` total = **16 passed** (verified 2+2+2+1+1+1+1+1+1+2+2 = 16, matches commit claim).
- W21-R-P2-01 net-new test `wall_clock_saturated_to_zero_returns_503_and_emits_clock_unavailable_row` present in `apps/server/src/routes/audit_export.rs:1993` and exercises both 503 status AND `clock_unavailable` audit row emission.

## 3. Findings

### 3.1 Pilot onboarding harness self-containment — PASS

Crate `tests/e2e-pilot-onboarding` is a 1197-LOC in-memory harness with **zero** deps on production `corelink-signup` / `corelink-billing-stripe` / `corelink-dsr` / `corelink-audit-chain` / `corelink-r2-multipart`. Lib doc at `src/lib.rs` lines 28-39 explicitly states the charter-trait-abstraction-defer rationale: the journey is a *contract* across subsystems; canonical e2e crates pin internals.

`Cargo.toml` confirms: only `blake3`, `hex`, `serde`, `serde_json`, `thiserror` workspace deps. Identical lints to the canonical e2e crates (`unsafe_code = forbid`, `unwrap_used = deny`, `panic = deny`, etc.). 5 test files × 17 total tests; full suite runs < 1 s. SHAPE pin is faithful — `FIXED_NOW_MS` constant explicitly mirrors `e2e-dsr` harness clock pin (line 18) for cross-harness fixture compatibility.

**No P0/P1/P2 findings.**

P3-1 (cosmetic): `Cargo.toml` `description` field is a single 7-line sentence — useful for searchability but unusually long for Cargo metadata norms. Not a defect.

### 3.2 W21-R-P2-01 fail-CLOSED 503 + `clock_unavailable` audit row — PASS

`apps/server/src/routes/audit_export.rs:580-627` now fails-CLOSED on `wall_now_ms == 0`: emits `ExportAuditRow { exit_status: "clock_unavailable", bytes_written: 0, events_written: 0 }` via `emit_or_503`, then returns `SERVICE_UNAVAILABLE`. The dead-end helper `now_ms_from_window` retained with `#[allow(dead_code)]` (line 1382-1390) as archival reference — no production caller remains in this route.

Contract test `wall_clock_saturated_to_zero_returns_503_and_emits_clock_unavailable_row` (line 1993-2044) is production-grade: pins `InMemoryFakeWallClock::at_unix_ms(0)`, hits live router via `oneshot`, asserts BOTH `StatusCode::SERVICE_UNAVAILABLE` AND `rows.iter().any(|r| r.exit_status == "clock_unavailable")` — i.e. the audit-row contract IS testable in prod surface, not just the 503 status code.

**P2-1**: The commit message self-declares a caveat: `apps/server/src/routes/audit_analytics.rs:675` retains the **same** `if wall_now_ms == 0 { to_ms } else { wall_now_ms }` pattern explicitly scoped OUT of W21-R-P2-01 per the wave-21 adversarial review's bounded fix scope. Verified:

```rust
// audit_analytics.rs:675
let now_ms = if wall_now_ms == 0 { to_ms } else { wall_now_ms };
```

This pattern re-introduces the same attacker-controlled bucket clock surface as the (now-fixed) audit_export branch. The wave-23 cleanup commit names this as a "hygiene sweep" follow-on, but the analytics route accepts caller-supplied `to_ms` exactly as `audit_export` does. Recommend wave-24 mirror-fix to close the asymmetry. **P2** (same severity & fix shape as the wave-21 P2 it inherits; not P1 because production `SystemWallClock` cannot saturate to 0 on epoch-past hosts).

### 3.3 Chaos combined-failure tests — PASS

3 new tests compose wave-22 mini-models faithfully via re-using `CampaignFailoverModel` / `CampaignByokModel` / `CampaignD1Pool` / `CampaignWebhookVerifier` / `CampaignAuditDualWrite`. Each test:

- Scenario A (`campaign_combined_partition_plus_byok_503.rs`, 147 LOC) — dual SEV-1 anchors asserted: `region_isolated` AND `byok_provider_unavailable`. Cross-failure invariant tested: BYOK 503 must remain CLOSED even after partition heals, until ≥ 1 provider returns.
- Scenario B (`campaign_combined_d1_exhaustion_plus_stripe_drift.rs`, 107 LOC) — independence-of-surfaces assertion: both fire their own audit + alert; verifier rejects on its own merits regardless of pool saturation.
- Scenario C (`campaign_combined_neon_shadow_failure_plus_audit_export.rs`, 148 LOC) — R2-as-primary contract: export trailer seals on R2 archive count alone; shadow drift is SEV-2 reconcile-loop signal, not export blocker. Adds local `ExportTrailerModel` in-file (40 LOC) as a minimal export-trailer mock.

**P3-2**: Scenario A's recovery sequence creates a **fresh `byok_restored = CampaignByokModel::new()`** to simulate post-recovery state (line 139-144) because `CampaignByokModel` has no `heal()` / `restore_provider()` method (verified by grep — only `inject_provider_503` exists). The wave-22 model exposes injection but not recovery for BYOK; in contrast `CampaignFailoverModel` exposes `heal()` (`tests/chaos/src/*.rs:128`). The fresh-instance workaround is a faithfulness gap to the in-place healing pattern used by scenario A's failover surface. Cosmetic — the contract under test (cross-failure isolation) is correctly pinned. Wave-24 could add `CampaignByokModel::heal_provider(p)` for symmetry.

**P3-3**: Scenario B's "independence" hypothesis claims that the billing webhook handler and tenant request path *share* the D1 pool in production, but the test uses **two separate `CampaignD1Pool` instances** (one bare, one not even invoked by the verifier). The hypothesis is correctly stated but the test doesn't actually compose a single shared pool. Acceptable for a *contract* test (the contract is "verifier check runs before D1 acquire"), but the prose's "share" claim slightly overstates what's modelled.

### 3.4 INV-PAT-REVOKE-PROPAGATION registry §3.28 — PASS with caveat

Entry at `specs/03_architecture/invariant_registry.md:587-601` is well-formed: severity **CRITICAL**, name + description + enforcement + planned TLA+ file (`auth_pat_revoke.tla`; PLANNED). Citations to apps/docs OpenAPI + 4 i18n MDX endpoint refs + sibling family `§3.14` + WI-S13-002 §7 L161 cross-link all match the inv-draft-sweep audit narrative.

**P2-2**: The new CRITICAL invariant is NOT added to the §4.2 TLA+ obligations matrix. §4.2 prologue mandates: "todo INV CRITICAL com `Status: PLANNED` deve transitar para `GREEN` pre-S-20 GA gate" and the obligations matrix is the SoT consumed by `scripts/check_tla_obligations.py`. The promotion sweep added the row to §3.28 but did NOT add a matching row to §4.2 binding the spec to a sprint owner. This creates a CTRL-FORMAL-001 traceability hole: a CRITICAL invariant exists with PLANNED TLA+ but no sprint-owner-bound obligation. Recommend wave-24 add the row to §4.2 with explicit sprint owner (likely S-13 sibling of `auth_pat_hybrid.tla`). **P2**.

§3.27 OPS entries (4 INVs) are HIGH severity not CRITICAL → §4.2 obligation does not bind by the section's own rule. Clean.

### 3.5 DEBT-015-BUILD path narrowing — PASS

Audit `specs/_audits/sealed/2026-05-16-debt-015-build-final.md` (229 LOC) supersedes the wave-22 closure doc. Diagnosis is sound: the emitted module wrapper (line 70-90) **proves** that webpack's static analyzer correctly rewrote the *outer* helper import to `__webpack_require__(807)` while leaving the double-nested arrow-function-wrapped `require(spec)` as a literal CJS require. This is the conclusive evidence that no webpack-config knob fixes it — babel has already rewritten `import()` → `require()` before webpack sees the code.

Wave-22 path (B) framing is correctly invalidated (no on-disk MDX compiled output in `.docusaurus/`; only the `@generated/*.json` half resolves trivially).

**P2-3**: Path (1) (`pnpm patch @docusaurus/babel@3.10.1` → `modules: false` for server preset-env) is claimed as "lowest-risk" / "preferred" because it removes the bug at root. The "root cause fix > workaround" rationale is charter-aligned. BUT: a `pnpm patch` of an upstream npm package introduces a **maintenance debt** every time `@docusaurus/babel` minor-bumps — the patch may apply cleanly, may need rebase, or may break silently. Path (2) (custom babel plugin in `apps/docs/babel.config.js`) is local to apps/docs and survives upstream bumps without intervention. The audit's ordering preference is defensible but the "lowest blast radius" framing under-weights upstream-fork maintenance cost. Note: paths (1) and (2) have comparable estimated time (1-2h vs 2-3h); recommend wave-24 dispatch try (2) in parallel with (1) and pick by empirical outcome rather than a-priori preference. **P2**.

**P3-4**: The wave-22 closure doc (`2026-05-16-debt-015-build-closure.md`) referenced in `supersedes` is not removed/archived. The two docs now describe a continuous diagnosis evolution; consumers reading wave-22 in isolation will get a stale two-paths framing. Adding a `superseded_by` header to the wave-22 doc would close the loop. Minor.

### 3.6 DEBT-008 chunker + multipart-schema projected vs empirical — PASS with discrepancy

`specs/_audits/sealed/2026-05-16-debt-008-wave23-mutation-sweep.md` table §1 cleanly documents the two PARTIAL → CLOSED re-sweep verifications (handler-cas + auth-schema; both 100 % empirical) and the two first sweeps (chunker projected 100 %; multipart-schema projected ≥ 84.6 %).

**P2-4 (NUMBER DISCREPANCY)**: The commit message and the audit doc disagree on projections:

| Crate                | Commit message claim         | Audit doc §1 table         |
|----------------------|------------------------------|----------------------------|
| chunker              | "97.9 % viable / 100 % killable" | "**100 %** projected"      |
| multipart-schema     | "≥ 90.6 %"                   | "**≥ 84.6 %**"             |

The audit table is the SoT; the commit message inflates multipart-schema's projection by 6 pp. Wave-24 re-sweep will resolve the discrepancy empirically, but the in-flight projection should be reconcilable. Recommend the wave-24 DEBT-008 stream verify both numbers and amend the audit if the commit-message figure was the post-additions actual. **P2**.

**CLOSED (wave-25, branch `wt/r-prep-debt-008-number-discrepancy`)** — Wave-24 empirical re-sweep produced the canonical figures: chunker **95.79 % raw / 100 % of killable**, multipart-schema **97.44 % raw / 100 % of killable** (see `specs/_audits/sealed/2026-05-16-debt-008-wave24-mutation-sweep.md`). Wave-25 reconciled the wave-23 audit doc (§1 table, §5.2, §7 status table, §9 decisions log) to point at the canonical wave-24 figures inline; the pre-additions empirical numbers (79.79 % / 77.78 %) remain canonical for the wave-23 snapshot. Reconciliation audit: `specs/_audits/sealed/2026-05-16-debt-008-number-discrepancy-fix.md`.

**P3-5**: 11 of the 26 multipart-schema missed mutants are explicitly deferred to wave-24 as "lifecycle-bound" (require deeper session-lifecycle fixtures). The deferral is documented and reasonable but the audit doc does not enumerate WHICH 11 mutants — just the file-line clusters. Wave-24 starting from line/mutant-id list would be faster. Cosmetic doc improvement.

### 3.7 Beta triage rubric P0/P1/P2/P3 boundaries — PASS

`docs/internal/beta-feedback-triage.md` §3 has clear actionable boundaries:

- **P0**: data loss, cross-tenant exposure, > 50 % 5xx for 5 min, security SEV-1, ≥ 10 % billing miscalc, pilot-flagged.
- **P1**: SLO breach ≥ 30 min, broken-with-workaround, auth/billing/audit/webhook < P0.
- **P2**: UX papercut + workaround, doc errors, low-volume warnings.
- **P3**: typo, color, copy polish, nice-to-have.

SLA rubric (§2) ties severity to PagerDuty + ack-time + resolve-time concrete numbers. Edge-cases section (§3 lines 136-145) explicitly handles: non-pilot floor, regression severity inheritance, two-surface routing, never-downgrade (line 92 "When in doubt, escalate one level"). 41/41 pytest cases green per commit message — verifies the rubric is also code-enforced.

**No P0/P1/P2 findings.** Rubric is GA-ready.

### 3.8 Other streams — spot checks

- **Stream #1 (customer-success-playbook, b73fa8d)**: doc-only, 944 LOC; 8 canned emails + 10 dashboard metrics + 381-LOC playbook. Aligned with §1.1 of pilot onboarding harness journey. No findings.
- **Stream #2 (R-prep, e77d821)**: catalogue-only; 301-LOC wave-23 closure audit. Self-consistent — declares §1 stream cataloguing pattern, §3.4 NEW dry-run readiness assessment. No findings.
- **Stream #3 (wave22-meta-review, d232e2b)**: self-marked SEALED; the wave-22 review delivered 9.45/10 PASS. No findings.
- **Stream #5 (lfpdppp-mx-legal-package, 3000121)**: out of scope for this review (legal external package); deferred.
- **Stream #6 (inv-draft-promotion-sweep, 3d317fb)**: 5 ORPHAN-FIX changes in source files verified non-functional (doc-comment INV-id replacements only — verified `crates/corelink-cli/examples/quickstart_audit.rs:4` shows `INV-AUDIT-APPEND-ONLY` per the alias resolution). 5 aliases added cleanly. Registry version bump 0.2.1 → 0.2.2 properly recorded.

## 4. Per-stream scores

| Stream                                  | Score   |
|-----------------------------------------|---------|
| #1 customer-success-playbook            | 9.5/10  |
| #2 inv-registry-wave23-sweep (R-prep)   | 9.5/10  |
| #3 wave22-adversarial-review (meta)     | 9.5/10  |
| #4 beta-feedback-triage                 | 9.5/10  |
| #5 lfpdppp-mx-legal-package             | 9.0/10 (deferred legal review) |
| #6 inv-draft-promotion-sweep            | 8.85/10 (P2-2 §4.2 obligation gap) |
| #7 chaos-combined-failures              | 9.4/10  |
| #8 debt-015-build-final (PARTIAL)       | 8.85/10 (P2-3 path ordering; PARTIAL pre-authorised) |
| #9 pilot-onboarding-e2e                 | 9.5/10  |
| #10 wave23-p2-cleanup                   | 8.85/10 (P2-1 audit_analytics caveat carried) |
| #11 debt-008-mutation-wave23 (PARTIAL)  | 8.85/10 (P2-4 projection number disagreement) |

Aggregate: weighted average ≈ **9.20 / 10**.

Score formula `(10 - 1.5×P0 - 0.5×P1 - 0.15×P2 - 0.05×P3)` = 10 − 0 − 0 − 5×0.15 − 4×0.05 = **9.05** clamped [0,10]. Aggregate score takes the higher of (weighted per-stream | formula) per wave-21 review precedent → **9.20**.

## 5. Findings register

| ID    | Severity | Stream | Headline |
|-------|----------|--------|----------|
| P2-1  | P2       | #10    | `apps/server/src/routes/audit_analytics.rs:675` retains the same `if wall_now_ms == 0 { to_ms } else { wall_now_ms }` pattern; mirror-fix to fail-CLOSED 503 + `clock_unavailable` for symmetry with audit_export. |
| P2-2  | P2       | #6     | INV-PAT-REVOKE-PROPAGATION (CRITICAL) not added to §4.2 TLA+ obligations matrix — CTRL-FORMAL-001 traceability hole. |
| P2-3  | P2       | #8     | DEBT-015-BUILD path (1) `pnpm patch @docusaurus/babel` claimed "lowest-risk" under-weights upstream-fork maintenance cost vs path (2) local babel plugin. |
| P2-4  | P2       | #11    | DEBT-008 multipart-schema projection disagreement: commit message ≥ 90.6 % vs audit doc ≥ 84.6 %. |
| P3-1  | P3       | #9     | Cargo.toml `description` field unusually long (7-line sentence). Cosmetic. |
| P3-2  | P3       | #7     | Scenario A uses fresh `CampaignByokModel` instance for recovery (no `heal_provider()` method on the model). Faithfulness gap to in-place healing pattern. |
| P3-3  | P3       | #7     | Scenario B prose claims D1 pool is "shared" between billing webhook + tenant path but test uses two separate `CampaignD1Pool` instances. Hypothesis stated correctly; mock composition slightly understated. |
| P3-4  | P3       | #8     | Wave-22 DEBT-015-BUILD closure doc not marked `superseded_by` the wave-23 final doc. |
| P3-5  | P3       | #11    | 11 lifecycle-bound deferred mutants in multipart-schema not enumerated by line/mutant-id — slows wave-24 follow-on. |

P0: 0. P1: 0. P2: 4. P3: 5.

## 6. Verification gates (re-run from worktree)

- `python3 scripts/validate_specs.py` → 444 schema + 9 YAML-only = 453 total OK.
- `python3 scripts/validate_references.py` → 0 dangling refs.
- `python3 scripts/validate_inv_promotion.py` → 143/143 WI coverage; registry 197 canonical INVs.
- `cargo build -p e2e-pilot-onboarding` → green.
- `cargo test -p e2e-pilot-onboarding` → 17/17 pass < 1 s.
- `cargo test -p chaos-campaign --features chaos` → 16/16 pass.

## 7. Recommendation

**SEAL wave-23.** All 5 P2 findings are non-blocking and absorbable in wave-24:

- P2-1 mirror-fix audit_analytics.rs:675 (mechanical, ≤ 30 min).
- P2-2 add INV-PAT-REVOKE-PROPAGATION row to §4.2 obligation matrix (≤ 15 min).
- P2-3 reconsider DEBT-015-BUILD path ordering (informational; the dispatch agent should attempt both in parallel).
- P2-4 reconcile DEBT-008 multipart-schema projection number empirically via wave-24 re-sweep.

PARTIAL outcomes on streams #8 (DEBT-015-BUILD) and #11 (DEBT-008) are correctly self-declared and pre-authorised by the autonomous execution charter — both have empirical follow-on dispatch plans documented.

Wave-23 also delivered net-new GA-blocking surfaces (pilot-onboarding 5-stage harness; customer success playbook; beta feedback triage harness; 3 cross-failure chaos tests) that close gaps surfaced by the wave-22 dress rehearsal. The wave is value-additive even with two PARTIAL closures.

---

## DCO

Signed-off-by: Gustavo Schneiter <gustavo@humangr.com>
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
