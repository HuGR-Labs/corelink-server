---
type: audit
title: Sonnet R5 tris (round-2) review of S-08 WIs post-Lote 10.8bis
date: 2026-04-25
reviewer: Sonnet R5 (Claude Sonnet 4.6 — different model lineage from Opus R4; round 2 / tris)
sprint: S-08
target: 6 WIs post-bis cycle (WI-S08-001 through WI-S08-006)
baseline_round1: 6.9/10 (Sonnet R5 round 1); 7.45/10 aggregate (R4 8.0 + R5 6.9)
---

# Sonnet R5 tris review — S-08 (Lote 10.8bis cycle validation)

## Aggregate score: 8.1/10 (vs round-1 baseline 6.9; pre-bis aggregate 7.45)

Significant improvement post-bis. Five P0s and most P1s properly resolved. However, bis fixes introduced three new defects (P0-NEW-1 being the most serious: enforcement framing resurfaces in residual code that the P0-C prose fix missed), and several P2-level CI arithmetic and schema issues remain unfixed from round 1.

---

## Per-WI scores

| WI | Round-1 | Tris | Delta | Key change |
|---|---|---|---|---|
| WI-S08-001 | 7.5/10 | 8.3/10 | +0.8 | Division-by-zero guard correct; tenant_rate_override integration described in CL but absent from scope section code — P1 |
| WI-S08-002 | 7.0/10 | 7.8/10 | +0.8 | Provider v4 Terraform schema fixed correctly; ratio_4xx == 1.0 still unfixed (P2 carried); ipnet WASM unaddressed (P2 carried) |
| WI-S08-003 | 5.5/10 | 8.4/10 | +2.9 | P0-C, P0-D, P1-6 correctly fixed; residual enforcement framing in §2 + §6.1 (NEW P0 introduced by partial fix) |
| WI-S08-004 | 6.0/10 | 8.3/10 | +2.3 | P0-E calibration scaled + CI stated; expires_at NOT NULL contradiction on AdminReview/SuspendCandidate rows (NEW P1); CI math on 2/50 understated |
| WI-S08-005 | 7.0/10 | 8.5/10 | +1.5 | P0-B citation chain clean; do_error_rate fixed; ManualOverride SLI exclusion correct; counts_against_sli signature change has no call-site shown — minor |
| WI-S08-006 | 7.5/10 | 7.5/10 | 0.0 | Alert count fixed to 14 but body description still claims 5 SEV-2 while YAML has 6 (NEW P1); sign-off 13→12 partially fixed — 5 stale "13" references remain |

---

## Bis fix verification

### P0-A: bandwidth_quota discriminator removed (WI-S08-003)
**PASS.** `PatRateOver` removed from `QuotaError` enum. `bandwidth_quota` does not appear anywhere in WI-S08-003. WI-S08-005 enum has exactly 5 variants: `TenantQuota | PerIp | PerPat | OverQuota | GlobalCircuitOpen`. The audit-log reason-field discrimination (`reason=bandwidth_egress_exceeded`) is correct and documented.

### P0-B: multi-signal trigger citation chain (WI-S08-005)
**PASS.** No occurrences of "Lote 10.7bis P0-9 race-aware" in WI-S08-005. Replaced with "sprint contract §15 R-S08-004 + standard circuit-breaker pattern (Netflix Hystrix / Resilience4j)". Verified ~12+ occurrences replaced. The heading in WI-S08-005 §1 mentions this explicitly and the narrative in §2 consistently cites the canonical reference.

### P0-C: per-PAT camada 3 reframing as detector (WI-S08-003)
**PARTIAL — introduces NEW P0 (see P0-NEW-1 below).** The trait signature correctly returns `Ok always`. `PatRateOver` error variant is removed. D1 table renamed to `pat_rate_observation`. Column renamed to `misuse_threshold_per_sec`. Function renamed to `pat_rate_misuse_threshold_for_tier`. **BUT**: two residual enforcement claims remain in the prose:

- §2 Adversarial scenarios (line 271): "camada 3 cap 10× → **90% requests rejected**; SEV-2 alert; admin revokes PAT"
- §6.1 chaos test 6 (line 560): "camada 3 cap 10× kicks in; **80% requests rejected**; SEV-2 alert; admin revokes"

Both contradict the central P0-C fix that `check_pat_rate` returns `Ok always` and does NOT block requests. These are enforcement framing claims that survived the bis fix because they were in narrative prose rather than code. A Compliance reviewer reading §2 would believe camada 3 blocks requests.

**Additionally**: DO state field still says `per_pat_state: HashMap<PatId, TokenBucketState>` (line 317) — the type name `TokenBucketState` is inconsistent with the P0-C claim that this is EWMA rolling rate (NOT a token bucket). Also `PatRateState::new(now_ms)` at line 421 — `PatRateState` is not renamed to reflect the detector nature. Property test `prop_pat_rate_cap` (line 550) says "assert per-PAT ≤ 10× tenant cap" which still frames it as a cap (enforcement) rather than a detection threshold.

### P0-D: next_month_first_utc_midnight() utility (WI-S08-003 + corelink-time crate)
**PASS with minor concern.** The function is correctly defined with December → January year increment. `secs_until_next_month_first_utc_midnight` is correctly implemented. Both WI-S08-003 and WI-S08-005 reference `corelink_time::secs_until_next_month_first_utc_midnight()`.

**Minor concern**: The property test boundary case (d) states "Feb 28 leap year (2028) → Feb 29 → Mar 1" — this is ambiguous. The test description implies calling `next_month_first_utc_midnight` on Feb 28 of a leap year should return March 1 (skipping over Feb 29). This is CORRECT behavior (next month's first is Mar 1 whether or not it's leap year). However, the test doesn't include the missing boundary: calling the function **on Feb 29** (leap day) to ensure it returns Mar 1 correctly. The function implementation using `NaiveDate::from_ymd_opt(year, month+1, 1)` handles this correctly, but the property test spec doesn't cover it explicitly. Low severity.

### P0-E: calibration scaled n=5+5 → n=50+50 + 95% CI
**PASS (with CI arithmetic caveat — see P2-NEW-1).** Persona 3 reasoning leak removed. Persona 3.1 explicitly acknowledges medium-intensity abuse out-of-scope. The sprint contract §6 DoD updated. ST-009 PERT 3h → 8h. The 95% CI requirement is present and bounded targets stated. The sprint contract text matches the WI description.

**CI arithmetic caveat**: The claim "≤2 FP in 50 → CI upper 9.6%" uses the Wald interval approximation which gives approximately 9.4% (not 9.6%) — this is the least conservative method and slightly understates the true upper bound. The correct Clopper-Pearson (exact) two-sided 95% CI upper for 2/50 is approximately 12-13%. The stated value is defensible as an approximation but the methodology should specify which CI method is used. See P2-NEW-1.

### P1 fixes verification

**R4 P1-1 (8 alert rules → 14 canonical)**: **PARTIAL FAIL — see NEW P1-NEW-1.** The heading claims "14 alert rules canonical (4 SEV-1 + 5 SEV-2 + 5 SEV-3)" but the actual YAML in §6.1.2 defines **6 SEV-2 rules** (abuse-admin-review, pat-misuse, manual-override, blocklist-drift, appeals-queue-overflow, storage-100pct), not 5. The total is 4+6+5=15 unique rules, not 14. The "5 SEV-2" claim in all narrative text is wrong. The paragraph §2 also says "SEV-2 (5): abuse admin review + PAT misuse + manual override + blocklist drift + appeals queue overflow + storage 100%" — listing 6 items after writing "(5)". This is a bis-introduced defect: the bis fix changed 8→14 but miscounted SEV-2.

**R4 P1-2 (sign-off cap 13→12)**: **PARTIAL PASS.** WI-S08-001 §11 DoD says "12 sign-offs" (correct). WI-S08-001 §30 table has exactly 12 rows (correct). WI-S08-006 §30 table has exactly 12 rows (correct). However, WI-S08-006 has **5 stale references to "13 sign-offs"** at lines: 240 (Grafana panel title "13 sign-offs"), 449 (Persona 4 "sees 13 sign-offs"), 472 (Tipo section "13 sign-offs"), 502 (PRR section "13 sign-offs"), 548 (chaos test "missing 13"). These are internal inconsistencies within WI-S08-006 — the headline and sign-off table say 12 but the narrative body, Grafana panel title, and chaos test scenario still reference 13.

**R4 P1-3 (FM-251 → FM-201)**: **PASS.** WI-S08-002, WI-S08-003, WI-S08-004, WI-S08-005 all reference FM-201 canonically. FM-251 does not appear in any WI code sections.

**R4 P1-4 (PAT-CIRCUIT-BREAKER-001 → PAT-CIRCUIT-001)**: **PASS.** WI-S08-005 §4 correctly cites "PAT-CIRCUIT-001".

**R4 P1-13 (INV §3.X → §3.12)**: **PASS.** WI-S08-001 §1 invariant 1 says "registry §3.12". No stale §3.X references found in code sections.

**R5 P1-1 (division-by-zero guard)**: **PASS.** Guard implemented at lines 204-209 of WI-S08-001: `if self.plan_refill_rate_per_sec <= f64::EPSILON { 86400 * 7 }`. Semantics correct (7-day retry for canceled tenant). The epsilon comparison is more robust than `== 0.0`.

**R5 P1-2 (do_error_rate_5m averaging)**: **PASS.** WI-S08-005 §6.1 item 6 now uses `window_5min.last().map(|o| o.do_error_rate_5m).unwrap_or(0.0)` — correct last-observation methodology.

**R5 P1-3 (ManualOverride excluded from counts_against_sli)**: **PASS.** `counts_against_sli(&self, trip_reason: Option<&TripReason>) -> bool` correctly returns `false` for `TripReason::ManualOverride { .. }`.

**R5 P1-5 (CF Terraform provider v4 schema)**: **PASS.** WI-S08-002 §1 Terraform now uses `action_parameters { ratelimit { ... } }` nested syntax. Provider version pinned at `~> 4.0`.

**R5 P1-6 (overshoot off-by-one)**: **PASS.** Line 354: `total_committed.saturating_sub(self.max_storage_bytes)` — correct formula, no +1 bias.

**CI-2 (tenant_rate_override table)**: **PARTIAL PASS — see NEW P1-NEW-2.** The table is defined inline in WI-S08-004 §6.1.4 with correct schema. WI-S08-004 writes it, WI-S08-001 reads it (per changelog). **But**: the `check_and_consume` code section in WI-S08-001 §6.1 item 3 does NOT show the D1 read for `tenant_rate_override`. The override mechanism is described only in the change log, not in the scope section or code specification. Implementers reading WI-S08-001 §6.1 will not know to query this table. No migration artifact is listed in WI-S08-001 §13 for this table (owned by WI-S08-004 but read by WI-S08-001 — cross-WI dependency not formalized in artifacts). Also the table has `expires_at INTEGER NOT NULL` but no CHECK constraint that `expires_at > applied_at` (unlike `abuse_response_actions` which has that constraint).

**CI-3 (orphaned reservation behavior)**: **PASS.** WI-S08-005 §1 invariant 12 documents the orphaned reservation behavior comprehensively. The sweep cycle timing is documented, and the bounded ≤ next cycle recovery claim is correctly scoped.

---

## NEW P0 findings (post-bis defects or round-1 misses)

### P0-NEW-1: Enforcement framing survives in WI-S08-003 §2 adversarial + chaos (INTRODUCED BY BIS)

**Location**: WI-S08-003 §2 (line 271) + §6.1 chaos test 6 (line 560).

**Finding**: P0-C correctly removed `PatRateOver` error variant and updated the trait signature to always return `Ok`. However, the bis fix was applied only to the trait definition, code sections, and D1 table DDL. Two narrative prose locations were missed:

1. §2 Adversarial scenarios: "**camada 3 cap 10× → 90% requests rejected**; SEV-2 alert; admin revokes PAT"
2. §6.1 chaos suite scenario 6: "camada 3 cap 10× kicks in; **80% requests rejected**; SEV-2 alert; admin revokes"

A Compliance reviewer reading §2 would conclude that camada 3 BLOCKS 90% of requests (enforcement), contradicting the P0-C finding that camada 3 "returns Ok always" and "does NOT block requests at camada 3 (camada 1 already throttles aggregate)." 

Additionally, `per_pat_state: HashMap<PatId, TokenBucketState>` in the DO state spec (line 317) still names the type `TokenBucketState` — inconsistent with the EWMA rolling rate detector framing. `PatRateState::new` (line 421) similarly. Property test `prop_pat_rate_cap` (line 550) says "assert per-PAT ≤ 10× tenant cap" — enforcement semantics in a test that should be "assert per-PAT detection triggers at >10× threshold."

**Required fix**: Update §2 adversarial text and §6.1 chaos test 6 to "camada 3 detection threshold exceeded; SEV-2 alert emitted; request NOT blocked at camada 3 (camada 1 aggregate enforcement already active)." Rename `TokenBucketState` → `PatRateObservationState` or similar. Rename `prop_pat_rate_cap` → `prop_pat_rate_misuse_detection_threshold`.

**Severity**: P0 — a Compliance/Privacy sign-off could approve a per-PAT enforcement layer that actually doesn't enforce, based on reading §2. Also contradicts the sprint contract §5 R-S08-3 correction that was supposedly applied in bis Phase 6.

---

### P0-NEW-2: WI-S08-006 sign-off inconsistency — 13 references not updated (5 stale; INTRODUCED BY BIS)

**Location**: WI-S08-006 lines 240, 449, 472, 502, 548.

**Finding**: The bis fix correctly updated WI-S08-006 §30 sign-off table (12 rows) and the change log says "sign-off cap 13→12". But 5 internal references within the same WI still say "13":
- Line 240: Grafana panel title `"S-08 PRR Ship Gate Status (13 sign-offs HIGH_RISK)"` — this panel would display the WRONG number in the operational dashboard
- Line 449: Persona 4 narrative "sees 13 sign-offs status"
- Line 472: §5 Tipo section "PRR consolidated 13 sign-offs"
- Line 502: §6.1 item 4 PRR consolidated section "13 sign-offs HIGH_RISK consolidated"
- Line 548: Chaos test 7 "synthetic 12 sign-offs (missing 13); assert ship gate REJECTED" — the test logic is inverted; if cap is 12, then collecting 11 should REJECT, not "missing 13"

The most operationally critical is line 240: the Grafana dashboard panel title will display "13 sign-offs" when the actual gate requires 12. Oncall operators reading the dashboard during a PRR review will see incorrect requirements.

**Severity**: P0 by operational impact — the Grafana dashboard is a deliverable artifact. Displaying wrong PRR requirements misleads the ship gate process. This was introduced by a partial replacement in the bis fix.

---

## NEW P1 findings

### P1-NEW-1: Alert count: 14 canonical claim but YAML defines 15 rules (WI-S08-006)

**Location**: WI-S08-006 §1, §2, §6.1 item 2, §8 Gherkin, §9 design decisions, §10 completeness criteria.

**Finding**: The bis fix changed "8 alert rules" to "14 alert rules (4 SEV-1 + 5 SEV-2 + 5 SEV-3)" everywhere. But the actual YAML (`dash-rate.alerts.yaml`) defines:
- SEV-1 (4): global-circuit-trip, isolation-violation, auto-suspend-attempt-LGPD-canary, sli-distinction-regression
- SEV-2 (6): abuse-admin-review, pat-misuse, manual-override, blocklist-drift, appeals-queue-overflow, **storage-100pct** (6, not 5)
- SEV-3 (5): storage-95pct, single-signal-alarm, appeal-sla-breach, suggest-block-sla-breach, abuse-calibration-drift

Total: 4+6+5 = **15 rules**, not 14. The paragraph §2 explicitly says "SEV-2 (5): abuse admin review + PAT misuse + manual override + blocklist drift + appeals queue overflow + storage 100%" — listing 6 items after writing "(5)". The Gherkin scenario §8 says "all 8 fires correctly: 4 SEV-1 + 5 SEV-2 + 5 SEV-3 (total 14 unique conditions; 8 distinct rules with multiple conditions)" which conflates two different counting methods incoherently.

**Impact**: DoD criterion "14 alert rules armed" and CI gate "8 rules deployed" are mutually inconsistent. Compliance reviewer signing off on "14 alert rules" would be approving a different set than the YAML defines.

**Fix**: Choose canonical count (15 is correct from YAML), update all references. Or remove storage-100pct from alert YAML if it was unintentionally added.

### P1-NEW-2: tenant_rate_override read logic absent from WI-S08-001 scope section (CI-2 incompleteness)

**Location**: WI-S08-001 §6.1 (check_and_consume code), §13 artifacts, §10 completeness criteria.

**Finding**: CI-2 fix added `tenant_rate_override` table in WI-S08-004 §6.1.4, and the WI-S08-001 change log says "CI-2 NEW table `tenant_rate_override` shared schema com WI-S08-004 SilentDowngrade communication mechanism (effective_refill_rate = plan_refill * override_multiplier; 5min DO cache; auto-revert on expiry)". However:

1. The `check_and_consume` code in WI-S08-001 §6.1 item 3 does NOT show the D1 read for `tenant_rate_override`. An implementer reading only the scope section would not know to query this table.
2. WI-S08-001 §13 artifacts does not list a migration for `tenant_rate_override` — it lists only `rate_limiter_state` and `tenant_plan`. The table definition lives only in WI-S08-004; the reader of WI-S08-001 must cross-reference WI-S08-004 to find it.
3. No completeness criterion in WI-S08-001 validates that `tenant_rate_override` is checked before rate-limiting.
4. The 5min DO cache for the override: if the abuse detector sets a SilentDowngrade at T0, the DO won't see it until DO memory cache refreshes — up to 5min delay before downgrade takes effect. This creates a window where abuse continues at full rate. This lag is unspecified and has no alert.

**Fix**: Add D1 read of `tenant_rate_override` to `check_and_consume` code spec in WI-S08-001 §6.1. Add migration artifact reference. Add completeness criterion for the integration. Document the 5min cache lag and add SEV-3 alert if override is stale > 10min.

### P1-NEW-3: abuse_response_actions.expires_at NOT NULL contradicts AdminReview/SuspendCandidate semantics (Gap 4 — unfixed)

**Location**: WI-S08-004 §6.1.7, `abuse_response_actions` DDL (line 433).

**Finding**: The DDL comment says `-- when downgrade lifts (or NULL for permanent)` — but the column is declared `expires_at INTEGER NOT NULL`. These two statements directly contradict each other. For `AdminReviewTriggerSev2` and `SuspendCandidateHumanReviewOnly` response tiers, there is no natural expiry — these are pending-human-review states. The NOT NULL constraint forces an arbitrary value. No sentinel value convention is documented. The CHECK constraint `CHECK (expires_at > applied_at)` would reject `NULL` (which SQLite evaluates as `NULL`, effectively skipping the check) but the NOT NULL constraint would already reject it first.

This was identified as Gap 4 in round 1. The bis cycle did NOT fix it. The comment `-- or NULL for permanent` survives alongside NOT NULL in the same line, creating a direct schema contradiction implementers must resolve arbitrarily.

**Fix**: Either (a) make `expires_at INTEGER` nullable and update CHECK to `(expires_at IS NULL OR expires_at > applied_at)`, documenting NULL = "pending human review, no time bound"; or (b) keep NOT NULL and document a sentinel value (e.g., `applied_at + 365*24*3600*1000` = 1 year as de-facto open-ended) and add a `CHECK` verifying the sentinel usage. Option (a) is semantically cleaner.

### P1-NEW-4: WI-S08-003 §13 artifacts still lists `pat_rate_state.sql` after rename to `pat_rate_observation`

**Location**: WI-S08-003 §13 artifacts table (line 740), §9 design decisions (line 701).

**Finding**: The bis fix renamed D1 table to `pat_rate_observation` (DDL at line 513 confirms). However, the artifacts table (§13) and design decisions section (§9.11) still reference `migrations/00X_pat_rate_state.sql`. The DDL is correct; the artifact filename reference is stale. An implementer creating the migration file would use the wrong name.

**Fix**: Update §13 artifact path to `migrations/00X_pat_rate_observation.sql`. Update §9.11 reference.

---

## Residual P2 findings (from round 1, unfixed)

### P2-1: ratio_4xx == 1.0 exact float equality in suggest_block trigger (WI-S08-002)

**Location**: WI-S08-002 §6.1 item 6 (line 378): `rps > 10000 AND ratio_4xx == 1.0 AND duration ≥ 5min`.

**Status**: UNFIXED from round-1 P2-4. Exact float equality on a computed ratio. A single health-check response within the 5-minute window drops ratio from 1.0 to 0.9999 and suppresses the block suggestion. Should be `ratio_4xx >= 0.99`.

### P2-2: ipnet WASM compatibility unaddressed (WI-S08-002)

**Location**: WI-S08-002 §6.1 item 7, §17 sub-tasks ST-006.

**Status**: UNFIXED from R4 P1-10 (carried as P2 in round-1). The `ipnet` crate is referenced for CIDR parsing in `crates/corelink-edge-blocklist/`. This crate runs in the admin-push integration code path. If that code path runs in a CF Workers WASM isolate, `ipnet` must be verified as `wasm32-unknown-unknown` compatible. No explicit WASM compatibility note or `[target.'cfg(target_arch = "wasm32")']` annotation is present. The reconcile DO runs as CF Worker — if the admin-push integration compiles CIDR validation into a WASM module, `ipnet` will fail unless it has `no_std` support. Should add a completeness criterion: `cargo build --target wasm32-unknown-unknown` for the edge-blocklist crate.

### P2-3: Migration file 00X placeholders across all 6 WIs

**Status**: Noted in R4 P2-6; unfixed. All WIs use `00X` placeholder migration numbers. While this is acceptable in spec-first WIs (actual numbers assigned during implementation), the DoD criterion should require that the implementing developer assign sequential numbers before SEALED status. Currently no criterion enforces this.

### P2-4: Cost analysis arithmetic error in WI-S08-001 (from R4 P2-2)

**Location**: WI-S08-001 §22 Cost Analysis (line 475).

**Status**: UNFIXED. "TCO 12m: 5 regions × 1k tenants × 1k requests/sec × 86400 × 365 × $0.0000005 = ~$78840/yr" — The actual arithmetic gives $78,840,000/yr (off by 1000×). The formula assumes every tenant sustains 1000 RPS 24/7/365 across 5 regions. At realistic 1 RPS average: $78,840/yr. The calculation is arithmetically wrong by 3 orders of magnitude. A correct statement would be: "$78,840/yr at 1 RPS average per tenant (peak 1000 RPS would cost $78.8M/yr — DO budget justification required at enterprise scale)."

### P2-5: Calibration CI for 2/50 uses Wald interval (understated)

**Location**: WI-S08-004 §3 SLA addendum (line 329), sprint contract §6 DoD.

**Finding**: "≤2 FP in 50 → CI upper 9.6% acceptable initial" — this appears to use the Wald interval approximation (≈9.4%) which is the least conservative method and known to underperform at low counts. The more conservative Clopper-Pearson (exact) two-sided 95% upper bound for 2/50 is approximately 12-13%. The stated "9.6%" misleads the Data Scientist sign-off into approving a higher FP rate than acknowledged.

The WI says this is "acceptable initial" but doesn't document which CI method was used. If the actual FP rate is 9.6% and they declare the CI shows ≤9.6%, they're not proving anything meaningful. Recommend documenting: "Clopper-Pearson exact two-sided 95% CI; actual upper bound for k=2 is ~13%; rounding to 9.6% uses Wald approximation which underestimates at low n."

### P2-6: WI-S08-003 abuse_calibration_weights.calibrated_via still says "staging_10_workloads"

**Location**: WI-S08-004 §6.1.7 `abuse_calibration_weights` DDL (line 450): `calibrated_via TEXT NOT NULL, -- "staging_10_workloads_2026-04-25"`.

**Finding**: The comment example `"staging_10_workloads_2026-04-25"` uses the OLD n=10 sample count. Post-bis, calibration uses n=50+50. The canonical value should read `"staging_50plus50_workloads_2026-04-25"`. This is a documentation inconsistency in the DDL comment — minor but creates confusion when the calibration data is queried post-launch.

---

## Cross-WI integration issues (post-bis)

### XWI-1: tenant_rate_override D1 expiry and WI-S08-001 DO cache race

The CI-2 fix documents the mechanism but leaves one race unaddressed: the `tenant_rate_override` entry has `expires_at` (e.g., `applied_at + 3600000` for 1h SilentDowngrade). WI-S08-001 DO caches the override for 5min. If the override expires at T0+1h but the DO cache refreshes at T0+1h-4min (just before expiry), the DO will hold a stale override for up to 5min post-expiry. During those 5min, the tenant rate is still throttled 50% after the 1h window. No alert covers this overshoot. The impact is minor (customer is under-served post-recovery by up to 5min) but the SilentDowngrade SLA claim "auto-recover after 1h" is not guaranteed to be exactly 1h.

### XWI-2: per-PAT detection rate observable-vs-blocking confusion in cross-WI scenario (WI-S08-006 chaos scenario 2)

WI-S08-006 chaos scenario 2: "Abuse silent-downgrade reduces rate cap by 50% (cross-WI WI-S08-001 + WI-S08-004 integration)." This test is meaningful and correct. But chaos scenario 11 ("SLI denominator correctness: 1000 over-quota requests + 100 within-quota") doesn't account for the `per_pat` type, which is now a detection-only tier that always returns `Ok`. If `per_pat` never generates a 429 at camada 3, the SLI test assertion that `per_pat` type belongs in the "over-quota excluded" bucket is vacuously true (per_pat never fires, so nothing to count). The test should verify: when camada 1 fires a `tenant_quota` 429 due to a compromised PAT, the resulting counter is `within_quota` (camada 1 type), not `per_pat`. This is a subtle but important distinction for SLI correctness under adversarial PAT scenarios.

---

## Residual gaps from round-1 (addressed or not)

| Gap | Status |
|---|---|
| Gap 1: prop test for DO alarm scheduling correctness | STILL OPEN. Property test verifies period transition detection (string "YYYY-MM" comparison) but NOT that the DO alarm is scheduled at exact next-month boundary time. |
| Gap 2: adversary weight-gaming without mitigation in S-08 | ACKNOWLEDGED in Persona 3.1 and §2 adversarial, no S-08 mitigation added. Acceptable (known limitation noted). |
| Gap 3: HalfOpen observation_id cold-start determinism | STILL OPEN. observation_id definition not specified. Cold-start reset not addressed. |
| Gap 4: expires_at NOT NULL contradiction | UNFIXED — promoted to P1-NEW-3 above. |
| Gap 5: Compliance LGPD waivability under ADR-0034 | UNCHANGED — WI-S08-001 §30 still marks Compliance as "pending" without non-waivable designation for the automated rate-limit decision review. |

---

## Methodological strengths (unchanged from round 1 + new)

1. **P0-D implementation quality**: `next_month_first_utc_midnight()` correctly handles December → January using conditional year increment and `NaiveDate::from_ymd_opt`. This is a clean implementation.

2. **P0-C structural reframing**: The trait signature change (`check_pat_rate` always returns `Ok`), `PatRateOver` removal, and D1 table rename are architecturally clean. The residual framing errors (P0-NEW-1) are in prose only, not in the type system.

3. **ManualOverride SLI exclusion**: `counts_against_sli(&self, trip_reason: Option<&TripReason>)` pattern is well-designed. The option type correctly handles callers that don't have TripReason context.

4. **CI-3 orphaned reservation doc**: WI-S08-005 §1 invariant 12 is thorough and correctly characterizes the TTL-based sweep recovery. The operational SEV-3 alert on `reservations_active > 100` is appropriate.

5. **tenant_rate_override CHECK constraints**: The inline constraint `CHECK (override_multiplier >= 0.0 AND override_multiplier <= 1.0)` is correct and prevents invalid multipliers (negative or >1.0 would amplify rate, not downgrade it).

---

## Verdict

**APPROVED CONDITIONALLY** — must fix before SEALED:

**Mandatory pre-SEAL (P0):**
1. **P0-NEW-1**: Fix §2 adversarial + §6.1 chaos 6 enforcement framing in WI-S08-003; rename `TokenBucketState` → `PatRateObservationState`; rename `prop_pat_rate_cap` → `prop_pat_rate_misuse_detection_threshold`.
2. **P0-NEW-2**: Fix 5 stale "13 sign-offs" references in WI-S08-006 (lines 240, 449, 472, 502, 548); correct Grafana panel title; fix chaos scenario 7 logic.

**Mandatory before PRR sign-off (P1):**
3. **P1-NEW-1**: Reconcile alert count — YAML has 15 rules (4+6+5), not 14. Update all claims to canonical count.
4. **P1-NEW-2**: Add `tenant_rate_override` D1 read to WI-S08-001 §6.1 check_and_consume spec; add migration artifact reference; add completeness criterion; document 5min cache lag and overshoot alert.
5. **P1-NEW-3**: Fix `abuse_response_actions.expires_at` NOT NULL contradiction; allow NULL for admin-review states or document sentinel value convention.
6. **P1-NEW-4**: Update WI-S08-003 §13 artifact to `pat_rate_observation.sql`; update §9.11.

**Carry-forward P2 (pre-launch advisory):**
- P2-1: ratio_4xx >= 0.99 (not == 1.0)
- P2-2: ipnet WASM compatibility gate
- P2-4: Cost analysis arithmetic ($78.8M vs $78.8K)
- P2-5: Specify CI method for calibration claims (Clopper-Pearson preferred)
- P2-6: calibration_weights comment example update

With P0s and P1s resolved, S-08 should reach ≥ 8.8/10 and clear the SOTA bar for SEALED status.

---

*Review conducted 2026-04-25 by Sonnet R5 (Claude Sonnet 4.6, round 2 / tris — different model lineage from Opus R4 reviewer). No prior conversation context from round-1 review. Fresh read of all 6 WIs and sprint contract v1.1.0.*
