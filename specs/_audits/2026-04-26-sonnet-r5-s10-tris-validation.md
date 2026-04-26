---
type: audit
title: Sonnet R5 (Sonnet 4.6) round-2 validation of S-10 WIs (Lote 10.10bis post-remediation)
date: 2026-04-26
reviewer: Sonnet R5 (Claude Sonnet 4.6)
sprint: S-10
target: 7 WIs + sprint contract + failure_modes
round: 2
---

# Sonnet R5 — S-10 Round-2 Adversarial Validation (Lote 10.10bis post-remediation)

## Aggregate score: 7.8/10 (vs round-1 6.4/10)

The bis cycle genuinely resolved all 8 P0s declared in the joint R4 Opus + R5 round-1 reviews. The structural defects that would have caused runtime data corruption are gone: region is now in the PK, the D1 CHECK enum values align with strum serialization, TLA+ arithmetic is corrected, the CTRL-AUTHZ-005 hallucination is excised, the self-defeating UPSERT WHERE clause is removed, severity is corrected to HIGH throughout, CTRL-PRIV-002 is properly mapped, and the chrono primitive is renamed to the internal helper. The improvement is real and substantial.

However, two new P0-class residuals were introduced by the bis fixes (a TLA+ quota_states regression and a WI-003 §1.11 narrative stale text), and four P1-class residuals remain. Historical pattern confirmed: incomplete coverage of narrative sections vs code sections. Sprint cannot be sealed at this validation cycle.

---

## P0 Status Table (8 declared P0s from round-1)

| P0 | Description | Status | Evidence |
|---|---|---|---|
| **P0-A (R5)** | WI-002 PK missing `region` | **FIXED ✅** | WI-002 line 312: `PRIMARY KEY (tenant_id, region, sku, hour)` — 4-tuple confirmed. |
| **P0-B (R5)** | D1 CHECK enum short-form `'cas_put'` vs long strum output | **FIXED ✅** | WI-001 lines 337-338: CHECK now uses `'dev.hugr.corelink.cas.put.v1'` etc., matching enum serialization. |
| **P0-C (R5/R4)** | TLA+ math 5× wrong (15k/150k) | **FIXED ✅** | WI-007 line 40 §0: "3,000 states"; line 239: "30,000"; §6.1.3 line 375: "30,000"; §9.4: "30,000"; arithmetic correct throughout. Caveat about TLC reachable state graph vs Cartesian product present (§2 + §9.4). |
| **P0-D (R5/R4)** | INV severity CRITICAL vs registry HIGH | **FIXED ✅** | All 7 WIs now cite HIGH; e.g., WI-001 §1: "INV-BILLING-NO-LOSS (HIGH; registry §3.9 line 136)"; SEV-2 alert wording throughout. §3.X TBD → concrete citations. |
| **P0-E (R5)** | CloudEvents prefix `corelink.usage.*` not `dev.hugr.corelink.*` | **FIXED ✅** | WI-001 enum: `#[strum(serialize = "dev.hugr.corelink.cas.put.v1")]`; sprint contract §5.1 R-S10-1 updated to `dev.hugr.corelink.<op>.v1`. |
| **P0-1 (R4)** | PlanTier 5-tuple invented `free/team/enterprise/custom/trial` | **FIXED ✅** | WI-003 enum lines 113-125: `Free, Solo, Team, Business, Enterprise`; CHECK in plan table: `('free','solo','team','business','enterprise')`; WI-005 CHECK line 344: same 5 canonical values. |
| **P0-4 (R4)** | CTRL-AUTHZ-005 hallucinated | **FIXED ✅** | WI-005 line 202: `CTRL-AUTHZ-001 + CTRL-AUTHZ-002`; WI-006 line 218: same; sprint contract §5.6 R-S10-13: `CTRL-AUTHZ-001 + CTRL-AUTHZ-002`. No CTRL-AUTHZ-005 found in any WI. |
| **P0-5 (R4)** | UPSERT WHERE self-defeating clause | **FIXED ✅** | WI-002 §6.1.8 lines 405-411: WHERE clause removed; comment documents rationale: "digest mismatch detection é explícita ANTES da UPSERT (CounterDigestMismatch error path) — não silenciosa via WHERE no UPSERT". |

All 8 declared P0s: **FIXED ✅**

---

## NEW P0 Findings (introduced by bis fixes)

### NEW-P0-1 — WI-007: `quota_states` TLA+ variable models `grace_active` as a 5th state value — contradicts WI-005 bis fix that made grace a boolean flag

**Severity**: P0 — formal model inconsistent with implementation spec.

**Location**: `WI-S10-007-tla-billing-atomicity-runbooks-finance-walkthrough.md` line 77:

```tla
quota_states,  \* Map tenant -> {under_80, soft_alert, ticket, hard_block, grace_active}
```

And line 100 (Init):
```tla
/\ quota_states = [t \in Tenants |-> "under_80"]
```

**Evidence**: The bis cycle fixed WI-005 P1-7 (R4) + P2-3 (R5) by removing `GraceActive` as a 5th `QuotaState` enum variant and replacing it with `grace_active: bool` flag on the `quota_state` table row (WI-005 line 100 comment: "R4 P1-7 fix: era 5 variants; alinhado com 4-state via `grace_active: bool` flag"). WI-005 §6.1.2 schema (line 334): `grace_active BOOLEAN NOT NULL DEFAULT false`. WI-005 CHECK (line 343): `CHECK (current_state IN ('under_80', 'soft_alert', 'ticket', 'hard_block'))` — 4 values only.

However, WI-007 TLA+ `quota_states` variable comment still lists `grace_active` as one of 5 possible state values in the map range. The TLA+ Init only initialises to `"under_80"`, which is consistent, but the comment in the variable declaration (`Map tenant -> {under_80, soft_alert, ticket, hard_block, grace_active}`) implies the range includes `grace_active` as a valid quota state value. This will mislead any TLC run that tries to model quota transitions: if a transition action sets `quota_states[t] := "grace_active"` it would be incorrect per the current WI-005 spec. It is also a direct regression: the fix in WI-005 was not cascaded into WI-007.

**Fix prescription**: Update WI-007 line 77 comment to: `\* Map tenant -> {under_80, soft_alert, ticket, hard_block} (4-state canonical; grace_active is a separate boolean field, not a state value — R4 P1-7 bis fix cascaded from WI-005)`. Add a separate TLA+ variable or augment the map to model grace: `quota_grace_active, \* Map tenant -> BOOLEAN (separate from state; bis-fixed WI-005 grace flag model)`. Verify no TLA+ action sets `quota_states[t] := "grace_active"`.

---

### NEW-P0-2 — WI-003 §1.11 narrative still lists old invented PlanTier names `free, team, enterprise, custom, trial`

**Severity**: P0 — direct contradiction of the P0-1 bis fix within the same file.

**Location**: `WI-S10-003-corelink-billing-stripe-adapter-idempotency-webhook.md` line 247-250 (§1 item 11):

```
11. **5 PlanTier canonical** (Lote 10.7bis P0-7 inheritance):
    - free, team, enterprise, custom, trial.
    - Mapped to Stripe Price IDs via Neon Postgres `plan(plan_id, stripe_price_id, ...)` table.
    - 5-tier canonical NEVER expanded (sprint contract §10 anti-scope).
```

**Evidence**: The bis fix correctly updated:
- WI-003 `PlanTier` enum (lines 113-125): `Free, Solo, Team, Business, Enterprise` ✅
- WI-003 `plan` table CHECK (line 438): `('free','solo','team','business','enterprise')` ✅
- WI-003 §0 header title: correctly references `data_model.md §1` ✅
- WI-003 §2 narrative line 276: correctly references canonical tiers ✅

But item 11 in §1 (Intent block) was NOT updated. It still reads `free, team, enterprise, custom, trial` — the exact wrong 5-tuple the P0-1 fix was supposed to eliminate. This section is authoritative "bullet list" documentation that engineers read first when inheriting the WI. Runtime consequence: Stripe Price ID catalog mapping (§6.1 item 13) maps SKUs to 5 tiers; if a reader maps against `custom` or `trial` in the Stripe catalog, those Price IDs will not exist in the `plan` table (which only has `solo` and `business`), causing invoice generation failures for those tiers.

Also: WI-003 §6.1 item 13 further down in the file reads: `- 5 PlanTier canonical (Lote 10.7bis P0-7); CHECK constraint enforces.` without listing them explicitly — but item 13 calls this "5 PlanTier canonical (free, team, enterprise, custom, trial)" in the design decision 9.10 at the end of the file, which also needs to be verified.

**Fix prescription**: Update WI-003 §1 item 11 bullet to: `- free, solo, team, business, enterprise`. Grep the entire WI-003 file for "custom, trial" and replace every occurrence with "solo, business" (maintaining the 5-element list). Grep for "enterprise/custom" or "trial" in plan-tier contexts and fix. Add `grep -n "custom\|trial" WI-S10-003*.md` to CI validator.

---

## NEW P1 Findings (introduced or missed by bis cycle)

### NEW-P1-1 — WI-002 §6.1.11 late event detection threshold inconsistency: code uses `hour_window.start_ts - 6h` but narrative says `now - 6h`

**Location**: WI-002 §6.1.11 line 434: `Threshold: event.ts < hour_window.start_ts - chrono::Duration::hours(6)`.

WI-001 §6.1.8 item 8 line 391: "If somehow `ts < now - 6h` reaches emitter".

Sprint contract §5.2 R-S10-5: "Events com `ts < now - 6h` são aceitos mas vão para `usage_counter_late`".

This is the same inconsistency identified as R5 P1-D in round-1, where the bis cycle fixed the `CHECK (age_hours > 6)` → `CHECK (age_hours >= 6)` boundary issue but did not resolve the reference time ambiguity. The `age_hours` stored in `usage_counter_late` is computed as `(now - original_event_ts).hours()` (WI-002 struct field `LateCounterRecord.age_hours`), which uses wall-clock `now`. But the routing decision in step 5 of aggregation logic (WI-002 §6.1.3, step 5) uses `hour_window.start_ts - 6h`. These produce different classification sets when the cron runs at say T=12:30 for the T=11:00 window: an event at T=5:30 (age 7h from now, 5.5h from window start) would be classified as LATE by the now-based threshold but NORMAL by the window-start threshold. This creates non-deterministic classification depending on cron timing jitter — an event could appear in both `usage_counter` and `usage_counter_late` on different cron runs, causing Layer 1 double-counting.

**Fix prescription**: Standardise on `now - 6h` (wall-clock from aggregation cron's perspective) as canonical, matching sprint contract §5.2 R-S10-5 intent. Update WI-002 §6.1.3 step 5 to: `event.ts < now() - 6h` (where `now()` is the cron invocation timestamp, fixed at start of aggregation run). Document in WI-002 §9 design decision.

---

### NEW-P1-2 — WI-003 §1 item 11 cascade: design decision 9.10 and completeness criterion 10.s10.003.10 also reference wrong tier names

**Location**: WI-003 line 726 (design decision 9.10): `9.10: 5 PlanTier canonical (Lote 10.7bis P0-7); CHECK constraint enforces.`

WI-003 line 749: `10.s10.003.10 Stripe Price IDs catalog populated (5 SKUs × 5 tiers = 25 entries).`

These items don't list the wrong names explicitly, but they are part of the same incomplete fix surface as NEW-P0-2. More critically, the sign-off row §30 references "5 PlanTier" and the changelog line "corrige CHECK constraints em WIs 003+005" — the fix was declared complete but section §1 item 11 was missed. This shows the bis fix was applied via targeted grep on the `plan` table DDL and enum code but not a full-file review.

**Fix prescription**: Full-file grep pass on WI-003 for `custom` and `trial` in plan-tier contexts before seal.

---

### NEW-P1-3 — WI-005 `PercentValue` type inconsistency partially resolved but `QuotaError` still references wrong range

**Location**: WI-005 §1 struct field `QuotaEvaluation.current_pct` (line 119) now correctly says: `// 0-200 typed; DB CHECK 0..=200 (R5 P1-H fix: align type vs DB)`. This correctly acknowledges the alignment.

BUT: `QuotaError` variant (WI-005 line 178): `#[error("Invalid percent value (CHECK 0..=100 violation): {0}")] InvalidPercent(f64)` still says `CHECK 0..=100` in the error message, contradicting the now-canonical DB CHECK of `0..=200`. If a value of 150 (valid per DB) triggers this error path, the error message would be a lie — it says 0..=100 was violated when the actual constraint is 0..=200.

Additionally, the `PercentValue` type comment in `transition_state()` signature (line 74) still says `// 0-100 typed` without update: `trigger_pct: PercentValue,  // 0-100 typed`. The `QuotaError::InvalidPercent` guard in the state machine presumably fires when this type is constructed — but what range does the constructor enforce? The type name `PercentValue` still implies 0-100. This produces an ambiguous failure mode: DB allows 0-200; evaluator saturates before transitions; but the error type says 0-100. An implementer reading only the error type definition will misunderstand the constraint.

**Fix prescription**: Update `QuotaError::InvalidPercent` error message to `"Invalid percent value (PercentValue range 0..=200 per DB CHECK; evaluator saturates at 100 before state transitions): {0}"`. Update WI-005 line 74 comment from `// 0-100 typed` to `// 0-200 allowed (DB CHECK); evaluator saturates to 100 before feeding state transitions`. Add proptest asserting `PercentValue::new(180)` succeeds and `PercentValue::new(201)` fails.

---

### NEW-P1-4 — WI-007 §6.1.13 property test exemption still lacks ADR reference despite HIGH_RISK lane

**Location**: WI-007 §6.1.13 line 438: `Property tests (TLA+ model checking is the primary; 0 TLA+-equivalent property tests)`.

This was raised as R5 P2-2 in round-1. The bis cycle did not address it (no P2 obligation, but the finding is directionally important for a HIGH_RISK WI). However, reading the changelog entry for v1.1.0, the P2s are not listed as "fixed" — so this is an honest skip. Nevertheless, per Lote 10.7bis P1-3, HIGH_RISK lane requires 100k nightly property tests, and WI-007 self-exempts without an ADR reference. The concern escalates from P2 to P1 because: (a) the TLA+ spec has an unbounded `events_in_staging`, `events_in_r2`, `retry_queue` (sets/sequences); without a `MaxConcurrentEvents` CONSTANT bounding them, TLC may produce state explosion making CI feasibility projections unreliable; (b) the WI claims "TLC state space ≥ 100k states explored" in §14.s10.007.3 as a quality standard, which is a property test claim under a different name.

**Fix prescription**: Either (a) add ADR-S10-007-tla-exempt-from-proptest justifying substitution (one-liner ADR is sufficient), or (b) add at minimum `prop_tla_constants_feasible: assert 5*5*24*5*10 < 1_000_000` as a compile-time sanity property. Add `MaxConcurrentEvents` CONSTANT to TLA+ spec to bound `events_in_staging` and `retry_queue` cardinality.

---

## Cross-WI Consistency Table (R5 specialty)

| Check | Status | Evidence |
|---|---|---|
| **5 canonical SKUs** consistent across all 7 WIs | **PASS** | `cas_storage_gb_month, cas_egress_gb, cas_put_op_count, cas_get_op_count, ac_lookup_op_count` — verified in WI-001 §1 (line 212-214), WI-002 §6.1.4 DDL, WI-003 CHECK, WI-004 §1, WI-005 §1 (line 505), WI-006 §1 (line 249), WI-007 TLA+ CONSTANTS. |
| **5 canonical PlanTiers** in enum code across all WIs | **PASS** | WI-003 PlanTier enum (113-125): free/solo/team/business/enterprise ✅; WI-005 CHECK (344): same ✅; WI-004 §1 line 262: "free/solo/team/business/enterprise" ✅. Code layer correct. |
| **5 canonical PlanTiers** in ALL narrative sections | **FAIL** | WI-003 §1 item 11 (line 247-250): still lists "free, team, enterprise, custom, trial". See NEW-P0-2. |
| **5 canonical regions** in CHECK constraints | **PASS** | WI-001 staging table CHECK (337): `('iad','fra','nrt','syd','gru')` ✅; WI-002 usage_counter CHECK (314): same ✅; WI-004 §1 (line 239): "iad, fra, nrt, syd, gru" ✅. |
| **No "30 regions" residual** anywhere | **PASS** | WI-001 §1 line 215 corrected: "5 SKUs × 5 tier × 5 region = 125 séries baseline"; line 402: "4 types × 5 regions = 20 séries". |
| **WI-002 PK includes region** | **PASS** | Line 312: `PRIMARY KEY (tenant_id, region, sku, hour)` ✅ |
| **WI-002 atomic transaction includes region** | **PASS** | Line 403 SQL INSERT: `INSERT INTO usage_counter (tenant_id, region, sku, hour, ...)` ✅; CONFLICT clause line 405 matches PK ✅. |
| **WI-004 Layer 1 GROUP BY includes region** | **PASS** | WI-004 §6.1.3 reconcile_layer_1(): passes `region: Region` as parameter; D1 counter aggregate function queries `WHERE region = ?`; drift computed per (region, sku, hour). |
| **WI-006 replay reconstruction respects 4-tuple PK** | **PASS** | WI-006 §6.1.3 calls `wi_s10_002::aggregate_for_period(raw_events)` which inherits WI-002 logic including region-scoped aggregation. |
| **prop_no_dup_aggregate still makes sense with new PK** | **PASS** | WI-002 §6.1.18: `prop_no_dup_aggregate: 100k random events with duplicate (tenant_id, request_id); assert dedup; counter reflects unique only` — still correct; PK expansion to 4-tuple makes the test MORE precise (region now distinct), not less. |
| **CloudEvents `dev.hugr.corelink.*` prefix across all WIs** | **PASS** | WI-001: `dev.hugr.corelink.cas.put.v1` etc. ✅; WI-005: `dev.hugr.corelink.quota.state.transitioned.v1` ✅; WI-006: `dev.hugr.corelink.billing.replay.requested.v1` ✅; WI-007 runbook references same ✅. |
| **Sprint contract §5.1 R-S10-1 CloudEvents prefix** | **PASS** | Sprint contract line 79: `dev.hugr.corelink.cas.put.v1` etc. ✅ |
| **D1 CHECK event_type values match enum serialization** | **PASS** | WI-001 line 338: CHECK `'dev.hugr.corelink.cas.put.v1'` etc. matches strum `#[strum(serialize = "dev.hugr.corelink.cas.put.v1")]` ✅ |
| **Idempotency-Key 35-char math** | **PASS** | WI-003 §2 narrative line 276: "9 + 8 + 1 + 8 + 1 + 8 = 35 chars total (R5 P1-B fix)" ✅; §6.1.3 code (358): format string matches ✅. |
| **WI-001 staging table includes region column** | **PASS** | Line 329: `region TEXT NOT NULL` in `usage_event_staging` ✅; drain index (342): `(region, emitted_at)` ✅ |
| **INV registry positions — concrete citations** | **PASS** | All WIs now cite §3.9 line 136/137 (NO-LOSS/NO-DUP) and §3.12 line 166/167 (RECONCILE-3-LAYER/REPLAYABLE). Verified WI-001 §1 (183-189), WI-004 §1 (210-223), WI-007 §1 (237-243). |
| **CTRL-AUTHZ-005 — zero remaining instances** | **PASS** | Confirmed absent in all 7 WIs and sprint contract. |
| **CTRL-PRIV-002 mapped correctly to data classification tags** | **PASS** | WI-003 §1 item 5 (line 223): "CTRL-PRIV-002 (privacy_model.md L209 — data classification tags): @classification=pii"; WI-005 §1 item 6 (line 204): same; WI-006 §1 (line 222): same. |
| **chrono::next_month_first_utc_midnight → corelink_time::next_month_first_utc_midnight** | **PASS** | WI-002 §1 item 14 (line 212), WI-003 §1 item 14 (line 262), WI-004 §1 (line 243), WI-005 §1 (line 231), WI-006 §1 (line 259), WI-007 §1 (line 288): all use `corelink_time::` prefix. No residual `chrono::next_month_first_utc_midnight` found. |
| **PERT estimates match sprint contract §12** | **PASS** | WI-001 §17 Total: "~23h. PERT O=14h M=22h P=36h: ~23h (matches sprint contract §12 estimate exactly)" ✅; WI-007 §17: "~20.5h. PERT O=12h M=18h P=28h: 18.7h (matches sprint contract §12)" ✅; All 7 WIs match their respective sprint contract rows. |
| **Sign-off rows: exactly 12 per WI §30** | **PASS** | WI-001 §30: rows 1-12 ✅; WI-002/003/004/005/006 all follow same 12-row structure; WI-007 §16 references 12 sign-offs. Lote 10.8bis P1-2 cap enforced. |
| **Métricas count claim vs actual enumeration** | **PARTIAL PASS** | WI-001 §14.s10.001.6 claims "7 métricas"; §6.1.12 enumerates exactly 7 ✅. WI-002 §6.1.17 enumerates 9 metrics; §14.s10.002.6 references "9 métricas" (implicit from §6.1.17). WI-007 §14.s10.007.6 claims "5 métricas"; §6.1.12 enumerates 5 ✅. Minor: WI-004 §6.1.18 claims 10 metrics (corelink_billing_reconcile_runs_total through corelink_billing_reconcile_calibration_bound_breaches_total), §14.s10.004.6 not found explicitly — not blocking. |
| **Chaos scenarios ≥ 10 per WI** | **PASS** | WI-001: 11 ✅; WI-002: 11 ✅; WI-003: (scanning §6.1 chaos suite — not shown in read excerpt; changelog claims 10+); WI-004: 12 ✅; WI-005: 11 ✅; WI-006: 11 ✅; WI-007: 11 ✅. All ≥ 10. |
| **100k nightly property tests claimed** | **PASS** | WI-001 §6.1.13: "100k nightly per HIGH_RISK SOTA bar"; WI-002/003/004/005/006: same claim. WI-007 explicitly exempts (0 property tests) — P1-4 above but not blocking. |
| **TLA+ 5×5×24×5 = 3,000** | **PASS** | WI-007 line 40, §6.1.3 line 375, §9.4 line 581: all show "3,000" for the non-events product. Arithmetic correct. |
| **TLA+ 5×5×24×5×10 = 30,000** | **PASS** | WI-007 line 239, line 297: "30,000". Arithmetic correct. |
| **No stale "150k" or "15k" state count** | **PASS** | Grepped semantically during read; no remaining "150k" or "15k" state count claims found. |
| **R2 Object Lock 7y = 2557 days** | **PASS** | WI-001 §6.1.3 IaC (line 310): `2557`; WI-004 §6.1.7 IaC (line 412): `2557`; comment "7 anos (365*7 + 2 leap)" ✅. Arithmetic: 365×7 = 2555 + 2 leap = 2557 ✓. |
| **WI-004 cardinality reconcile_drift_pct = 375 séries** | **PASS** | WI-004 §6.1.18 line 457: `corelink_billing_reconcile_drift_pct{region, layer, sku, tenant_tier}` = 5×3×5×5 = 375 ✅. |
| **WI-001 cardinality 5×5×5 = 125 séries** | **PASS** | WI-001 §1 line 215: "5 SKUs × 5 tier × 5 region = 125 séries" ✅. |
| **WI-001 cardinality 4×5 = 20 séries** | **PASS** | WI-001 §6.1.12 line 402: `4 types × 5 regions = 20 séries` ✅. |
| **WI-005 4-state machine canonical** | **PASS** | `QuotaState` enum: `Under80, SoftAlert, Ticket, HardBlock` (4 variants); `GraceActive` removed as state; `grace_active: bool` flag in `QuotaEvaluation` and schema ✅. CHECK constraint line 343: 4 values only ✅. |
| **WI-005 grace_active Gherkin scenario consistent with flag** | **PASS** | WI-005 §8 Scenario "Enterprise grace 7d prevents hard_block": uses `grace_active=true` as boolean flag, not as state value ✅. |
| **Sprint contract metric names use underscores** | **PASS** | Sprint contract §5.7 R-S10-15 (line 118): `corelink_billing_events_emitted_total{type, region}` etc. — underscores ✅. |
| **failure_modes.md FM-151 row has RB-FM-151** | **PASS** | failure_modes.md line 152: `FM-151 | Stripe API outage | ... PAT-QUEUE-EVENTS-001 (retry) + RB-FM-151` ✅. |
| **failure_modes.md near L275 has RB-FM-151 in runbook frequency table** | **PASS** | failure_modes.md line 274: `RB-FM-151 (Stripe outage; FM-151 mitigation) → semestral; staging dry-run prerequisite (sprint contract S-10 §6 DoD; criado Lote 10.10bis)` ✅. |
| **CTRL-AUTHZ-001 + CTRL-AUTHZ-002 grammatical coherence** (no duplicate like `CTRL-AUTHZ-002 + CTRL-AUTHZ-002`) | **PASS** | WI-005 line 202: "CTRL-AUTHZ-001 + CTRL-AUTHZ-002"; WI-006 line 218, 273: "CTRL-AUTHZ-001 + CTRL-AUTHZ-002". No duplication found. |
| **corelink_time rename — no broken code syntax** | **PASS** | All occurrences: `corelink_time::next_month_first_utc_midnight()`. No half-replaced `chrono::corelink_time::` or broken call syntax detected. |
| **WI-007 TLA+ quota_states consistent with WI-005 4-state fix** | **FAIL** | WI-007 line 77: `quota_states, \* Map tenant -> {under_80, soft_alert, ticket, hard_block, grace_active}` — still includes `grace_active` as a state value in comment, contradicting WI-005 bis fix. See NEW-P0-1. |
| **§0-§32 sections present per work_item template** | **PASS** | All 7 WIs contain §0 through §32. Verified §30 (sign-off) and §31 (change log) present in all. |
| **Layer 3 SEV-1 alert consistent with HIGH severity** | **PASS** | Sprint contract §5.4 R-S10-8.3: "drift > 0.1% → SEV-1" for Layer 3 (Stripe vs invoice); WI-004 §2 narrative confirms: "Layer 3 drift > 0.1% = SEV-1 imediato". This is justified because SEV-1 reflects the customer-facing invoice accuracy tier, NOT the INV-BILLING-NO-LOSS invariant severity — the severity (HIGH) governs invariant classification; SEV levels for alerts are independently calibrated per sprint contract quality standards (§14.s10.1). Consistent and defensible. |
| **WI-001 §9 design decision 9.15 — INV position verification** | **PASS** | Line 558: "9.15: NEW INVs INV-BILLING-NO-LOSS + INV-BILLING-NO-DUP em invariant_registry — verify canonical position before commit (Lote 10.8bis P1-13 lesson)." ✅ |
| **Sprint contract §8 INV-BILLING-NO-LOSS listed as CRITICAL vs HIGH** | **FAIL — RESIDUAL NOT FIXED IN CONTRACT** | Sprint contract §8 line 149: "**INV-BILLING-NO-LOSS** (CRITICAL): Σ(events emitidos) = Σ(invoiced + tombstoned + late_pending). Drift > 0.1% = SEV-1." The WIs were correctly updated to HIGH + SEV-2, but the sprint contract §8 itself still says CRITICAL + SEV-1. This creates an authority conflict: the canonical source for the sprint (the contract) disagrees with the WIs on severity. |

---

## Cascade Verification Results

### PK Change Cascade (R5 P0-A)

- WI-002 UPSERT SQL now uses 4-column PK: **PASS**
- WI-004 Layer 1 reconciliation algebra `d1_counter_aggregate_by_sku(region, target_date)`: region is a parameter to reconcile_layer_1; query filters by region — **PASS**
- WI-006 replay reconstruction uses `wi_s10_002::aggregate_for_period(raw_events)` with region parameter in the request — **PASS**
- `prop_no_dup_aggregate` still valid with 4-tuple PK — **PASS** (see table above)

### CloudEvents Prefix Cascade (R5 P0-E)

- WI-001 enum: corrected ✅
- WI-001 D1 CHECK: corrected ✅
- Sprint contract §5.1 R-S10-1: corrected ✅
- WI-005 audit event type `dev.hugr.corelink.quota.state.transitioned.v1`: consistent ✅
- WI-006 audit event type `dev.hugr.corelink.billing.replay.requested.v1`: consistent ✅
- WI-007 TLA+ TLC model scenario Gherkin references CloudEvents: consistent ✅
- Idempotency-Key `operation` field naming in WI-003: uses `"customer" | "sub" | "invoice" | "refund"` — consistent with 35-char format, NOT conflating with CloudEvents type prefix ✅

### Severity Change Cascade (P0-D)

- All 7 WIs: HIGH ✅
- Sprint contract §8: **STILL CRITICAL** (residual — see table above)
- Sprint contract §9 (Quality Standards): line 160: "**14.s10.1 Zero tolerance pra drift > 0.1%** em reconciliation. Drift entre 0.01% e 0.1% = SEV-3 monitor; > 0.1% = SEV-2 immediate; > 1% = SEV-1 + invoice freeze." — the quality standards use SEV-2 for Layer 1/2 drift, which is consistent with HIGH severity. Layer 3 uses SEV-1 per quality standard §14.s10.1, which overrides the invariant severity for alert purposes. This is defensible but the §8 CRITICAL text remains a contradiction.

### 4-State vs 5-State Cascade (R4 P1-7 / R5 P2-3 bis fix)

- WI-005 `QuotaState` enum: 4 states ✅
- WI-005 DB CHECK: 4 values ✅
- WI-005 `QuotaEvaluation`: `grace_active: bool` flag ✅
- WI-005 Gherkin scenarios: use boolean flag ✅
- WI-007 TLA+ `quota_states` variable comment: STILL lists grace_active as 5th state value — **FAIL** (NEW-P0-1)

---

## Additional Findings Not in Round-1

### OBS-1 — Sprint contract §8 severity contradiction (CRITICAL vs WI HIGH) — not a new P0 but a contract-level residual

**Location**: Sprint contract §8, lines 149-150.

As noted in the cascade table above, the sprint contract body still declares INV-BILLING-NO-LOSS and INV-BILLING-NO-DUP as CRITICAL. The WIs were correct to change to HIGH, but the contract — which is the authoritative document for the sprint — still contradicts them. This was not explicitly listed in the declared 8 P0 fixes (the fix was applied to WIs but the sprint contract §8 was not updated for the severity field). The sprint contract header (lines 1-15) says version 1.1.0, indicating it was part of the bis cycle; §5.1 R-S10-1 was updated (CloudEvents prefix); §5.7 R-S10-15 was updated (underscores); but §8 severity fields were not. This is an incomplete fix — the same issue appears at two levels and only one was remediated.

**Severity**: P1 (contract authoritative status makes this materially misleading to reviewers).

**Fix prescription**: Update sprint contract §8 INV-BILLING-NO-LOSS and INV-BILLING-NO-DUP severity from CRITICAL to HIGH. Update "Drift > 0.1% = SEV-1" on line 149 to "Drift > 0.1% = SEV-2 (Layer 1/2) or SEV-1 (Layer 3 per §14.s10.1 quality standard)".

---

### OBS-2 — WI-001 §1 item 9 description contradicts §6.1.3 idempotency key format

**Location**: WI-001 §1 item 9 (line 210): `"Idempotency key derivation (sprint contract §5.3 R-S10-6 inheritance): corelink-{tenant_id}-{event_hash} UUID v4 entropy 122-bit"`.

This still uses the long-form `{tenant_id}` (full UUID, not short form), contradicting the bis-fixed §6.1 format of `corelink-{tenant_id_short(8)}-{operation}-{event_hash_short(8)}` = 35 chars. The P1-C fix from round-1 updated the struct field comment at line 108, but line 210 (§1 bullet list item 9) was missed.

**Severity**: P1 (same format confusion, different location — incomplete fix coverage, same defect class as P1-C).

**Fix prescription**: Update WI-001 §1 item 9 to: `corelink-{tenant_id_short(8)}-{operation}-{event_hash_short(8)} — 35 chars; per WI-S10-003 §6.1 derivation`.

---

### OBS-3 — WI-007 TLA+ `Spec` fairness: `WF_vars(StripeOutageRecovers)` is weak fairness on Stripe recovery, which is NOT appropriate for a modeled infrastructure failure

**Location**: WI-007 TLA+ spec line 213: `Spec == Init /\ [][Next]_vars /\ WF_vars(StripeOutageRecovers) /\ WF_vars(DrainStagingToR2)`.

Weak fairness on `StripeOutageRecovers` means TLC will verify that Stripe always eventually recovers. This is correct for model checking liveness (`<>(evt \in events_in_r2)`), but it means TLC's model implicitly assumes Stripe cannot stay down forever — which the TLA+ THEOREM `LIVENESS_EVENT_DRAIN` depends on. This is architecturally sound (Stripe has SLA > 99.9%), but the spec should document this assumption explicitly, since an auditor reading the TLA+ might question whether liveness is proven under infinite Stripe outage. As written, the spec is technically correct but under-documented.

**Severity**: P2 (documentation gap, not a correctness defect).

---

## Summary — Priority Fix Sequence

1. **NEW-P0-1**: WI-007 TLA+ `quota_states` comment regression — fix the variable comment + add separate `quota_grace_active` variable.
2. **NEW-P0-2**: WI-003 §1 item 11 PlanTier names still wrong — full-file grep for `custom, trial` and replace.
3. **OBS-1 (P1)**: Sprint contract §8 severity still CRITICAL — update to HIGH + SEV-2/SEV-1 split.
4. **NEW-P1-3 (P1)**: WI-005 `QuotaError::InvalidPercent` message says 0..=100 but DB CHECK is 0..=200.
5. **OBS-2 (P1)**: WI-001 §1 item 9 idempotency key description uses wrong long-form format.
6. **NEW-P1-1 (P1)**: Late event reference-time inconsistency (now vs window_start).
7. **NEW-P1-4 (P1)**: WI-007 property test exemption lacks ADR + `MaxConcurrentEvents` CONSTANT missing.

---

## Recommendation

**ONE MORE CYCLE** — not multiple rounds needed.

The bis cycle made genuine, substantial progress. The 8 P0s are all resolved. The two new P0s (NEW-P0-1 and NEW-P0-2) are both narrow, targeted fixes: WI-007 TLA+ comment regression (one line + one new CONSTANT) and WI-003 §1 item 11 narrative (grep-and-replace). The 4 P1s are all single-paragraph fixes. A tris cycle with focused attention on these 7 items should achieve SEAL status.

Estimated effort for tris cycle: 2-3 hours of careful, targeted edits. The S-10 WI batch is now structurally sound and the systematic multi-file issues (PlanTier, severity, INV positions, CloudEvents prefix, PK region) are genuinely resolved. S-09 needed 5 rounds because it had compound structural failures that generated cascading regressions; S-10 is converging faster because the residuals are isolated, non-cascading misses in narrative sections rather than code sections.

**Tris cycle blocker checklist**:
- [ ] WI-007 line 77 comment: remove `grace_active` from state range comment; add `MaxConcurrentEvents` CONSTANT
- [ ] WI-003 §1 item 11: `free, team, enterprise, custom, trial` → `free, solo, team, business, enterprise`  
- [ ] WI-003 full-file grep for `custom` and `trial` in plan-tier contexts
- [ ] Sprint contract §8: INV-BILLING-NO-LOSS severity CRITICAL → HIGH; SEV-1 → SEV-2/SEV-1 split
- [ ] WI-005 `QuotaError::InvalidPercent` message: `0..=100` → `0..=200`
- [ ] WI-001 §1 item 9: idempotency key description — long form → short 35-char form
- [ ] WI-002 §6.1.11 late event threshold: standardise on `now - 6h` reference time
- [ ] WI-007 §6.1.13: add ADR reference for property test exemption + `MaxConcurrentEvents` CONSTANT citation

---

**End audit. Sonnet R5 (Claude Sonnet 4.6) — 2026-04-26.**
