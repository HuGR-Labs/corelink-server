---
id: "AUDIT-R5-SONNET-S09-PART2"
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
inherits_from:
  - "FRAMEWORK-00"
  - "OBSERVABILITY-MODEL"
  - "SLO-CATALOG"
  - "INVARIANT-REGISTRY"
tags: ["audit", "review", "r5", "sonnet", "s09", "observability", "testability", "lote-10.9", "part2", "wi-005-to-007"]
references:
  - "specs/04_sprints/_sealed/S09/_spec_contract.md"
  - "specs/04_sprints/_sealed/S09/_review_R5_sonnet_part1.md"
  - "specs/04_sprints/_sealed/S09/_review_R4_opus_part2.md"
  - "specs/04_sprints/_sealed/S09/work_items/WI-S09-005-12-grafana-dashboards-as-code.md"
  - "specs/04_sprints/_sealed/S09/work_items/WI-S09-006-multi-burn-rate-slo-alerts-pagerduty.md"
  - "specs/04_sprints/_sealed/S09/work_items/WI-S09-007-synthetic-canary-3-regions-runbook-dry-run.md"
---

# R5 (Sonnet) — S-09 Testability + Validator Coverage Review · Part 2 (WI-S09-005..007)

> **Reviewer persona:** R5 — Sonnet testability + structural-validator
> reviewer. Focus: property test density, structural validators, chaos-test
> framework, synthetic canary coverage, alert flapping detector calibration.
> **Scope:** S-09 WI-005..007 (12 Grafana dashboards-as-code; multi-burn-rate
> SLO alerts + PagerDuty; synthetic canary 3 regions + runbook dry-run)
> **Companion:** `_review_R5_sonnet_part1.md` covers WIs 1..4
> **Cross-cut:** R4 Opus architectural review in `_review_R4_opus_part{1,2}.md`
> **Framework citations:** Framework §31 (testability), §32 (proptest density
> gate); Google SRE Workbook Ch 5 Table 4 (multi-burn-rate); Ch 8 (alert
> discipline + flapping detector); PagerDuty Events API v2 §dedup_key.

---

## 0. Executive summary

**Testability aggregate (WI-005..007): 8.2 / 10.** Slightly below Part 1
(8.7). The dashboards-as-code piece (WI-S09-005) tests *structural validity*
(`validate_dashboards.py` JSON parse + count + tags) but does NOT test
*semantic correctness* (panel titles reference registered IDs; query strings
align with metric exposition; AdminCtx-RBAC enforcement actually fires).
WI-S09-006 ships strong property tests on the multi-burn-rate decision
table (`prop_alert_decision_canonical_table4` 10k iter) but the YAML
recording rules have no equivalent lint. WI-S09-007 ships the canary
property test corpus (75 tests) but the chaos-test framework executing
the 12 catalogued scenarios is not at SEAL — same gap as Part 1 P1-S4.

Headline P0/P1 cluster:

1. **P1-S5 — Alert flapping detector calibration is hand-tuned without
   sensitivity analysis.** The "auto-quarantine flapping > 3×/week"
   threshold per sprint contract §14.s09.2 is a single hand-tuned number.
   There is no property test exercising the calibration across realistic
   alert-fire distributions (e.g. Poisson with λ=2/week vs λ=4/week —
   the false-quarantine rate diverges sharply at the boundary). Need a
   `prop_flapping_detector_calibration_robust` test that sweeps λ ∈
   {0.5, 1, 2, 3, 5, 10}/week and asserts the false-quarantine rate is
   < 1% for λ ≤ 2 and quarantine catches > 99% of legitimate flapping
   for λ ≥ 5.
2. **P1-S6 — Flapping detector ↔ ticket-arm interaction has no
   property test.** R4 Part 2 P1-B flags this as an architectural gap;
   from a *testability* perspective, the gap is that there is no
   property test `prop_flapping_detector_carves_out_ticket_arm` 10k iter
   pinning that the quarantine logic ONLY suppresses page-arm rules
   (SEV-0, SEV-1), never ticket-arm rules (SEV-2, SEV-3).
3. **P1-S7 — Canary synthetic-tenant exclusion enforced in two
   independent places.** The exclusion lives in (a) Rust crate
   `prop_synthetic_tenant_excluded_from_sli` 10k iter, AND (b) YAML
   PromQL recording rule. The two are NOT cross-validated. A change in
   one without the other silently breaks the discipline. Need a single
   source of truth — preferably the Rust crate emits the YAML at build
   time, OR the YAML is parsed at test time and the canary tenant UUID
   is asserted to match the Rust const.
4. **P1-S8 — INV drift triple in WI-005-12 dashboards.** Cross-reference
   Part 1 P1-S1; this is the testability-side framing — the validator
   chain (`validate_inv_promotion.py`) must be green for Lote 10.9
   review to seal.

If the dev applies all 4 P1 findings, projected score: **9.0 / 10.**

## 1. Per-WI testability scores

| WI | Score | Property test density | Structural validator | Falsifiability target |
|---|---|---|---|---|
| **WI-S09-005** 12 Grafana dashboards-as-code | **8.0 / 10** | NO Rust property tests (dashboards are JSON); `validate_dashboards.py` ~210 LOC structural lint | JSON parse + canonical-12 count + panel count ≥ 8 + template variables (`region` / `tenant_tier` / `tenant`) + datasource consistency + tags (`corelink`, `sota`) + `lastUpdated` ISO 8601 + `cardinality_budget_used` annotation | **STRUCTURAL ONLY** — no semantic check (INV reference, query string alignment, AdminCtx-RBAC enforcement) |
| **WI-S09-006** Multi-burn-rate SLO alerts + PagerDuty | **8.4 / 10** | 83 tests; `prop_alert_decision_canonical_table4` 10k iter load-bearing falsifiability; `prop_pagerduty_dispatch_idempotent_dedup_key` 10k iter pins Events API v2 §dedup_key contract; `prop_synthetic_tenant_excluded_from_sli` 10k iter | `BurnRateWindow` `#[non_exhaustive]` 4-canonical Fast1h/Medium6h/Slow24h/Long3d; `AlertDecision` `#[non_exhaustive]` 5-canonical Quiet/TicketSev3/TicketSev2/PageSev1/PageSev0; `Sli` `#[non_exhaustive]` 7-canonical from `slo_catalog.md §4.x`; `PagerDutyEventAction` `#[non_exhaustive]` 3-canonical Events API v2; `PagerDutyServiceKey` `#[non_exhaustive]` 3-canonical Staging/ProdUs/ProdEu | Google SRE Workbook Ch 5 Table 4 — for every (sli, window, target_pct, sample) tuple decision matches Table 4 exactly |
| **WI-S09-007** Synthetic canary 3 regions | **8.2 / 10** | 75 tests; `prop_canary_digest_correctness` 10k iter pins precedence (digest mismatch → FailedRegion regardless of every other input); `prop_canary_assertion_ladder_precedence` 10k iter; `prop_canary_region_canonical` 10k iter | `CanaryRegion` `#[non_exhaustive]` 3-canonical (Enam + Weur + Apac per R2 region hints — Lote 10.9bis P1 R4 P1-10 corrected); `CanaryProbe` trait + `InMemoryCanaryProbe` per-instance F-001 closure | BLAKE3 digest match precedence + observability stack health probe + dispatch lag detection |

**Aggregate (mean): 8.20 / 10.**

## 2. P0 findings (testability-blocking)

**None.** The property-test corpus + structural validators are dense enough
to satisfy framework §32 proptest density gate. The P1 cluster is the
actionable lot.

## 3. P1 findings

### P1-S5 — Alert flapping detector calibration is hand-tuned without sensitivity analysis

**Severity:** P1 — load-bearing for SRE Workbook Ch 8 alert discipline.

**Defect:** WI-S09-006 §0 + sprint contract §14.s09.2 ship a single
threshold "auto-quarantine flapping alerts > 3×/week". This is hand-tuned;
no property test exercises the calibration across realistic alert-fire
distributions. The false-quarantine rate (legitimate alerts wrongly
suppressed) diverges sharply at the threshold boundary.

For a Poisson process with rate λ alerts/week:
- λ = 1: P(≥ 4 fires/week) = 0.019 (1.9% false-quarantine rate — acceptable)
- λ = 2: P(≥ 4 fires/week) = 0.143 (14.3% false-quarantine rate — too high)
- λ = 3: P(≥ 4 fires/week) = 0.353 (35.3% false-quarantine rate —
  unacceptable)

The 3×/week threshold is **too aggressive** for any SLI with > 1 alert/week
baseline rate. This is precisely the "noisy SLI" problem — high-traffic
SLIs (like DASH-CAS-PUT for prod-us with 10k QPS) will legitimately fire
multi-burn-rate alerts > 3×/week during ordinary operation.

**Concrete remediation:**

1. Add `prop_flapping_detector_calibration_robust` test that sweeps λ ∈
   {0.5, 1, 2, 3, 5, 10}/week (16 canonical seeds × 6 λ values = 96
   sweeps).
2. Acceptance criteria: false-quarantine rate < 1% for λ ≤ 2; true-
   positive rate > 99% for λ ≥ 5.
3. If acceptance fails, recalibrate threshold (likely to ≥ 5×/week, or
   adaptive — quarantine when fires > μ + 3σ for the last 30 days of
   that specific rule).
4. Document the calibration rationale in WI-006 §6 with the explicit
   distribution assumption.

### P1-S6 — Flapping detector ↔ ticket-arm interaction has no property test

**Severity:** P1 — cross-references R4 Part 2 P1-B; the architectural carve-
out exists in the SRE Workbook but is not in WI-006 §0 and is not pinned
by a property test.

**Defect:** Per Google SRE Workbook Ch 8 §"Tickets, Pages, and Alerts":
flapping discipline applies to **page** arm (SEV-0, SEV-1) only, NOT
**ticket** arm (SEV-2, SEV-3). The multi-burn-rate 4-window canonical
PRODUCES legitimate ticket-arm flapping on slow burns (Slow24h fire +
clear + re-fire pattern over a 7d sustained slow burn). Quarantining the
ticket arm would suppress the legitimate slow-burn signal.

WI-006 §0 cites "auto-quarantine flapping alerts > 3×/week" without the
page-vs-ticket carve-out. No property test pins the discipline.

**Concrete remediation:**

1. Add `prop_flapping_detector_carves_out_ticket_arm` 10k iter:
   ```rust
   proptest! {
       #[test]
       fn prop_flapping_detector_carves_out_ticket_arm(
           rule in any::<AlertRule>(),
           fire_count in 4u32..20u32,
       ) {
           let quarantined = flapping_detector::should_quarantine(&rule, fire_count);
           if matches!(rule.severity, Severity::Sev2 | Severity::Sev3) {
               prop_assert!(!quarantined, "ticket-arm rules MUST NOT quarantine");
           }
       }
   }
   ```
2. Update WI-006 §6 with the explicit carve-out + property test reference.
3. Cross-reference SRE Workbook Ch 8 §"Tickets, Pages, and Alerts" in
   §6.

### P1-S7 — Canary synthetic-tenant exclusion enforced in two independent places

**Severity:** P1 — single-source-of-truth violation; silent drift hazard.

**Defect:** The canary synthetic-tenant UUID
(`00000000-0000-0000-0000-canary000000`) appears in:

1. **Rust const** in `corelink-canary::CanaryTenantId` (pinned by
   `prop_synthetic_tenant_excluded_from_sli` 10k iter at the SLI
   evaluation boundary)
2. **YAML PromQL recording rule** in `dashboards/alerts/dash-slo-multi-
   burn.yml` (e.g. `tenant_id != "00000000-0000-0000-0000-canary000000"`)

The two are NOT cross-validated. If a future operator:
- Renames the canary tenant in Rust without updating YAML → SLI denominator
  includes canary traffic
- Edits the YAML tenant exclusion (typo or deletion) without updating
  Rust → SLI denominator includes canary traffic

In both cases, the synthetic canary inflates the real-tenant SLI signal
(the Lote 10.8bis P1-NEW-3 regression class).

**Concrete remediation (single source of truth):**

1. **Generate YAML from Rust const at build time.** Add a
   `crates/corelink-slo/build.rs` that emits the YAML recording rule with
   the canary tenant UUID interpolated from the Rust const.
2. **OR** add a unit test that parses the YAML at test time and asserts
   the embedded UUID matches the Rust const exactly.
3. Add `prop_canary_tenant_exclusion_yaml_matches_rust_const` test pinning
   the cross-artifact discipline.

### P1-S8 — INV drift triple in WI-005-12 dashboards

**Severity:** P1 — cross-reference Part 1 P1-S1. From a testability lens:
the `validate_inv_promotion.py` script's exit status is the validator-
chain claim that PRR-S09 §10 makes. The script flags drift. R5 escalates
because this is squarely the validator-chain coverage R5 audits.

See Part 1 P1-S1 for the concrete remediation matrix:

- `INV-CAS-DIGEST-INTEGRITY` → rename to `INV-CAS-INTEGRITY` (alias drift)
- `INV-EXEC-IDEMPOTENT` → promote to §3.12 (S-09 row)
- `INV-LGPD-AUTO-SUSPEND-FORBIDDEN` → promote to §3.12 (S-09 row)

**Additional R5 lint:** extend `validate_dashboards.py` with a semantic-lint
pass: any `INV-*` substring in panel titles or descriptions MUST match a
registered `invariant_registry.md §3` row ID. Add to CI.

## 4. P2 findings

### P2-S5 — `prop_alert_decision_canonical_table4` adversarial sample distribution

`prop_alert_decision_canonical_table4` 10k iter pins the decision matches
Table 4 for every (sli, window, target_pct, sample) tuple. The proptest
default generators may under-explore boundary values (exactly at 14.4×
multiplier, exactly at 5min window).

**Concrete remediation:**

- Add canonical-boundary-value seeds: sample IN {14.39, 14.40, 14.41,
  6.0, 5.99, 6.01, ...}; window IN {299, 300, 301 seconds}.
- Add `prop_alert_decision_boundary_canonical` 10k iter with boundary-
  weighted distribution.

### P2-S6 — Canary digest mismatch precedence test

`prop_canary_digest_correctness` 10k iter pins precedence (digest mismatch
→ FailedRegion regardless of every other input). The "regardless of every
other input" claim should be exhaustive at the type level — i.e. the
assertion ladder positions BEFORE the digest check (`cas_put_p99`,
`cas_get_p99`, `ac_lookup_p99`) should ALL be exercised in the same
property test, with digest mismatch forcing FailedRegion across the
cartesian.

### P2-S7 — Synthetic canary 12_960 loops/72h ↔ alert dispatch latency

R4 Part 2 P1-E flags the architectural ambiguity. From a testability lens,
there is no property test asserting: "given a canary assertion failure at
time T, the SEV-2 alert is dispatched within MTTA target 5 minutes".
Suggest adding `prop_canary_alert_dispatch_within_mtta` 10k iter that
constructs a failure at time T and asserts dispatch ≤ T + 5min.

### P2-S8 — Runbook dry-run drift detection

WI-S09-007 §1 ships `scripts/rb_fm_153_dry_run.sh` + `scripts/rb_obs_
cardinality_001_dry_run.sh` with "drift-detectable" claim. The drift
detection mechanism is unspecified — what catches a runbook MD step
added without an updated dry-run script?

**Concrete remediation:**

- Add `scripts/validate_runbook_dry_runs.py` that parses `## Step N`
  headers in runbook MD + counts `echo "Step N"` markers in the script;
  rejects mismatches.

## 5. P3 findings

### P3-S4 — `validate_dashboards.py` test corpus

The validator ships ~210 LOC but does not ship its own test corpus
verifying false-positive / false-negative rates. Suggest adding a
`tests/validate_dashboards/` directory with:
- A "happy path" dashboard JSON that should pass
- A "missing panel count" dashboard that should fail
- A "drift INV reference" dashboard that should fail (once P1-S8 lints
  land)
- A "missing template variable" dashboard that should fail

### P3-S5 — PagerDuty Events API v2 dedup_key construction

WI-S09-006 ships `dedup_key = {sli_slug}:{window_slug}:{tenant_id}`. The
construction is canonical and pinned by `prop_pagerduty_dispatch_
idempotent_dedup_key` 10k iter. Suggest adding a property test that
exercises the **collision space**: across the cartesian (7 SLI × 4
window × N tenants), assert no two distinct alert instances produce the
same dedup_key.

### P3-S6 — Canary 3-region naming canonical

`CanaryRegion::Enam` / `Weur` / `Apac` follow R2 region hints per
`data_model.md §2.1`. Suggest a CI lint that scans all observability
artifacts (dashboards, alerts, metrics) for the canonical region names
and rejects non-canonical variants (e.g. `us-east`, `eu-west`, `ap-
south` — these were the pre-correction values per Lote 10.9bis P1 R4
P1-10).

## 6. Cross-WI structural validator coverage (Part 2 addendum)

| Validator | Layer | Covers | Gap |
|---|---|---|---|
| `validate_dashboards.py` | YAML/JSON structural | 12-count + panel ≥ 8 + tags + lastUpdated + cardinality_budget_used | **INV drift triple** (P1-S8); semantic lint missing; recency lint missing |
| `validate_alert_rules.py` | YAML structural | **NOT SHIPPED at SEAL** | Synthetic-tenant exclusion lint (P1-S7); window canonical shape lint |
| `validate_runbook_dry_runs.py` | Cross-MD/script | **NOT SHIPPED at SEAL** | Runbook drift detection (P2-S8) |
| `BurnRateWindow` `#[non_exhaustive]` | Rust type | 4-canonical Fast/Medium/Slow/Long | Boundary-value coverage (P2-S5) |
| `AlertDecision` `#[non_exhaustive]` | Rust type | 5-canonical Quiet/TicketSev3/TicketSev2/PageSev1/PageSev0 | Flapping-detector carve-out (P1-S6) |
| `Sli` `#[non_exhaustive]` | Rust type | 7-canonical from `slo_catalog.md §4.x` | None — well-bounded |
| `PagerDutyServiceKey` `#[non_exhaustive]` | Rust type | 3-canonical Staging/ProdUs/ProdEu | Cross-region tenant dispatch (R4 Part 2 P2-B) |
| `CanaryRegion` `#[non_exhaustive]` | Rust type | 3-canonical Enam/Weur/Apac | Canonical region name lint (P3-S6) |
| `prop_alert_decision_canonical_table4` | Statistical / property | Google SRE Workbook Ch 5 Table 4 | Boundary-value distribution (P2-S5) |
| `prop_canary_digest_correctness` | Property | Digest mismatch precedence | Cartesian over assertion ladder (P2-S6) |
| `prop_synthetic_tenant_excluded_from_sli` | Property | Rust-side intent | YAML cross-artifact alignment (P1-S7) |
| Chaos suite (12 catalogued scenarios) | Narrative | 12 adversarial scenarios | **Not executable at SEAL** (Part 1 P1-S4) |

## 7. Recommendation

**CONDITIONAL APPROVE for testability** for WIs 005..007 → STAGING-STABLE
*contingent* on:

- **P1-S5** (flapping detector calibration sensitivity) MUST land in Lote
  10.9bis remediation before any HIGH-traffic SLI ships to prod-us /
  prod-eu — the 3×/week threshold is too aggressive for λ > 1/week SLIs;
- **P1-S6** (flapping ↔ ticket carve-out) MUST land in Lote 10.9bis
  remediation;
- **P1-S7** (canary tenant exclusion single source of truth) MUST land
  before any production tenant onboarding — silent SLI inflation is the
  same defect class as the Lote 10.8bis P1-NEW-3 regression;
- **P1-S8** (INV drift triple) MUST land per Part 1 P1-S1 remediation
  matrix.

Per Lote 10.9 review charter: this report is an AUDIT artifact (`type:
audit`, `doc_status: REVIEW`) and does NOT modify WI-S09-005..007 doc_status
(which remain FROZEN per the original SEAL). Remediation lands as a separate
Lote 10.9bis WI.

**Calibrated praise:** the 158-test corpus (75 canary + 83 SLO) across
WIs 006..007 is dense; `prop_alert_decision_canonical_table4` and
`prop_canary_digest_correctness` are the right falsifiability targets at
the right granularity. WI-005's `validate_dashboards.py` is a SOLID
structural-lint foundation — the gap is purely *semantic-lint extension*,
not first-principles design. The Lote 10.9-quaters NEW-P0-1 correction
landing the 12 canonical dashboards aligned with `observability_model.md
§10` Nível-3 is exactly the discipline R5 audits for.

The S-09 testability foundation, with the 4 P1 findings addressed, lands at
**9.0 / 10** — within the SOTA bar. The remaining gap to ~10 is the chaos-
suite executable framework (Part 1 P1-S4) which is appropriately scoped as
a S-17 forward-looking dependency.

---

**End R5 Sonnet Part 2 v1.0.0.**

---

## Closure footnote (Lote 10.9bis wave 17 — 2026-05-15)

**P1-S8 INV drift triple — CLOSED.** Remediation matrix applied:

- `INV-CAS-DIGEST-INTEGRITY` → renamed to canonical `INV-CAS-INTEGRITY` (registry §3.2; alias drift).
- `INV-EXEC-IDEMPOTENT` → promoted to registry §3.12 (S-09 row, HIGH).
- `INV-LGPD-AUTO-SUSPEND-FORBIDDEN` → promoted to registry §3.12 (S-09 row, HIGH; LGPD Art. 20 + GDPR Art. 22).

`validate_inv_promotion.py` drift 3 → 0; 4 quality-gate validators exit 0.
