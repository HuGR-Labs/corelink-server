---
type: audit
title: Agent R4 (Opus 4.7) round-2 validation of S-10 WIs (Lote 10.10bis post-remediation)
date: 2026-04-26
reviewer: Agent R4 (Claude Opus 4.7)
sprint: S-10
target: 7 WIs + sprint contract + failure_modes
round: 2
---

# Agent R4 — S-10 round-2 validation post-Lote-10.10bis

## Aggregate score: 6.9/10 (vs round-1 6.7/10)

**Modest improvement; bis cycle absorbed ~60% of declared fixes faithfully but failed the sed-coverage discipline.** The 8 P0s claimed fixed are actually mostly fixed *in the canonical primary location* (the SQL CHECK, the enum, the §1 invariant block) but the cascade into narratives, Gherkin scenarios, design-decision lists, risk registers, post-mortem hooks, struct fields, and the spec contract §8 invariants block was **systematically missed** — exactly the S-08/S-09 anti-pattern the user flagged.

**Three bis-introduced or surviving P0s** prevent SEAL:
1. **Spec contract §8 invariants block (lines 149-151) STILL CRITICAL/SEV-1** — the bis fix updated WIs but missed the source-of-truth document. Sprint contract is the authority; if it says CRITICAL, WIs are wrong, not vice versa.
2. **WI-002 PRIMARY KEY 4-tuple sweep ~30% complete** — DDL fixed (line 312) but `CounterRecord` struct (line 94-104) has NO `region` field; trait doc, narrative, Gherkin, design decisions, INV §12, título, multiple §6.1 subsections still cite 3-tuple `(tenant_id, sku, hour)`. Hash chain digest computed without region in canonical_json → multi-region tampering false-negative.
3. **WI-007 TLA+ `MaxConcurrentEvents` CONSTANT claimed NEW but NOT in actual TLA+ block** — narrative §2 (L297) and §9.4 (L581) reference it as if added, but the CONSTANTS block (L58-65) only declares `MaxEventsPerHour, MaxRetries`. Pure narrative/code drift.

Bis cycle quality is comparable to S-09 round-2: improved over round-1 but still requires a tris (10.10-tris) cycle to clean up the cascade misses. Approximately 1-2 hours of careful sed work would close the residuals.

---

## P0 status table

| # | P0 (round-1 / round-1.5) | Status | Evidence |
|---|---|---|---|
| **P0-1** | PlanTier 5-tuple canonical | ⚠️ Partial | WI-S10-003 §6.1.11 line 248 still lists `free, team, enterprise, custom, trial`; design decision 9.13 line 548 same. CHECK at line 438 fixed; PlanTier enum at line 113-125 fixed. **Sed missed §6.1.11 + §9.13 narrative.** |
| **P0-2** | INV §3.X positions + severity | ⚠️ Partial | §1 invariant blocks + §12 INV blocks correctly say HIGH + §3.9 line 136/137 / §3.12 line 166/167. **BUT**: spec contract §8 (lines 149-151) still says CRITICAL + SEV-1 (source-of-truth document NOT updated); WI-001 risk register R-001 says CRITICAL+SEV-1 (line 714); §28 post-mortem hooks line 674-675 say CRITICAL. Cascade incomplete. |
| **P0-3** | TLA+ math 30k/3k + caveat | ⚠️ Partial | Numbers fixed (3,000 / 30,000 / caveat about reachable graph). **`MaxConcurrentEvents` CONSTANT claimed NEW but NOT in TLA+ CONSTANTS block at line 58-65** (only `MaxEventsPerHour` exists). Narrative drift. Hours = {0..23} caveat from R4 round-1 NOT addressed (still represents hour-of-day, billing periods span months). |
| **P0-4** | CTRL-AUTHZ-005 hallucinated | ✅ Fixed | Zero residuals; replaced with `CTRL-AUTHZ-001 + CTRL-AUTHZ-002` consistently across WI-005/006 + sprint contract §5.6 R-S10-13. Replacement is grammatical. |
| **P0-5** | WI-002 UPSERT WHERE | ⚠️ Partial | SQL fixed (line 405-411 ON CONFLICT no longer has self-defeating WHERE); `CounterDigestMismatch` error path mentioned (line 136 + line 426). **BUT Gherkin scenario line 528 still says `Then UPSERT WHERE own_digest = D1 succeeds`** — old wording survived in test scenarios. |
| **P0-6** | Cloudflare R2 Terraform | ✅ Fixed | WI-001 line 298-307 + WI-004 line 401-415: nested `object_lock_configuration` block inside `cloudflare_r2_bucket`. Comment says "Validar resource type names contra cloudflare/cloudflare provider pinned em infra/cloudflare/versions.tf" — pre-merge verification flagged. |
| **R5 P0-A** | WI-002 PK 4-tuple | ❌ Regressed (incomplete) | DDL line 312 has 4-tuple PK ✅; usage_counter_late line 344 has region ✅. **BUT**: (a) `CounterRecord` struct lines 94-104 has NO `region` field — Rust→SQL bind impossible; (b) `LateCounterRecord` lines 107-116 same; (c) trait doc line 69 still says `PRIMARY KEY (tenant_id, sku, hour) UNIQUE`; (d) título line 28 + 40, narrative line 47, design decision 9.6 line 610, INV §12 line 647, hot-path discipline line 168/236, Gherkin line 526 ALL still 3-tuple. (e) Hash chain digest computed from canonical_json(record sans own_digest) — without region in struct, cross-region records produce identical digests → tamper detection silently degraded. Estimated 70% of references still wrong. |
| **R5 P0-B + P0-E** | UsageType strum + D1 CHECK + CloudEvents prefix | ✅ Fixed | UsageType enum (L83-93) serializes to `dev.hugr.corelink.cas.put.v1` etc.; D1 CHECK (L338) matches long form; sprint contract §5.1 R-S10-1 (L77-80) updated. |

---

## NEW P0 findings (introduced or surfaced by bis cycle)

### NEW-P0-1 — Spec contract §8 invariants block NOT updated; CRITICAL/SEV-1 still source-of-truth

**Location**: `_spec_contract.md` lines 149-151:
```
- **INV-BILLING-NO-LOSS** (CRITICAL): Σ(events emitidos) = Σ(invoiced + tombstoned + late_pending). Drift > 0.1% = SEV-1. Reference: invariant_registry.md.
- **INV-BILLING-NO-DUP** (CRITICAL): nenhum charge duplicado por mesma fonte; enforced via Idempotency-Key + (tenant_id, request_id) UNIQUE.
- **INV-AUDIT-APPEND-ONLY** (CRITICAL): usage events em R2 são append-only via Object Lock; tampering detected via hash chain.
```

**Evidence**: The bis changelog row in every WI claims "(2) ... severity drift CRITICAL→HIGH cascateia em SEV-2 alert wording (vs SEV-1); Lote 10.8bis P1-13 lesson finalmente absorvida". WIs §1 invariant blocks now consistently say `HIGH; registry §3.9 line 136`. **But the sprint contract — which is the parent document the WIs inherit from — still asserts CRITICAL with SEV-1.** When a reader sees WI says HIGH and contract says CRITICAL, contract wins. This is the worst kind of cascade miss because it inverts the truth gradient.

Sub-issues:
- §14.s10.1 (line 160) is CONSISTENT with HIGH+SEV-2 ("Drift entre 0.01% e 0.1% = SEV-3 monitor; > 0.1% = SEV-2 immediate; > 1% = SEV-1 + invoice freeze") — the >1% SEV-1 is for the Layer 3 carve-out, fine.
- §5.4 R-S10-8.1/8.2/8.3 (lines 100-102) Layer 1/2 = SEV-2, Layer 3 = SEV-1 — fine.
- But §8 invariants block contradicts both the WIs and §14.s10.1's gradient.
- §15 risk register line 244 still contains "Pseudonymization (não delete) → mantém chain integrity; CTRL-PRIV-002 alignment" — CTRL-PRIV-002 mismapping NOT corrected in spec contract (only in WI §1 blocks).

**Fix prescription**: Update spec contract §8:
```
- **INV-BILLING-NO-LOSS** (HIGH; registry §3.9 line 136): Σ(events emitidos) = Σ(invoiced + tombstoned + late_pending). Drift > 0.1% = SEV-2 (HIGH severity per registry §2 canonical mapping); > 1% = SEV-1 + invoice freeze (escalation gate). Reference: invariant_registry.md §3.9.
- **INV-BILLING-NO-DUP** (HIGH; registry §3.9 line 137): ...
- **INV-AUDIT-APPEND-ONLY** (CRITICAL; registry §3.6 line 116): ... [this one IS CRITICAL canonical, keep]
```

Also update §15 risk register line 244 to remove pseudonymization conflation: "Pseudonymization (não delete) → mantém chain integrity; CTRL-PRIV-002 (data classification tags) + S-11 DSR pseudonymization procedure separate".

**Severity**: P0 because spec contract is the authoritative document and WIs explicitly inherit from it; current state means the WI claims contradict their own parent.

---

### NEW-P0-2 — WI-S10-002 PRIMARY KEY 4-tuple cascade ~30% complete; Rust types out of sync with SQL

**Location**: `WI-S10-002-counter-aggregator-cron-do-hash-chain.md`

**Evidence**: The DDL was fixed correctly:
- Line 312: `PRIMARY KEY (tenant_id, region, sku, hour)`
- Line 314: `CHECK (region IN ('iad', 'fra', 'nrt', 'syd', 'gru'))`
- Line 344: usage_counter_late PK includes region

But the rest of the WI was NOT swept:
- **Line 28 título**: "aggregates por `(tenant_id, sku, hour)` → D1 `usage_counter`"
- **Line 40 título row**: "PRIMARY KEY (tenant_id, sku, hour) UNIQUE"
- **Line 47 §2 narrative**: "aggregates por `(tenant_id, sku, hour)`"
- **Line 69 trait doc comment**: "counter rows UPSERT-safe via PRIMARY KEY (tenant_id, sku, hour) UNIQUE"
- **Line 94-104 `CounterRecord` struct**: NO `region` field declared (only tenant_id, sku, hour, qty_bytes, qty_ops, ...)
- **Line 107-116 `LateCounterRecord` struct**: NO `region` field declared
- **Line 168 §1 hot-path discipline**: "PRIMARY KEY (tenant_id, sku, hour) UNIQUE em usage_counter"
- **Line 236 §2 narrative**: "PRIMARY KEY (tenant_id, sku, hour) UNIQUE = UPSERT idempotent"
- **Line 294 §6.1.3 step 4**: "aggregate by (tenant_id, sku, hour) → SUM(bytes), SUM(ops), COUNT(events)"
- **Line 526 Gherkin**: "Then aggregator detects PRIMARY KEY (tenant_id, sku, hour) conflict"
- **Line 610 §9 design decision 9.6**: "PRIMARY KEY (tenant_id, sku, hour) UNIQUE → UPSERT idempotent on cron retry"
- **Line 647 §12 INV**: "INV-BILLING-NO-DUP ... PRIMARY KEY (tenant_id, sku, hour) UNIQUE"

**Compounding bug — hash chain breaks across regions**: `compute_counter_digest()` (line 375) hashes `canonical_json(record sans own_digest field)`. If `CounterRecord` has no region field, two records with identical `(tenant_id, sku, hour, qty_bytes, qty_ops, event_count, aggregated_at, prev_hash)` from different regions produce **identical digests** despite being distinct rows. This silently breaks per-region tamper detection. The WI asserts at line 196: "counter rows scoped per (region, tenant, sku, hour)" — but the struct/digest don't enforce this.

**Compounding bug — Layer 1 reconciliation join breaks**: WI-004 line 217 says `Layer 1 enforces: Σ(R2 events) = Σ(usage_counter qty) + Σ(usage_counter_late qty) per (tenant, sku, hour)` — old 3-tuple. The WI-004 implementation aggregates per-region in `r2_aggregate_by_sku(region, target_date)` and `d1_counter_aggregate_by_sku(region, target_date)` — so each call is region-scoped, fine — but Layer 2 (line 386) and Layer 3 (line 392) still say `per (tenant, sku)` without region. If the actual D1 query joins on `(tenant, sku)` ignoring region, multi-region totals collide.

**Fix prescription**:
1. Add `region: Region` to `CounterRecord` (line 94) AND `LateCounterRecord` (line 107).
2. Update `compute_counter_digest()` doc comment to confirm region is included in canonical_json.
3. Sed all 10+ "(tenant_id, sku, hour)" 3-tuple references in narrative/Gherkin/design decisions to "(tenant_id, region, sku, hour)" 4-tuple.
4. Update WI-004 Layer 2/3 narrative to clarify per-region or per-billing-period scoping.
5. Add Gherkin scenario `prop_multi_region_counters_independent` per Sonnet R5 P0-A prescription.

**Severity**: P0 — cross-region tamper detection silently degraded; Layer 2/3 reconciliation arithmetically wrong if cross-region totals collapse.

---

### NEW-P0-3 — WI-007 TLA+ `MaxConcurrentEvents` CONSTANT claimed NEW but NOT in actual TLA+ code

**Location**: `WI-S10-007-tla-billing-atomicity-runbooks-finance-walkthrough.md`

**Evidence**: The bis changelog row claims `MaxConcurrentEvents bound NEW`. Narrative §2 line 297 says: "events_in_staging/r2/retry_queue são unbounded sets sem `MaxConcurrentEvents` CONSTANT (NEW)". §9.4 line 581 says: "TLC reachable state graph TBD — runs em CI with bounded MaxConcurrentEvents".

But the actual TLA+ code block at line 54-65 declares only:
```
CONSTANTS
    Tenants,
    SKUs,
    Regions,
    Hours,
    MaxEventsPerHour,           \* upper bound model checking (e.g., 10)
    MaxRetries
```

`MaxConcurrentEvents` is **not** a CONSTANT in the spec. The narrative claims it bounds `events_in_staging`, `events_in_r2`, `retry_queue` (line 297) — but no action in the TLA+ code references it. `EmitEvent` (line 108-117) does `events_in_staging' = events_in_staging \union {evt}` — unbounded set growth.

This is exactly the S-08/S-09 anti-pattern: narrative absorbs the lesson but code/spec doesn't.

**Fix prescription**:
1. Add `MaxConcurrentEvents` to CONSTANTS block (line 64).
2. Add guard `Cardinality(events_in_staging) < MaxConcurrentEvents` to `EmitEvent` action (line 108) and similar for `DrainStagingToR2`, `retry_queue` push.
3. Document concrete value (e.g., 100 for CI; 1000 for nightly) in `tlc.cfg` or model file.
4. Run TLC dry-run to confirm reachable state graph stays under 1M with the bound.

Also, the R4 round-1 caveat about `Hours = {0..23}` was NOT addressed. Billing periods span months; modeling time as hour-of-day suggests states recur after 24 hours which is wrong for monthly billing aggregation. Consider `Hours == {0..(24*30 - 1)}` or `BillingPeriods` time abstraction.

**Severity**: P0 — TLA+ formal verification claim is the diferencial vs competitors per sprint contract §16. False rigor undermines SOC 2 CC1.4 evidentiary value (Finance walkthrough mock auditor will spot this).

---

## NEW P1 findings

### NEW-P1-1 — WI-005 PercentValue type/error/DDL drift across §1 / §6.1 / §1 enum

**Location**: `WI-S10-005-quota-state-machine-overage-email.md`

**Evidence**:
- **Line 74**: `trigger_pct: PercentValue, // 0-100 typed` — old comment in DemoteFromSoftAlert input
- **Line 119**: `pub current_pct: PercentValue, // 0-200 typed; DB CHECK 0..=200 (R5 P1-H fix: align type vs DB)` — fixed
- **Line 178**: `#[error("Invalid percent value (CHECK 0..=100 violation): {0}")]` — old constraint message
- **Line 328**: `current_pct NUMERIC(5,2) NOT NULL, -- 0.00-100.00 (precision)` — DDL precision comment says 0-100 but constraint at L345 says ≤200
- **Line 345**: `CHECK (current_pct >= 0 AND current_pct <= 200)` — fixed
- **Line 700**: design decision 9.19: "PercentValue typed (CHECK 0..=200 sanity; allows over 100 reporting bounded)" — fixed

So the type/comment/DDL/error contradicts internally. A developer reading line 74 sees "0-100"; line 119 sees "0-200"; line 178 error message says "0..=100 violation"; line 328 DDL precision comment says "0.00-100.00"; line 345 constraint allows up to 200. PercentValue tuple struct (assumed `f64`) probably saturates rather than panics, but the constraint is intent-confused.

**Fix prescription**: Resolve to single canonical: type allows 0-200, DDL CHECK 0-200, NUMERIC precision needs to be `(5,2)` covering 200.00 (already does, since `(5,2)` allows up to 999.99). Update line 74 comment to "0-200 typed"; line 178 error to "Invalid percent value (CHECK 0..=200 violation)"; line 328 DDL comment to "0.00-200.00 (saturation cap)".

---

### NEW-P1-2 — WI-S10-001 INV severity cascade incomplete in §24 risk register + §28 post-mortem hooks

**Location**: `WI-S10-001-usage-event-emitter-r2-append-only-idempotency.md`

**Evidence**:
- §1 (line 183-188): correctly says HIGH; registry §3.9 line 136/137 ✅
- §12 (line 583-584): correctly says HIGH; registry §3.9 line 136/137 ✅
- §6.1 (line 185): correctly says "HIGH severity → SEV-2 not SEV-1; SEV-1 reserved for CRITICAL invariants per registry §2" ✅
- **Line 674 §28 post-mortem hooks**: "INV-BILLING-NO-LOSS violation detected → CRITICAL post-mortem (revenue leak)" ❌
- **Line 675 §28 post-mortem hooks**: "INV-BILLING-NO-DUP violation → CRITICAL (double-charge customer)" ❌
- **Line 714 §24 risk register R-001**: `INV-BILLING-NO-LOSS violation | L | M | CRITICAL | M | LOW | Staging buffer + retry queue + chaos 30d; SEV-1 alert` ❌
- **Line 715 §24 risk register R-002**: `INV-BILLING-NO-DUP violation (double-charge) | L | M | CRITICAL | M | LOW | (tenant_id, request_id) UNIQUE + property test 100k` ❌

The risk register and post-mortem sections should align with the §1 / §12 severity but were skipped by sed. CRITICAL is now unjustified by the bis cycle's own severity decision.

Same residual likely in WI-002/003/004/005/006/007 risk registers (need spot-check). Confirmed in WI-002 line 821 changelog row historical notes.

**Fix prescription**: For WI-001 and any WI with the same pattern: change §24 R-001/R-002 severity columns to HIGH and mitigation columns to "SEV-2 alert + Finance review"; change §28 post-mortem hooks to "→ HIGH-severity post-mortem (revenue leak protection; Finance + SRE)".

---

### NEW-P1-3 — Quality Standards §14.s10.x.8 + Risk Register R-010/R-011 still reference "INV §3.X position verified" / "canonical position TBD pre-merge"

**Location**: All 7 WIs.

**Evidence**: Even though §1 and §12 INV blocks now have concrete §3.9/§3.12 positions, the §14.s10.x.8 quality discipline lines (e.g., WI-001 line 611) and §24 risk register row R-010/R-011 (e.g., WI-001 line 723) still cite "INV §3.X position verified before commit" and "canonical position TBD pre-merge" as if the position were unknown.

This is mostly cosmetic — the lesson is referenced (Lote 10.8bis P1-13) — but the **mitigation column** "canonical position TBD pre-merge" is now stale because position IS known. Either drop those R-010/R-011 risk rows (now mitigated by definition) or change mitigation to "INV positions §3.9/§3.12 verified at v1.1.0 commit (Lote 10.10bis); maintenance discipline ongoing".

**Fix prescription**: Either remove R-010/R-011 (risk now retired) or update mitigation column to "verified at v1.1.0; ongoing discipline". Preferred: keep row, update mitigation, change column "Residual após mitigação" to LOW with explicit "verified" annotation.

---

### NEW-P1-4 — WI-006 still has "Finance + Legal + Architect 3-of-3 sign-off" per-WI Legal

**Location**: `WI-S10-006-replay-forensic-endpoint-role-audit-trail.md` lines 234-237 (changelog cite), 462-466, 670-672, 750, 858

**Evidence**: R4 round-1 P1-6 specifically called out that Legal at the per-WI / per-replay level contradicts the sprint-level Legal-only convention (the sign-off matrix has Legal at sprint level only — see WI-001 line 746 "Legal sign-off via DPA reference at sprint level; not per-WI"). The bis cycle did NOT address this; "Finance + Legal + Architect 3-of-3 sign-off" remains the convention for `dry_run=false`.

The conflict: WI-006 line 887 says "Legal sign-off via DPA reference at sprint level + 3-of-3 sign-off process review" — acknowledging both, but the per-replay 3-of-3 still includes Legal explicitly, contradicting "sprint level only".

**Fix prescription** (R4 round-1 P1-6 prescription, restated): Replace "Finance + Legal + Architect 3-of-3 sign-off" with "Finance + Compliance Officer + Architect 3-of-3 sign-off"; document that Legal sign-off is captured at sprint level via DPA + Stripe contract review (must be in place before any `dry_run=false` is attempted). Add CLI mechanism specification: `corelink billing-signoff --invoice X --role <Finance|Compliance|Architect>` authenticated via S-03 RBAC + CloudEvent emission to S-09 audit chain.

---

### NEW-P1-5 — WI-S10-003 título line 29 + line 41 still describe CTRL-PRIV-002 as pseudonymization

**Location**: `WI-S10-003-corelink-billing-stripe-adapter-idempotency-webhook.md`

**Evidence**:
- Line 29 título: "LINDDUN compliance via PRIVACY-MODEL CTRL-PRIV-002 pseudonymization for DSR"
- Line 41 título row: "CTRL-PRIV-002 pseudonymization for LGPD/GDPR DSR (S-11 cooperation)"
- Line 542 §9.10: "DSR pseudonymization via S-11 cooperation (CTRL-PRIV-002)"

§1 invariant block at line 223 correctly says "data classification tags". §6.1.10 line 540-542 mentions "DSR pseudonymization via S-11 cooperation (CTRL-PRIV-002)" — keeps both linked but conflates.

The WI-005 line 727 quality block also keeps "CTRL-PRIV-002 (privacy_model.md): customer email pseudonymization for DSR" — old mismapping survives.

Spec contract line 244 also still has pseudonymization parenthetical citing CTRL-PRIV-002.

**Fix prescription**: Replace all pseudonymization-as-CTRL-PRIV-002 with "CTRL-PRIV-002 (data classification tags) ensures customer billing PII tagged @classification=pii; DSR pseudonymization is a separate procedure handled via S-11 cooperation (control TBD)".

---

### NEW-P1-6 — WI-S10-004 Layer 2/3 narrative still uses 3-tuple "(tenant, sku)" join

**Location**: `WI-S10-004-reconciliation-worker-3-layer-drift-alerts.md` lines 386 (Layer 2), 392 (Layer 3)

**Evidence**:
```
4. **Layer 2 reconciliation logic** (sprint contract §5.4 R-S10-8.2):
   - Reads D1 usage_counter for billing_period (entire month).
   - Reads Neon invoice_line_item for billing_period.
   - Compares per (tenant, sku) → drift.
   - Runs monthly on 1st (after WI-S10-003 monthly cron completes).

5. **Layer 3 reconciliation logic** (sprint contract §5.4 R-S10-8.3):
   - Reads Neon invoice_line_item for billing_period.
   - Pulls Stripe invoice via WI-S10-003 fetch_invoice (cooperation).
   - Compares per (tenant, sku) → drift.
```

Layer 2 reads `usage_counter` whose PK is now 4-tuple `(tenant_id, region, sku, hour)`. If Layer 2 collapses across regions to `(tenant, sku)`, the drift signal merges multi-region totals — masking single-region drift. Layer 3 invoice generation may be per-region too (pending WI-003 spec); per-region reconciliation prevents masking.

**Fix prescription**: Layer 2 should be `per (tenant, region, sku, billing_period)` if usage_counter is per-region; or explicitly aggregate across regions BEFORE comparing if Stripe invoice is per-tenant (no region). Document the chosen aggregation semantics.

---

### NEW-P1-7 — WI-S10-005 line 178 PercentValue error is also referenced in QuotaError without code update

(See NEW-P1-1 above; this is the error variant of the same drift cluster.)

---

## Residual P1 findings from round-1 not addressed

### P1-3 (R4 round-1) — Stripe Idempotency-Key 64-char limit not justified vs Stripe 255

**Location**: WI-S10-003 line 178-180 + line 366

**Evidence**: Bis didn't add ADR justifying the 64 vs 255 threshold. R5 P1-B narrative was rewritten (good — line 275 now correctly describes 35-char format), but the assertion length 64 vs Stripe's 255 still has no documented rationale. Future operation names > 8 chars (e.g., "subscription_cancel" = 19) would push format past 64 and panic legitimately.

**Status**: NOT addressed.

---

### P1-6 (R4 round-1) — WI-S10-006 sign-off mechanism unspecified

(See NEW-P1-4 above — same finding from round-1 surviving the bis cycle.)

**Status**: NOT addressed.

---

### P1-9 (R4 round-1) — `chrono::next_month_first_utc_midnight()` source path

**Status**: ⚠️ Partially addressed. The function is now `corelink_time::next_month_first_utc_midnight()` (no longer claimed as `chrono::`). The crate `corelink_time` is referenced in 25+ places, but the **canonical helper crate path** (e.g., `crates/corelink-time/src/canonical.rs`) is still not cited in any WI. R5 mentioned this in P2-5 — still missing.

**Fix prescription**: Add to WI-S10-002 §13 Artifacts Produced (or WI-S10-001 §13): "NEW: `crates/corelink-time/src/canonical.rs` — `next_month_first_utc_midnight()` helper; signature `fn next_month_first_utc_midnight() -> DateTime<Utc>`; UTC-anchored; chained tests `prop_idempotent_across_dst`, `prop_handles_leap_year`."

---

### P1-10 (R4 round-1) — RB-BILLING-002 (late events triage) reference

**Status**: ✅ Addressed (referenced in spec contract §15 line 242 and WI-S10-002 risk register; not duplicated).

---

### P2-2 (R5 round-1) — WI-007 self-exempts from 100k property tests for HIGH_RISK without ADR

**Location**: WI-S10-007 §6.1.13 line 438: "Property tests (TLA+ model checking is the primary; 0 TLA+-equivalent property tests)"

**Status**: NOT addressed. No ADR cited justifying TLA+ as substitute for 100k nightly property tests on HIGH_RISK lane.

**Fix prescription**: Add ADR-S10-007-TLA-substitutes-proptest with justification + at minimum 1 lightweight property test (e.g., `prop_state_space_under_1M_states` validating CI feasibility threshold). Or explicitly downgrade WI-007 from HIGH_RISK to MEDIUM_RISK with PRR cap adjustment.

---

### P2-9 (R4 round-1) — "Mfa" capitalization

**Location**: WI-S10-006 multiple places (line 64, 174, 218, 273, 311, 463, 743, 836, 845, 858, 870, 878).

**Status**: NOT addressed. Still "Mfa" lowercase-a-fa instead of all-caps "MFA".

---

### P2-10 (R4 round-1) — TLA+ Reconcile action without bound rule

**Location**: WI-007 §1 INV_LAYER_1_RECONCILE (line 203-205): `\A r \in Regions, s \in SKUs, h \in Hours: reconciliation_layer_1_drift[r, s, h] =< 0.001`

**Status**: NOT addressed. There is no `Reconcile` action that **updates** drift values based on actual events ↔ counters comparison. The invariant is trivially true because drift starts at 0.0 (Init line 101) and is never updated by any action. Need an explicit `Reconcile(region, sku, hour)` action computing `drift = |events_total - counter_total| / events_total` and updating the state variable, with a SEV-1/SEV-2 transition modeled.

---

## Recommendation

**Recommend one more bis cycle (Lote 10.10-tris)** before SEAL. Target scope:

**Tris cycle priority queue** (estimated 1.5-2 hours total):

1. **Spec contract §8 invariants** (NEW-P0-1): 5-min sed; updates source-of-truth document. Highest priority.
2. **WI-S10-002 PK 4-tuple cascade** (NEW-P0-2): 15-min full sweep across título/narrative/Gherkin/design decisions/INV §12; add `region` field to `CounterRecord` + `LateCounterRecord` Rust structs; add note to `compute_counter_digest` docstring confirming region inclusion. Most damaging if missed.
3. **WI-S10-007 MaxConcurrentEvents CONSTANT** (NEW-P0-3): 5-min add CONSTANT + guard in EmitEvent action; document concrete value in tlc.cfg.
4. **WI-005 PercentValue cluster** (NEW-P1-1): 5-min — change line 74 + line 178 + line 328 to align with line 119 + line 345 (0-200).
5. **WI-001 risk register + post-mortem severity cascade** (NEW-P1-2): 5-min — change R-001/R-002 to HIGH+SEV-2; change post-mortem CRITICAL→HIGH.
6. **WI-006 Legal-at-sprint vs per-replay** (NEW-P1-4 / R4 P1-6 round-1): 15-min — replace Legal with Compliance Officer in 3-of-3 + add CLI mechanism spec.
7. **WI-S10-003 título + WI-005 §12 CTRL-PRIV-002 pseudonymization** (NEW-P1-5): 5-min — change to "data classification tags" + S-11 separate procedure.
8. **WI-S10-004 Layer 2/3 region scoping** (NEW-P1-6): 5-min — add region or explicit aggregation note.
9. **WI-S10-003 §6.1.11 + §9.13 PlanTier residuals** (round-2 P0-1 partial): 2-min — sed `(free, team, enterprise, custom, trial)` → `(free, solo, team, business, enterprise)` in lines 248 and 548.
10. **WI-S10-002 Gherkin scenario UPSERT WHERE residual** (round-2 P0-5 partial): 2-min — line 528 narrative.
11. **TLA+ Reconcile action** (R4 round-1 P2-10): 30-min — add reconcile action with drift computation.

After tris: re-run a quick round-3 validation; if NEW-P0s are addressed, SEAL is justified.

**DO NOT SEAL** at v1.1.0. Risk: at least one cross-region tamper-detection bug (P0-2) and one false-rigor TLA+ claim (P0-3) propagate into PRR HIGH_RISK 12 sign-offs; Compliance Officer / mock SOC 2 auditor will spot them and force rework post-promotion.

**Confidence**: Round-1 R4 audit (6.7/10) + R5 audit (6.4/10) flagged 8 P0s; bis cycle fixed ~5 of 8 cleanly (P0-4 CTRL-AUTHZ, P0-6 R2 Terraform, R5 P0-B CHECK enum, P0-3 TLA+ math numbers, P0-1 PlanTier core). Three P0s have residual or new defects (NEW-P0-1, NEW-P0-2, NEW-P0-3). Tris cycle should close these with high confidence.

The user's "rigor máximo absoluto, tudo impecável e perfeito" bar is **NOT yet met** at v1.1.0. The bis cycle made meaningful progress but missed cascade discipline — exact same anti-pattern as S-08/S-09 round-2.

---

**End audit (Lote 10.10bis post-remediation validation, round 2 by Agent R4 Opus 4.7).**
