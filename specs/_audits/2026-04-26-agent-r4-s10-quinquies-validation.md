---
type: audit
title: Agent R4 (Opus 4.7) round-3 validation of S-10 WIs (Lote 10.10-quaters post-remediation)
date: 2026-04-26
reviewer: Agent R4 (Claude Opus 4.7)
sprint: S-10
target: 7 WIs + sprint contract + failure_modes
round: 3
---

# Agent R4 — S-10 round-3 validation post-Lote-10.10-quaters

## Aggregate score: 8.0/10 (vs round-2 6.9/10, round-1 6.7/10)

**Material progress; quaters cycle absorbed all three round-2 NEW-P0s in their primary locations.** TLA+ `MaxConcurrentEvents` is now in CONSTANTS *and* enforced in EmitEvent action; `quota_grace_active` is a separate VARIABLE; `ReconcileLayer1` action exists with actual drift computation; WI-002 PK 4-tuple cascade is ~95% complete with `CounterRecord`/`LateCounterRecord` Rust structs both carrying `region: Region`; spec contract §8 invariants block now correctly says HIGH+SEV-2; WI-001 risk register R-001/R-002 + post-mortem hooks downgraded to HIGH.

**However, the same anti-pattern recurs at lower amplitude:** the quaters changelog explicitly scopes severity fix as "WI-001 risk register R-001/R-002 + post-mortem hooks" — and that scope is incomplete because **WI-002, WI-003, WI-004, WI-007 risk registers and post-mortem hooks still claim CRITICAL/SEV-1 for INV-BILLING-NO-LOSS / INV-BILLING-NO-DUP**. The sprint contract §5.4 R-S10-4 (line 86) still says aggregator key 3-tuple `(tenant_id, sku, hour)` — contradicting WI-002's 4-tuple PK in the source-of-truth document. WI-004 (Layer 1 reconciliation, the immediate consumer of WI-002's 4-tuple PK) still says "per (tenant, sku, hour)" 3-tuple in título / trait doc / narrative §3 / Gherkin — meaning the actual D1 join semantics may collapse multi-region totals.

Two NEW-P0s prevent SEAL. They are smaller-scope than round-2's P0s but are still cross-document arithmetic-correctness bugs (Layer 1 query semantics + INV severity contradiction) that an SOC 2 mock auditor reviewing the documents will spot.

---

## P0 status table — round-2 inventory

| # | P0 (round-2) | Status | Evidence |
|---|---|---|---|
| **P0-1** (round-1) | PlanTier 5-tuple canonical | ✅ Closed | WI-S10-003 §1 item 11 L247-248 + §6.1.13 L548 + §6.1 schema CHECK L441 + WI-005 enum + spec contract §5.3 R-S10-7 all canonical `free/solo/team/business/enterprise`. Zero residuals (changelog historical text excluded). |
| **P0-2** (round-1) | INV §3.X positions + severity | ⚠️ Partial | Spec contract §8 L149-151 NOW correct (HIGH+SEV-2 NO-LOSS/NO-DUP; CRITICAL APPEND-ONLY). WI §1/§12 blocks correct. **BUT post-mortem hooks + risk registers across WI-002/003/007 still say CRITICAL+SEV-1** for NO-LOSS/NO-DUP — see NEW-P0-1 below. |
| **P0-3** (round-1) | TLA+ math + `MaxConcurrentEvents` CONSTANT | ✅ Closed | TLA+ CONSTANTS L58-65 contains `MaxConcurrentEvents` (L64); EmitEvent guards `Cardinality(events_in_staging) + Cardinality(events_in_r2) + Len(retry_queue) < MaxConcurrentEvents` (L115); UNCHANGED clauses include `quota_grace_active`. `Hours` time-abstraction caveat documented in CONSTANTS comment L62 with both `{0..23}` (1 dia) e `{0..(24*30-1)}` (mês) options. |
| **P0-4** (round-1) | CTRL-AUTHZ-005 hallucinated | ✅ Closed | Zero residuals; consistently `CTRL-AUTHZ-001 + CTRL-AUTHZ-002`. |
| **P0-5** (round-1) | WI-002 UPSERT WHERE | ✅ Closed | SQL `ON CONFLICT ... DO UPDATE SET ...` no longer self-defeating; Gherkin L530 wording fixed; explicit comment about CounterDigestMismatch error path. |
| **P0-6** (round-1) | Cloudflare R2 Terraform | ✅ Closed | WI-001 L298-311 nested `object_lock_configuration` block with pre-merge validation note. |
| **R5 P0-A** (round-1) | WI-002 PK 4-tuple cascade | ⚠️ Partial — 95% complete | DDL ✅ + Rust `CounterRecord` L94-104 has `region: Region` ✅ + `LateCounterRecord` L107-118 has `region: Region` ✅ + trait doc L69 ✅ + título L28+40 ✅ + narrative §2 L47 ✅ + design decision 9.6 L613 ✅ + INV §12 L650 ✅ + Gherkin L529 ✅ + atomic transaction SQL bind L405 ✅. **Cascade miss into WI-S10-004 Layer 1 join semantics** (still 3-tuple "per (tenant, sku, hour)" in título L41 + trait doc L67 + narrative L217 + Gherkin L521) — see NEW-P0-2 below. **Additional miss in spec contract §5.2 R-S10-4 L86** still 3-tuple. |
| **R5 P0-B + P0-E** (round-1) | UsageType strum + D1 CHECK + CloudEvents prefix | ✅ Closed | No new defects. |
| **NEW-P0-1** (round-2; spec contract §8) | Spec contract §8 invariants block | ✅ Closed | Spec contract L149-151 now says HIGH+SEV-2 for NO-LOSS/NO-DUP; CRITICAL retained for AUDIT-APPEND-ONLY (canonical correct). §15 risk register L244 now correctly attributes CTRL-PRIV-002 = "data classification tags @classification=pii" with S-11 separate procedure. |
| **NEW-P0-2** (round-2; WI-002 PK cascade) | Rust struct fields + 15 references | ✅ Closed primary | All flagged round-2 references swept in WI-002. (Cascade into WI-S10-004 Layer 2/3 narrative remains — see NEW-P0-2 below; counted there because root cause is different document.) |
| **NEW-P0-3** (round-2; TLA+ MaxConcurrentEvents) | CONSTANT + guard in actions | ✅ Closed | All elements in actual TLA+ block (not just narrative). Plus quaters added `quota_grace_active` VARIABLE separation + `ReconcileLayer1` action with drift computation (R4 round-1 P2-10 absorbed). |

---

## NEW P0 findings (introduced or surfaced by quaters cycle)

### NEW-P0-1 — Risk register + post-mortem severity cascade incomplete: WI-002/003/004/007 still claim CRITICAL+SEV-1 for INV-BILLING-NO-LOSS/NO-DUP

**Locations**:
- `WI-S10-002-counter-aggregator-cron-do-hash-chain.md`:
  - L745 §28 post-mortem: "INV-BILLING-NO-LOSS Layer 1 violation detected ... → CRITICAL post-mortem (revenue leak)."
  - L746 §28 post-mortem: "INV-BILLING-NO-DUP violation (duplicate counter row) → CRITICAL (double-charge customer downstream)."
  - L786 §28 risk register R-001: `INV-BILLING-NO-LOSS Layer 1 violation (counter drift) | L | M | CRITICAL | M | LOW | 3-layer reconciliation WI-S10-004 + chaos 30d + SEV-1 alert`.
- `WI-S10-003-corelink-billing-stripe-adapter-idempotency-webhook.md`:
  - L869 §28 post-mortem: "INV-BILLING-NO-DUP violation (double-charge customer detected) → CRITICAL post-mortem ..."
  - L870 §28 post-mortem: "INV-BILLING-NO-LOSS violation (Stripe outage caused lost invoice) → CRITICAL + RB-FM-151 review."
- `WI-S10-007-tla-billing-atomicity-runbooks-finance-walkthrough.md`:
  - L743 §24 post-mortem: "RB-FM-151 dry-run reveals INV-BILLING-NO-LOSS violation → CRITICAL post-mortem (cooperation bug fix)."
  - L785 §28 risk register R-005: `RB-FM-151 dry-run reveals INV-BILLING-NO-LOSS violation | L | L | CRITICAL | L | LOW | PAT-QUEUE-EVENTS-001 cooperation bug fix WI-S10-003`.

**Evidence**: The quaters changelog explicitly scopes the fix as "(6) WI-001 risk register R-001/R-002 + post-mortem hooks severity CRITICAL→HIGH; SEV-1 → SEV-2". WI-001 was actually fixed (L674-675 + L714-715 = HIGH/SEV-2). But WI-002/003/007 same-pattern entries were NOT swept. This is the round-2 NEW-P1-2 finding that was scoped narrowly — and the narrow scope is wrong because the cascade pattern is identical.

The contradiction matters because the §1 invariant block in WI-002 (L164-167) says HIGH severity → SEV-2 canonical, and the §12 INV block (L649) says HIGH; registry §3.9 line 136. Then §28 post-mortem hooks 60 lines later reverses to CRITICAL. A reader (Compliance Officer / SOC 2 mock auditor) reviewing the document for severity discipline will flag the contradiction and force rework.

**Fix prescription** (15 minutes total):
1. WI-002 L745: change to "INV-BILLING-NO-LOSS Layer 1 violation detected → HIGH-severity post-mortem (revenue leak protection; Finance + SRE)".
2. WI-002 L746: change to "INV-BILLING-NO-DUP violation (duplicate counter row) → HIGH-severity (double-charge customer downstream; Finance + SRE)".
3. WI-002 L786 R-001: `Imp` column CRITICAL → HIGH; mitigation column "SEV-1 alert" → "SEV-2 alert + Finance review (HIGH severity → SEV-2 canonical per registry §2)".
4. WI-003 L869-870: same wording change as WI-002 L745-746.
5. WI-007 L743: same wording change.
6. WI-007 L785 R-005: `Imp` column CRITICAL → HIGH; rationale: a dry-run *discovering* a real INV violation is a post-mortem trigger but the underlying invariant severity is HIGH; the post-mortem should be HIGH-severity (escalated to CRITICAL only if the violation reached production, not staging dry-run).

**Severity**: P0 because the spec contract source-of-truth says HIGH but four WIs still claim CRITICAL — this is the same severity-contradicting-canonical anti-pattern that round-2 NEW-P0-1 identified, just propagated into different sections. The user's "rigor máximo absoluto" bar requires consistency.

---

### NEW-P0-2 — WI-S10-004 Layer 1 reconciliation join semantics still 3-tuple `(tenant, sku, hour)` after WI-002 PK migrated to 4-tuple

**Location**: `WI-S10-004-reconciliation-worker-3-layer-drift-alerts.md`

**Evidence**:
- L41 título: "**Layer 1**: Σ(R2 events) ↔ Σ(D1 usage_counter) per (tenant, sku, hour) — drift > 0.1% = SEV-2"
- L67 trait doc: `/// Layer 1: R2 events ↔ D1 usage_counter per (tenant, sku, hour).`
- L217 narrative §3 / §6.1: "Layer 1 enforces: Σ(R2 events) = Σ(usage_counter qty) + Σ(usage_counter_late qty) per (tenant, sku, hour)."
- L521 Gherkin: "Given Σ(R2 events) per (tenant, sku, hour) = Σ(D1 usage_counter qty + usage_counter_late qty)"

Additional cross-WI mismatch:
- **Spec contract §5.2 R-S10-4 line 86**: "Counter aggregator cron DO hourly: lê eventos do R2 hour bucket, agrega por `(tenant_id, sku, hour)`, escreve D1 `usage_counter`..." — sprint contract is the parent document; if it asserts 3-tuple aggregation key, WI-002's 4-tuple PK contradicts the inheritance contract.

**Why this matters arithmetically**: WI-002's `usage_counter` PK is `(tenant_id, region, sku, hour)`. WI-004 Layer 1 reads `usage_counter` and compares against R2 events. If Layer 1's SQL `GROUP BY` collapses across regions to `(tenant, sku, hour)`, then:
- Multi-region tenants have 5 rows per (tenant, sku, hour) — one per region.
- Layer 1's `Σ(usage_counter qty)` aggregated to (tenant, sku, hour) sums all 5 rows.
- Layer 1's `Σ(R2 events)` may be computed per-region (per `r2_aggregate_by_sku(region, target_date)` at WI-004 §6.1 around L380s).
- Mismatch: per-region R2 sum vs cross-region D1 sum — Layer 1 drift signal becomes meaningless OR worse, single-region drift is masked by another region's compensating offset.

The quaters changelog claims P1 fix "(i) WI-004 Layer 2/3 narrative aggregation semantics clarified (collapsed across regions for Stripe scope; per-region drift preservada via Layer 1)". This is partially correct for Layer 2/3 (Stripe scope is per-tenant, no region) but Layer 1 itself was NOT updated — the título, trait doc, narrative, and Gherkin all still say 3-tuple. The clarification claim contradicts what's actually in the document.

Round-2 NEW-P1-6 flagged this exactly; quaters declared "addressed" but only addressed Layer 2/3 narrative wording — left Layer 1 untouched.

**Fix prescription** (10 minutes):
1. WI-004 L41 título: change Layer 1 description to "per (tenant, region, sku, hour) — drift > 0.1% = SEV-2".
2. WI-004 L67 trait doc: change to `/// Layer 1: R2 events ↔ D1 usage_counter per (tenant, region, sku, hour).`
3. WI-004 L217 narrative: change to "per (tenant, region, sku, hour)" + add note: "Layer 1 enforces per-region drift detection — collapsing to (tenant, sku, hour) would mask single-region drift via cross-region offset."
4. WI-004 L521 Gherkin: change to "per (tenant, region, sku, hour)".
5. **Spec contract L86 R-S10-4**: change to "agrega por `(tenant_id, region, sku, hour)`" — source-of-truth alignment.
6. (Recommended) Add Gherkin scenario `Layer_1_per_region_drift_detection`: synthesize 0.5% drift in only region=fra; assert Layer 1 alert specifically for region=fra (not collapsed to total).

**Severity**: P0 because (a) spec contract — the inheritance source — contradicts WI-002's primary-key contract; (b) if Layer 1 actually implements 3-tuple aggregation as the documents read, the "0.1% drift" SEV-2 alert can be silently false-negative when multi-region offsets cancel; (c) this is a financial-grade integrity concern surviving from round-2 NEW-P1-6 promoted to P0 because of the cross-document inconsistency.

---

## Outstanding P1 findings

### P1-1 — WI-S10-003 título line 41 still references CTRL-PRIV-002 as pseudonymization

**Location**: `WI-S10-003-corelink-billing-stripe-adapter-idempotency-webhook.md` L41 (título row in §0 table):

> "PII customer name/email FIPS 140-3 encrypted at rest com BYOK S-04 inheritance + CTRL-PRIV-002 pseudonymization for LGPD/GDPR DSR (S-11 cooperation)"

**Evidence**: The quaters changelog claims "(c) WI-003 título L29 + WI-005 L727 + spec contract L244 CTRL-PRIV-002 mis-mapping → 'data classification tags @classification=pii' + S-11 separated DSR". L29 was indeed fixed (the description block at the top of the WI). But the título row in the §0 metadata table at L41 is a duplicate-content row (identical content with different formatting) and was NOT swept. Spec contract L244 ✅ + WI-005 L727 ✅ + WI-003 L29 ✅ — but L41 still says "pseudonymization for LGPD/GDPR DSR".

This is identical to the round-1 título-vs-narrative drift pattern that recurred in S-08 / S-09 cycles.

**Fix prescription**: Sed L41 from "CTRL-PRIV-002 pseudonymization for LGPD/GDPR DSR (S-11 cooperation)" → "CTRL-PRIV-002 (data classification tags @classification=pii; privacy_model.md L209) + S-11 DSR pseudonymization procedure (cooperation)".

---

### P1-2 — WI-S10-006 §29 review checkpoint D+3 still cites Legal in 3-of-3 sign-off context

**Location**: `WI-S10-006-replay-forensic-endpoint-role-audit-trail.md` L872:

> "D+3 Legal (3-of-3 sign-off Legal + GDPR Art. 22 automated decision review)"

**Evidence**: Quaters changelog "(b) WI-006 sign-off `Finance + Legal + Architect` → `Finance + Compliance Officer + Architect`" — fix landed in §6.1 description (L462-465 explicitly documents Legal-at-sprint-only). But the review checkpoint at §29 D+3 still cites "Legal (3-of-3 sign-off Legal ...)" as if Legal participates in the per-replay 3-of-3.

This contradicts the §6.1 fix and the parenthetical at L889 ("Legal sign-off via DPA reference at sprint level + 3-of-3 sign-off process review"). Legal at sprint level is correct convention; review checkpoint should NOT describe Legal as part of per-replay 3-of-3.

**Fix prescription**: Change L872 D+3 from "Legal (3-of-3 sign-off Legal + GDPR Art. 22 automated decision review)" to "Compliance Officer (3-of-3 sign-off review + GDPR Art. 22 automated decision review)" — and add separate "D+X Legal (sprint-level DPA + Stripe contract review; not per-replay)" if the Legal sprint-level checkpoint needs explicit slot.

---

### P1-3 — Spec contract metadata version still v1.1.0 + tag still `sota-v1.1` + updated-date still 2026-04-24

**Location**: `_spec_contract.md` L6, L8, L14:
```
version: "1.1.0"
created: "2026-04-24"
updated: "2026-04-24"
...
tags: [..., "sota-v1.1"]
```

**Evidence**: All 7 WIs bumped to v1.2.0 with `updated: 2026-04-26`. Spec contract body content WAS updated (§8 invariants block at L149-151 — see NEW-P0-1 round-2 → closed). But metadata frontmatter version + tag + date were not updated. Discipline gap. The validate_specs.py 172/178 OK doesn't catch this because the validator probably checks YAML schema completeness, not version consistency.

**Fix prescription**: Bump spec contract metadata to v1.2.0 + updated 2026-04-26 + tag `sota-v1.2`; add v1.2.0 row to spec contract changelog (if §31 exists at sprint contract level — verify).

---

### P1-4 — TLA+ block has unparseable operator/syntax issues (pre-existing, surfacing now under "rigor máximo")

**Location**: `WI-S10-007-tla-billing-atomicity-runbooks-finance-walkthrough.md` TLA+ block L54-261

**Evidence** (multiple sub-issues):
1. **`BillingPeriods` and `RequestIds` referenced but NOT in CONSTANTS** (L97, L105-107, L243). CONSTANTS at L58-65 declares only Tenants, SKUs, Regions, Hours, MaxEventsPerHour, MaxConcurrentEvents, MaxRetries. Init uses `BillingPeriods` for invoice_line_items (L97), reconciliation_layer_2_drift (L105), reconciliation_layer_3_drift (L106), invoice_freeze_active (L107). Next quantifier uses `RequestIds` (L243). TLC will refuse to parse.
2. **Undefined operators**: `SumCounters` (L160), `SumLineItems` (L172), `ComputeStripeInvoiceId` (L168), `EventAccountedInInvoice` (L221), `StripeIdParts` (L226), `Abs` (L205) — referenced but never defined. Either need `LOCAL`/inline definitions OR `EXTENDS` of a helper module that contains them.
3. **Wrong TLA+ partial-update syntax**: Lines 146-147 use `counters_d1[tenant, region, sku, hour]' = new_qty` and `hash_chain_head[region, "usage_counter"]' = "advanced"`. Standard TLA+ requires `counters_d1' = [counters_d1 EXCEPT ![tenant, region, sku, hour] = new_qty]`. Same issue at L159 (invoice_line_items), L208 (reconciliation_layer_1_drift). TLC will reject.

These are pre-existing flaws (round-1 R4 audit had P0-3 narrowly scoped to math + caveat; round-2 narrowly added `MaxConcurrentEvents`). Under "rigor máximo absoluto", a TLA+ block claimed to run in CI green with `THEOREM Spec => [](INV_BILLING_NO_LOSS /\ ...)` (L259) but with undeclared CONSTANTS + undefined operators + wrong syntax cannot actually be TLC-checked — the §6 DoD claim "TLA+ verde em CI" is currently unrealizable from the spec as written.

**Fix prescription**:
1. Add `BillingPeriods` and `RequestIds` to CONSTANTS block (5-min).
2. Add inline definitions for `Abs`, `SumCounters`, `SumLineItems`, `ComputeStripeInvoiceId`, `EventAccountedInInvoice`, `StripeIdParts` either as LOCAL operators in module body or as a separate helper module + EXTENDS clause (15-min).
3. Sed all `var[args]' = expr` patterns to `var' = [var EXCEPT ![args] = expr]` (10-min).
4. (Optional) Run actual TLC dry-run with bounded model (`Tenants={t1,t2}`, `Regions={iad,fra}`, `Hours={0,1}`, etc.) to confirm parse + verify INV_BILLING_NO_LOSS. Report state graph size to validate the "≤ 1M states / 30min CI" claim from §9.3.

**Severity**: P1 (not P0) because the claim is "TLA+ verde em CI sustained 30d" is a §6 DoD checkbox not yet ticked — it's a target state, not a current claim. But under "rigor máximo absoluto, tudo impecável e perfeito", the pseudo-TLA+ as written misleads the reader that it would parse. SOC 2 mock auditor reading the spec will require either real TLC output OR explicit "this is a sketch; production TLA+ in tla/billing_atomicity.tla — see §13 Artifacts" disclaimer.

---

### P1-5 — `compute_counter_digest()` doc comment doesn't explicitly state region IS in canonical_json input

**Location**: `WI-S10-002-counter-aggregator-cron-do-hash-chain.md` L375-385 (compute_counter_digest function)

**Evidence**: The comment at L378 says "Canonical JSON serialization (sorted keys, no whitespace) sans own_digest field" — implies all-fields-except-own_digest, which would include region (since CounterRecord struct has `region: Region` field). But the comment doesn't EXPLICITLY confirm region inclusion. Round-2 audit's NEW-P0-2 fix prescription explicitly asked for "Update `compute_counter_digest()` doc comment to confirm region is included in canonical_json".

The CounterRecord struct at L94-104 has `// region: Region — 5 canonical (iad/fra/nrt/syd/gru); R5 P0-A — region em PK + canonical_json digest input para tamper detection per-region` — so the struct comment IS explicit. But the digest function comment is implicit-by-derivation.

For "rigor máximo": both comments should be explicit-belt-and-suspenders.

**Fix prescription**: Change L378 comment to: "Canonical JSON serialization (sorted keys, no whitespace) sans own_digest field. **Region IS included** in canonical_json input — cross-region records produce distinct digests per R5 P0-A discipline."

---

### P1-6 (residual from round-2) — Quality Standards §14.s10.x.8 + Risk Register R-010/R-011 still cite "INV §3.X position TBD pre-merge" / "verified before commit"

**Status**: NOT addressed in quaters. WI-001 §14.s10.001.8 (L590s area), §14.s10.002.8 / 003.8 / 004.8 / 005.8 / 006.8 / 007.8 + R-010 in WI-001/002/003/006/007 risk registers still cite "canonical position TBD pre-merge" or "INV §3.X position verified before commit" as if the position were unknown.

The positions ARE now known (§3.9 L136/L137 / §3.12 L166/L167 + §3.6 L116). The mitigation column should reflect "verified at v1.2.0 (Lote 10.10-quaters); maintenance discipline ongoing".

**Fix prescription**: Sed all R-010 mitigation columns to "INV positions §3.9/§3.12 verified at v1.2.0; ongoing discipline" OR drop R-010 entirely (risk now retired by definition; replace with another HIGH_RISK risk entry to maintain 12-row count).

---

## Residual P1s from round-2 — status update

### Round-2 NEW-P1-1 (WI-005 PercentValue cluster)

**Status**: ✅ Closed. L74 + L119 + L178 + L328 + L345 + L700 all now consistent at 0..=200; precision (5,2) annotation correct.

### Round-2 NEW-P1-2 (WI-001 INV severity cascade)

**Status**: ✅ Closed for WI-001 specifically (L674-675 + L714-715 = HIGH/SEV-2). **But same pattern recurs in WI-002/003/007** — promoted to NEW-P0-1 above because cascade scope was misjudged.

### Round-2 NEW-P1-3 (R-010 stale "TBD pre-merge")

**Status**: NOT addressed (see P1-6 above).

### Round-2 NEW-P1-4 / R4 round-1 P1-6 (WI-006 Legal-at-sprint vs per-replay)

**Status**: ⚠️ Partial. §6.1 fixed (L462-465); sign-off matrix L889 parenthetical correct. **§29 review checkpoint L872 still cites Legal in 3-of-3** — see P1-2 above.

### Round-2 NEW-P1-5 (CTRL-PRIV-002 pseudonymization)

**Status**: ⚠️ Partial. WI-005 L727 ✅ + spec contract L244 ✅ + WI-003 L29 ✅. **WI-003 L41 título row still says "CTRL-PRIV-002 pseudonymization"** — see P1-1 above.

### Round-2 NEW-P1-6 (WI-004 Layer 2/3 region scoping)

**Status**: ⚠️ Partial. Layer 2/3 narrative clarified (per-tenant Stripe scope, no region). **Layer 1 narrative still says 3-tuple `(tenant, sku, hour)`** — promoted to NEW-P0-2 above because Layer 1 is the immediate consumer of WI-002's 4-tuple PK and a 3-tuple aggregation arithmetic-collapses multi-region drift.

### Round-2 NEW-P1-7

**Status**: ✅ Closed (subsumed by NEW-P1-1).

### Round-1 P1-3 (R4) Stripe Idempotency-Key 64-char rationale

**Status**: ✅ Closed. WI-003 L367-369 ADR rationale documented inline.

### Round-1 P1-9 (R4) corelink_time crate path

**Status**: ✅ Closed. WI-002 §14.s10.002.16 L686 documents crate path `crates/corelink-time/src/canonical.rs` + signature + property tests.

### Round-1 P2-2 (R5) WI-007 ADR property test exemption

**Status**: ✅ Closed. ADR-S10-007-tla-substitutes-proptest documented at WI-007 §6.1.13 L468 + lightweight sanity props NEW (`prop_tla_constants_feasible`, `prop_max_concurrent_events_bound`).

### Round-1 P2-9 (R4) "Mfa" capitalization

**Status**: ✅ Closed (modulo `MfaToken`/`MfaTokenMissing` Rust type identifiers which are camelCase by Rust convention — acceptable). 30 occurrences swept per quaters changelog claim.

### Round-1 P2-10 (R4) TLA+ Reconcile action drift bound

**Status**: ✅ Closed. `ReconcileLayer1` action with `r2_qty - counter_qty` drift_pct computation + INV_LAYER_1_RECONCILE encoding. Modulo P1-4 above (TLA+ syntax issues), the Reconcile action structure is sound.

---

## Recommendation

**Recommend ONE more cycle (Lote 10.10-quinquies)** before SEAL. Target scope is small and surgical (~30-45 minutes total):

**Quinquies cycle priority queue**:

1. **NEW-P0-1 INV severity cascade** — sweep WI-002 L745-746 + L786 + WI-003 L869-870 + WI-007 L743 + L785 to HIGH+SEV-2 (matching WI-001's already-fixed pattern). 15 min.
2. **NEW-P0-2 WI-004 Layer 1 + spec contract R-S10-4 4-tuple** — change WI-004 L41+L67+L217+L521 + spec contract L86 to `(tenant_id, region, sku, hour)`. 10 min.
3. **P1-1 WI-003 título L41** CTRL-PRIV-002 wording. 2 min.
4. **P1-2 WI-006 §29 D+3** review checkpoint Legal → Compliance Officer. 2 min.
5. **P1-3 spec contract metadata** version 1.1.0 → 1.2.0 + updated date + tag. 2 min.
6. **P1-4 TLA+ block** — at minimum add `BillingPeriods` + `RequestIds` to CONSTANTS (other syntax issues acceptable as known-future-work if disclaimer added). 5 min.
7. **P1-5 compute_counter_digest doc** explicit region inclusion comment. 2 min.
8. **P1-6 R-010 mitigation column** "verified at v1.2.0". 5 min × 5 WIs = 25 min (or drop R-010 + replace).

After quinquies: a quick round-4 sanity validation (10-min spot-check) is sufficient if NEW-P0-1 + NEW-P0-2 are addressed surgically. Round-3 score 8.0/10 reflects substantial improvement; the residual P0s are smaller in scope/blast-radius than round-2's three NEW-P0s.

**DO NOT SEAL at v1.2.0**. The two NEW-P0s would be caught by:
- Compliance Officer review (severity contradiction WI-002 §1 = HIGH vs §28 = CRITICAL is exactly what they're trained to spot).
- SOC 2 mock auditor (Layer 1 reconciliation join semantics determine whether per-region drift detection actually works — they will trace the 3-tuple back to WI-002's 4-tuple PK and ask the question).

Both are remediable in ~25 minutes; the quaters cycle delivered ~85% of declared work but the cascade-discipline gap repeats the S-08/S-09 anti-pattern at smaller amplitude. The user's directive "rigor máximo absoluto, tudo impecável e perfeito" is **NOT yet met** at v1.2.0 — but is genuinely close. One quick sweep and it's done.

**Confidence**: round-3 R4 audit (8.0/10) vs round-2 (6.9/10) and round-1 (6.7/10) shows real progress. Quaters absorbed all three round-2 NEW-P0s in primary locations + closed 6 of round-1's residuals + cleanly delivered TLA+ structural improvements (Reconcile action; quota_grace_active separation). The two NEW-P0s are propagation gaps from round-2 NEW-P1-2 (severity) and NEW-P1-6 (Layer 1 region scoping) that the quaters cycle declared "addressed" but only partially executed. Mathematical/discipline rigor is high; the remaining defects are document-cascade-completion (sed scope) not concept-correctness — a straightforward fix.

Compared to S-08 (closed at 4 cycles ≈ 9.0/10) and S-09 (closed at quinquies ≈ 9.2/10), S-10 is on a comparable trajectory. Quinquies is the natural close cycle.

---

**End audit (Lote 10.10-quaters post-remediation validation, round 3 by Agent R4 Opus 4.7).**
