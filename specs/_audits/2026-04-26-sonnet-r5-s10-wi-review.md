---
type: audit
title: Sonnet R5 (Sonnet 4.6) review of S-10 WIs
date: 2026-04-26
reviewer: Sonnet R5 (Claude Sonnet 4.6)
sprint: S-10
target: 7 WIs
---

# Sonnet R5 — S-10 WI adversarial review

## Aggregate score: 6.4/10

The S-10 batch is architecturally ambitious and lessons-citation density is high, but the WIs suffer from a cluster of **mechanical failures that will cause runtime data corruption** — not merely spec inconsistencies. Two findings are novel relative to the concurrent R4 Opus review: (1) the `usage_counter` PRIMARY KEY silently omits `region`, creating a one-row-per-(tenant,sku,hour) constraint that corrupts multi-region counters at the UPSERT level; (2) the D1 staging `CHECK (event_type IN ('cas_put', ...))` accepts values that the `UsageType` enum *never emits* (it serializes to `"corelink.usage.cas.put"` not `"cas_put"`), guaranteeing 100% reject rate at the CHECK constraint on every hot-path insert. Both are runtime P0s with no fallback.

Cross-WI systemic issues (shared with R4 findings): PlanTier 5-tuple is wrong, invariant registry §3.X positions uniformly left "TBD", severity inflation CRITICAL vs HIGH, TLA+ state-space 5× arithmetic error, `CTRL-AUTHZ-005` hallucinated, `CTRL-PRIV-002` mis-mapped, `chrono::next_month_first_utc_midnight()` not a real chrono function. These are confirmed independently; the fix prescriptions in R4 P0-1/P0-2/P0-3/P0-4/P1-9 apply.

R5-exclusive findings are highlighted with [R5-EXCLUSIVE].

---

## Per-WI scores

| WI | Score | Headline finding |
|---|---|---|
| WI-S10-001 (events emitter) | 6.0/10 | D1 `CHECK (event_type IN ('cas_put', ...))` rejects ALL inserts — enum serializes to `"corelink.usage.cas.put"` (long form), CHECK expects short form; 30-region cardinality math error; event type prefix violates Lote 10.9bis P0-G. |
| WI-S10-002 (counter aggregator) | 5.5/10 | `PRIMARY KEY (tenant_id, sku, hour)` omits `region` — multi-region tenants corrupt each other's counters at UPSERT; `CHECK (age_hours > 6)` is an off-by-one (boundary event at exactly 6h is rejected); UPSERT WHERE clause self-defeats (R4 P0-5). |
| WI-S10-003 (Stripe adapter) | 6.5/10 | §2 narrative describes a **different, incompatible** idempotency key format (62-char UUID-based) vs §6.1 code (35-char short-form); PlanTier names wrong per data_model.md; double-`+` typo. |
| WI-S10-004 (reconciliation) | 7.0/10 | Generally strongest of the 7. Late-event join algebra ambiguous (R4 P1-8). INV positions TBD. Otherwise solid. |
| WI-S10-005 (quota state machine) | 6.0/10 | 5th `GraceActive` state undocumented in sprint contract; `CHECK (current_pct <= 200)` allows 2× overage with no upper guard in state machine; `current_pct: PercentValue` (line 119) typed "0-100" contradicts DB CHECK "0-200". |
| WI-S10-006 (replay endpoint) | 6.5/10 | CTRL-AUTHZ-005 hallucinated (R4 P0-4); sign-off emission mechanism unspecified (R4 P1-6); CloudEvents type prefix correct `dev.hugr.corelink.*`. |
| WI-S10-007 (TLA+ + runbooks) | 5.5/10 | State-space wrong 5× (150k claimed, 30k actual) repeated at 5 locations; §0 title says "15k states" (3k actual) — two contradictory wrong numbers in same WI; TLC state-space model conceptually flawed (R4 P0-3). |

---

## P0 findings (catastrophic — block sprint promotion)

### P0-A [R5-EXCLUSIVE] — WI-002: `usage_counter` PRIMARY KEY omits `region`; multi-region data collision at every UPSERT

**Location**: `WI-S10-002-counter-aggregator-cron-do-hash-chain.md` line 312

**Evidence**:
```sql
PRIMARY KEY (tenant_id, sku, hour),
CHECK (region IN ('iad', 'fra', 'nrt', 'syd', 'gru')),
```

The table stores `region TEXT NOT NULL` (line 308) and the sprint contract §5.2 mandates per-region counters. But the PRIMARY KEY is `(tenant_id, sku, hour)` — `region` is **NOT in the PK**. For a tenant writing from regions `iad` and `fra` during the same hour:

- Row 1: `(tenant_1, cas_put_op_count, 1745712000)` region=iad, qty_ops=150
- Row 2 UPSERT attempt: `(tenant_1, cas_put_op_count, 1745712000)` region=fra, qty_ops=200

The second UPSERT hits the existing row (same PK) and **overwrites** qty_ops with 200, silently discarding the iad data. Final counter shows fra only. INV-BILLING-NO-LOSS is violated without any alert — the Layer-1 reconciliation will detect drift (R2 has 350 ops; D1 has 200) only after the fact, triggering spurious SEV-2s on every multi-region tenant.

Note: R4 P0-5 identified a `WHERE usage_counter.own_digest = excluded.own_digest` clause as self-defeating, but framed idempotency as "already enforced by PRIMARY KEY UNIQUE" — this is incorrect; the PK is wrong. The WHERE clause bug and the PK bug compound: even removing the WHERE clause, the UPSERT would still corrupt multi-region counters because the PK uniqueness constraint silently merges distinct region rows.

**Fix prescription**: Change PRIMARY KEY to `(tenant_id, sku, region, hour)`. Update all UPSERT SQL, JOIN logic, and all index definitions referencing the old PK. Update §6.1.3 Layer-1 reconciliation algebra (which correctly joins on `(tenant, sku, hour)` — must add `region`). Update WI-S10-004 Layer-1 references. Add proptest `prop_multi_region_counters_independent` verifying five concurrent iad/fra/nrt/syd/gru UPSERTs produce five distinct rows.

---

### P0-B [R5-EXCLUSIVE] — WI-001: D1 `CHECK (event_type IN ('cas_put', ...))` always rejects inserts; enum serializes to long-form `"corelink.usage.cas.put"`

**Location**: `WI-S10-001-usage-event-emitter-r2-append-only-idempotency.md` lines 82–91 (enum definition), line 332 (CHECK constraint)

**Evidence**:

Enum definition (lines 82–91):
```rust
#[derive(strum::Display, strum::EnumIter, serde::Serialize)]
pub enum UsageType {
    #[strum(serialize = "corelink.usage.cas.put")]
    CasPut,
    #[strum(serialize = "corelink.usage.cas.get")]
    CasGet,
    #[strum(serialize = "corelink.usage.ac.lookup")]
    AcLookup,
    #[strum(serialize = "corelink.usage.gc.purge")]
    GcPurge,
}
```

D1 staging table CHECK constraint (line 332):
```sql
CHECK (event_type IN ('cas_put', 'cas_get', 'ac_lookup', 'gc_purge'))
```

When an event is emitted, `event_type` is serialized from `UsageType` via `strum::Display`, producing `"corelink.usage.cas.put"` — not `"cas_put"`. The CHECK constraint rejects all values with the full dotted-path prefix. Every hot-path emit inserts into the staging table and **every insert fails the CHECK constraint** with a D1 constraint violation.

Since the WI is fail-OPEN at hot path (line 202: "billing event emit fail-OPEN at hot path"), the CHECK failure is silently swallowed at emit time, but the event is never staged. No retry succeeds because the value will always be the long form. No event drains to R2. INV-BILLING-NO-LOSS is violated for every event from day 1 — not as an edge case but as the default operation.

Secondary observation: line 106 (`idempotency_key: IdempotencyKey` comment: "UUID v4 derived `corelink-{tenant_id}-{event_hash}`") uses the long UUID format in the comment while §6.1 uses the short-form 35-char format. This commentary inconsistency is a P1 (see P1-C below).

**Fix prescription**: Align CHECK constraint values with enum serialization. Either:
- (a) Change CHECK to `('corelink.usage.cas.put', 'corelink.usage.cas.get', 'corelink.usage.ac.lookup', 'corelink.usage.gc.purge')` — matching what the enum actually emits; OR
- (b) Change `UsageType` serialization to short-form `("cas_put", "cas_get", "ac_lookup", "gc_purge")` and update CloudEvents `type` field derivation accordingly (note: option b conflicts with Lote 10.9bis P0-G canonical prefix requirement).

Option (a) is preferred: it keeps enum serialization consistent with the CloudEvents `type` field and fixes only the CHECK constraint. Add proptest `prop_usage_type_serialization_matches_check_constraint` asserting `UsageType::iter().all(|t| t.to_string().starts_with("corelink.usage."))`.

---

### P0-C — WI-007: TLA+ state-space 5×5×24×5×10 = 30,000 claimed as 150,000 (5× error) at 4 locations; additional 5×5×24×5 = 3,000 claimed as 15,000 at §0 title

**Location**: `WI-S10-007-tla-billing-atomicity-runbooks-finance-walkthrough.md`
- Line 40 (§0 title): "5 tenants × 5 SKUs × 24 hours × 5 regions = 15k states" — actual = **3,000**
- Line 239 (§1 invariants): "5 × 5 × 24 × 5 × 10 = ~150k states" — actual = **30,000**
- Line 297 (§2 narrative): same "~150k states" claim — **30,000**
- Line 375 (§6.1.3): same "~150k states" — **30,000**
- Line 581 (§9.4 Design Decisions): "5 × 5 × 24 × 5 × 10 = 150k states baseline" — **30,000**

The one correct calculation in the file: line 542 "5 × 5 × 24 × 5 × 1000 = 15M states" — this arithmetic is correct.

This is P0 because: (1) CI feasibility decisions are calibrated against the stated baseline; (2) state space is presented to Finance + mock auditor as a formal rigor argument — incorrect math undermines the SOC 2 CC1.4 evidentiary claim; (3) as R4 correctly notes, the conceptual model itself is wrong — TLC explores reachable transitions not Cartesian product of CONSTANTS, so the actual state count depends on action enabling rules. The WI needs a TLC dry-run to get real state count.

**Fix prescription**: Correct all arithmetic (3,000 and 30,000). Add a note that these are CONSTANTS-product upper bounds on state variable assignments, not TLC's reachable-state-graph count; add `MaxConcurrentEvents` CONSTANT to bound unbounded set variables (`events_in_staging`, `retry_queue`). Require a staging TLC dry-run and report actual reachable states in the WI §9.4 before sprint promotion.

---

### P0-D — All WIs: INV-BILLING-NO-LOSS and INV-BILLING-NO-DUP claimed CRITICAL; canonical registry §3.9 says HIGH

**Location**: All 7 WIs, §1 invariants block. Representative: WI-S10-007 line 237 "INV-BILLING-NO-LOSS (CRITICAL; registry §3.X canonical position TBD)".

**Evidence**: `invariant_registry.md` §3.9 (line 136): `INV-BILLING-NO-LOSS | HIGH`. Line 137: `INV-BILLING-NO-DUP | HIGH`. Registry §2 states CRITICAL requires TLA+ mandatory in CI as a precondition. The WIs claim CRITICAL without the TLA+ being written yet (WI-S10-007 §10.s10.007.1 is a completeness checkbox, not done).

Furthermore, all WIs claim "registry §3.X canonical position TBD" — but the actual positions are §3.9 (NO-LOSS, NO-DUP) and §3.12 (RECONCILE-3-LAYER, REPLAYABLE-FROM-EVENTS). This violates Lote 10.8bis P1-13 lesson: "verify INV §3.X canonical position MUST be verified before commit."

**Fix prescription**: Either (a) promote INV-BILLING-NO-LOSS/NO-DUP to CRITICAL in `invariant_registry.md §3.9` with an ADR (justified by WI-S10-007 TLA+ verification), contingent on TLA+ CI green — and update all WIs once ADR is merged; OR (b) correct all WI claims from CRITICAL to HIGH and change "SEV-1" alert wording to "SEV-2 + Finance review" where the alert derives from the CRITICAL claim. Replace all "§3.X canonical position TBD" with exact citations: `§3.9 line 136` (NO-LOSS), `§3.9 line 137` (NO-DUP), `§3.12 line 166` (RECONCILE-3-LAYER), `§3.12 line 167` (REPLAYABLE-FROM-EVENTS).

---

### P0-E — WI-001: CloudEvents `type` prefix `"corelink.usage.*"` violates Lote 10.9bis P0-G canonical `"dev.hugr.corelink.*"`

**Location**: `WI-S10-001-usage-event-emitter-r2-append-only-idempotency.md` lines 83–90, line 40 (§0 title).

**Evidence**: Lote 10.9bis P0-G establishes canonical CloudEvents type prefix as `dev.hugr.corelink.<op>.v1`. WI-S10-001 emits `"corelink.usage.cas.put"` (no `dev.hugr.` prefix, no `.v1` suffix). WI-S10-005 and WI-S10-006 correctly use `dev.hugr.corelink.quota.state.transitioned.v1` and `dev.hugr.corelink.billing.replay.requested.v1` respectively — so within S-10, WI-001 is the outlier. This creates a schema mismatch at the R2 archival layer: reconciliation workers consuming R2 events will read type `"corelink.usage.cas.put"` but filter for `"dev.hugr.corelink.*"` (as WI-S10-004 and WI-S10-006 would expect).

Note: sprint contract §5.1 R-S10-1 itself uses the old format `corelink.usage.cas.put` (the contract did not absorb Lote 10.9bis P0-G). The WI follows the contract — but the contract is wrong relative to the canonical lesson. This is a sprint-contract + WI coordinated fix.

**Fix prescription**: Update `UsageType` strum serialization to `"dev.hugr.corelink.cas.put.v1"`, `"dev.hugr.corelink.cas.get.v1"`, `"dev.hugr.corelink.ac.lookup.v1"`, `"dev.hugr.corelink.gc.purge.v1"`. Update D1 CHECK constraint accordingly. Update sprint contract §5.1 R-S10-1 to reflect new prefix. Add schema_version CloudEvents extension attribute `"1.0.0"` per line 215-218 (already cited but not reflected in enum type strings).

---

## P1 findings (significant — deviations from canonical sources)

### P1-A [R5-EXCLUSIVE] — WI-001: D1 staging table lacks `region` column; WI-002 CHECK confirms region stored but staging doesn't

**Location**: WI-S10-001 lines 322–340 (staging table schema)

**Evidence**: The `usage_event_staging` table stores `(tenant_id, request_id, event_type, event_payload_hash, event_id, emitted_at, drained_to_r2_at)` — no `region` column. But `UsageEventData::CasPut` (line 100) includes `region: Region` in its payload. When draining to R2, the R2 key is `billing-events-<region>/...` (line 344: "Queue `billing-events-drain-<region>` per region"). The drain consumer must know the region to write to the correct R2 bucket, but staging has no region tag — it can only discover region by deserializing `event_payload_hash` (which is a BLAKE3 hash of the payload, not the payload itself).

The drain consumer has no performant path from staging to the correct per-region R2 bucket. If the drain consumer uses a single global R2 bucket, events mix regions and WI-S10-004 Layer-1 reconciliation (which reads per-region R2 buckets) cannot operate correctly.

**Fix prescription**: Add `region TEXT NOT NULL CHECK (region IN ('iad', 'fra', 'nrt', 'syd', 'gru'))` to `usage_event_staging`. Populate from `UsageEventData.region` at emit time. Update drain queue routing to use this column. Update the index `idx_staging_per_tenant` to `(tenant_id, region, emitted_at)`.

---

### P1-B [R5-EXCLUSIVE] — WI-003: §2 narrative describes a 62-char idempotency key (UUID 36) contradicting §6.1 code which uses 35-char short form

**Location**: `WI-S10-003-corelink-billing-stripe-adapter-idempotency-webhook.md` lines 275 and 357

**Evidence**:

Line 275 (§2 narrative):
> "format constrained to 64 chars max canonical (`corelink-` 9 + UUID 36 + dash 1 + hash 16 = 62)"

This describes `corelink-{full-uuid-36}-{hash-16}` = 62 chars.

Line 357 (§6.1 implementation):
> "corelink-{tenant_id_short(8)}-{operation}-{event_hash_short(8)}" = 9 + 8 + 1 + 8 + 1 + 8 = 35 chars

Two incompatible formats in the same WI. The §6.1 format (35 chars) is referenced in WI-S10-001 line 208-209 and is consistent with the sprint contract. The §2 narrative format (62 chars, full UUID) is inconsistent with both the implementation spec and the Sprint contract.

The Stripe 24h idempotency window semantics differ between the two formats: the 35-char format encodes `operation` (e.g., "invoice") making it semantically namespaced; the 62-char format loses operation namespace. Downstream Stripe dedup semantics differ.

**Fix prescription**: Replace line 275 narrative with the correct format: "`corelink-` 9 + tenant_id_short(8) 8 + dash 1 + operation 8 + dash 1 + event_hash_short(8) 8 = **35 chars**; well within Stripe's 255-char limit; internal assertion at 64 chars (safety margin)." Delete the UUID-36 reference entirely. Add a clarifying sentence: "`tenant_id_short(8)` = first 8 hex chars of tenant UUID, hyphens stripped; `event_hash_short(8)` = first 8 hex chars of BLAKE3(canonical_json(payload))."

---

### P1-C — WI-001 line 106: idempotency key comment uses wrong long-UUID format

**Location**: `WI-S10-001-usage-event-emitter-r2-append-only-idempotency.md` line 106

**Evidence**:
```rust
idempotency_key: IdempotencyKey,  // UUID v4 derived `corelink-{tenant_id}-{event_hash}`
```

Comment says `corelink-{tenant_id}-{event_hash}` using full tenant_id (UUID 36 chars + prefix = 62 chars), contradicting §6.1 35-char format. This is the same format confusion as P1-B but in WI-001. Minor but confuses implementers reading the struct definition.

**Fix prescription**: Update comment to "`corelink-{tenant_id_short(8)}-{operation}-{event_hash_short(8)}` = 35 chars; see WI-S10-003 §6.1 for derivation".

---

### P1-D — WI-002: `CHECK (age_hours > 6)` is off-by-one; boundary events at exactly 6h are incorrectly rejected

**Location**: `WI-S10-002-counter-aggregator-cron-do-hash-chain.md` line 347

**Evidence**:
```sql
CHECK (age_hours > 6),  -- definition: late = > 6h
```

The trait doc (line-context from summary) says late threshold is `ts < now() - 6h`, which at exactly 6h produces `age_hours = 6`. The CHECK rejects `age_hours = 6` (requires `> 6`). An event at exactly 6h old fails the CHECK and cannot be inserted into `usage_counter_late`, nor can it go into `usage_counter` (since it's beyond the normal window). It is silently dropped — INV-BILLING-NO-LOSS violation for exactly-6h-old events.

Additionally, the aggregation step 5 late event filter uses `event.ts < hour_window.start_ts - 6h` (relative to window start) while the trait doc uses `ts < now() - 6h` (relative to current time). These produce different sets when cron runs mid-hour: an event 5.5h old relative to now() may be 7h old relative to window start. This logic inconsistency can cause events to be classified as late on some cron cycles and normal on others.

**Fix prescription**: Change `CHECK (age_hours > 6)` to `CHECK (age_hours >= 6)` to include exactly-6h events. Resolve late event filter inconsistency: choose one reference time (`now()` or `window_start`) and document it explicitly in both the trait doc and the CHECK constraint comment. Preferred: use `now()` as reference for runtime `age_hours` computation (worker sees the event now); `hour_window.start_ts - 6h` is semantically wrong (would classify events as "6h late from the start of the window they belong to", inflating age).

---

### P1-E — All WIs: invariant registry §3.X positions left "TBD"; Lote 10.8bis P1-13 not absorbed

(Confirmed by R5 independently; identical to R4 P0-2. Fix: replace "§3.X canonical position TBD" with §3.9 line 136/137 for NO-LOSS/NO-DUP and §3.12 line 166/167 for RECONCILE-3-LAYER/REPLAYABLE.)

---

### P1-F — WI-003 §0 title and PlanTier enum: lists only 4 tier names with double-`+` typo; missing `trial`

**Location**: `WI-S10-003-corelink-billing-stripe-adapter-idempotency-webhook.md` lines 29 and 73

**Evidence**:
- Line 29 title: `"5 canonical tiers free/team/enterprise/custom + + Lote 10.7bis P0-7"` — double `+` and lists only 4 names (free, team, enterprise, custom), claims "5 canonical"
- Line 73 PlanTier comment: `"// 5 canonical (free/team/enterprise/custom + + Lote 10.7bis P0-7)"` — same double `+`

(Note: per R4 P0-1, the correct canonical 5-tuple per `data_model.md §1` is `free/solo/team/business/enterprise`, not `free/team/enterprise/custom/trial`. This P1-F covers the additional formatting failure layered on top of the already-wrong tier list.)

**Fix prescription**: Fix double `+` to single `+`. Fix tier list to canonical: `free/solo/team/business/enterprise`. Recount to confirm "5 canonical" matches 5 actual values.

---

### P1-G — WI-001: cardinality uses 30 regions instead of 5 canonical; math wrong in both §1 and §6.1

**Location**: `WI-S10-001-usage-event-emitter-r2-append-only-idempotency.md` lines 213 and 396

**Evidence**:
- Line 213: "5 SKUs × 5 tier × **30 region** = 750 séries" — canonical region count is 5 (iad/fra/nrt/syd/gru), not 30
- Line 396: "4 types × **30 regions** = 120 séries" — should be 4 × 5 = 20

Correct cardinality: 5 × 5 × 5 = 125 séries; 4 × 5 = 20 séries. Total ~145 (well under INV-OBS-CARDINALITY-BUDGET 20k per metric; but the inflated 750 claim may trigger false concern or be used to justify future cardinality expansion without review).

**Fix prescription**: Replace all "30 region" references in WI-S10-001 with "5 region (iad, fra, nrt, syd, gru)". Recompute totals. Cross-check WI-S10-009-001 cardinality budget inheritance cited in §1.

---

### P1-H — WI-005: `QuotaEvaluation.current_pct` typed `0-100` but DB CHECK allows `0-200`; inconsistency undermines state machine safety

**Location**: `WI-S10-005-quota-state-machine-overage-email.md` lines 119 and 345

**Evidence**:
- Line 119 struct field: `pub current_pct: PercentValue, // 0-100 typed; CHECK 0..=100 enforced`
- Line 345 DB CHECK: `CHECK (current_pct >= 0 AND current_pct <= 200), -- allows over 100 for reporting (capped at 200 sanity)`

The type-level comment says "CHECK 0..=100 enforced" but the actual DB CHECK is `0..=200`. A tenant at 180% quota can be stored in the DB, but `PercentValue` (typed `0-100`) will either panic, saturate, or produce a conversion error when Rust reads the row. If `PercentValue::new(180)` panics, reading any tenant above 100% crashes the quota worker — fail-CLOSED at evaluation (fine per Lote 10.6bis) but the DB holds unreachable states.

Additionally, sprint contract §5.5 R-S10-10 defines `hard_block` triggers at 100%. If the system allows storage up to 200% and quota enforcement only fires at 100%, there is a window where a tenant at 150% who somehow bypassed hard-block (e.g., during grace) has no recalibration path — the state machine has no transition from `grace_active` back into `hard_block` if current_pct drops then rises above 100 again during the grace period.

**Fix prescription**: Align DB CHECK and PercentValue type: either `CHECK (current_pct >= 0 AND current_pct <= 100)` with `PercentValue(0..=100)` — treating > 100% as a hard error saturated to 100 in the aggregator; or use a `OveragePercent` newtype with `0..=200` range but update the struct comment and add an explicit saturation guard in the evaluator before calling state machine transitions.

---

### P1-I — Sprint contract §5.7 R-S10-15: metric names use dots (OpenTelemetry); WIs correctly use underscores (Prometheus); contract is the drifted source

(Confirmed by R5 independently; identical to R4 P1-11. Fix: update sprint contract §5.7 R-S10-15 to underscores.)

---

### P1-J — WI-002: `CHECK (hour % 3600 = 0)` at line 320 is semantically correct but comment "unix epoch hour" is misleading

**Location**: `WI-S10-002-counter-aggregator-cron-do-hash-chain.md` line 320

**Evidence**: `CHECK (hour % 3600 = 0) -- enforces UTC hour-alignment (unix epoch hour)`. The `hour` column is unix timestamp in seconds (not milliseconds per the `aggregated_at` column). `unix_ts % 3600 = 0` correctly enforces hour alignment in seconds. The comment calls it "unix epoch hour" which could be confused with hour-of-day (0..23). The check is correct but the comment is unclear.

Also: `aggregated_at INTEGER NOT NULL, -- unix ms` at line 307 uses milliseconds, while `hour` uses seconds. Mixed time units in the same table without explicit suffix differentiation (Lote 10.7bis P0-3 requires no `_ms` suffix — fine — but semantics must be documented consistently).

**Fix prescription**: Update comment to "unix timestamp seconds; must be on-the-hour boundary (ts % 3600 = 0)" and explicitly document that `aggregated_at` is unix milliseconds and `hour` is unix seconds in the table-level DDL comment block.

---

## P2 findings (minor — wording, formatting, documentation)

### P2-1 — WI-003: `assert_test_mode_in_ci()` is runtime panic, not compile-time

**Location**: `WI-S10-003-corelink-billing-stripe-adapter-idempotency-webhook.md` (§6.1 discipline block, line ~277)

The WI describes this as a "compile-time `assert_test_mode_in_ci()`" that "panics if live detected em test build." `cfg!(any(test, debug_assertions))` is a compile-time conditional — but the panic itself fires at runtime in test builds, not at compile time. The function does not prevent a `debug_assertions`-off release build from shipping with `CORELINK_STRIPE_MODE=live` in test CI. True compile-time enforcement requires a `build.rs` gate or `#[cfg]` attribute, not a runtime panic conditional.

**Fix prescription**: Clarify this is a "runtime guard in test/debug builds" not compile-time. Consider adding a `build.rs` that reads `CORELINK_STRIPE_MODE` at build time and errors if `live` key is detected in a `debug_assertions` profile.

---

### P2-2 — WI-007 §6.1.13: 0 property tests self-exemption without ADR for HIGH_RISK WI

**Location**: `WI-S10-007-tla-billing-atomicity-runbooks-finance-walkthrough.md` §6.1.13 (confirmed from summary)

WI-S10-007 is HIGH_RISK lane. Lote 10.7bis P1-3 establishes 100k nightly property tests as HIGH_RISK SOTA bar. The WI self-exempts with "0 property tests (TLA+ model checking is the primary)". No ADR is referenced justifying this exemption for HIGH_RISK lane.

**Fix prescription**: Either add an ADR (`ADR-S10-007-TLA-exempt-from-proptest`) documenting WHY TLA+ verification substitutes for property tests in this WI, or add at minimum 1 property test validating `billing_atomicity.tla` constants are within CI-feasible bounds (e.g., `5×5×24×5×10 < 1_000_000`).

---

### P2-3 — WI-005: `GraceActive` is a 5th state but sprint contract §5.5 R-S10-10 defines "4-state machine"

**Location**: `WI-S10-005-quota-state-machine-overage-email.md` lines 28, 100-112

The WI title says "canonical 4-state machine" but implements 5 states (Under80, SoftAlert, Ticket, HardBlock, GraceActive). The `grace_active` state has no defined transition-out rule in the state machine trait or sprint contract — specifically, what happens when grace expires (grace_days = 0) and current_pct is still > 100%? Does the machine re-enter `hard_block`? The transition is undefined.

**Fix prescription**: Either (a) represent grace as a flag (`is_grace_active: bool`) on the existing 4 states (aligns with sprint contract); or (b) formally add GraceActive to sprint contract §5.5 R-S10-10 with explicit entry/exit transitions and update title to "5-state machine". Document the GraceActive → HardBlock re-entry rule when `grace_expires_at < now() AND current_pct >= 100`.

---

### P2-4 — WI-001 line 213: cardinality budget claim "well under WI-S09-001 100k budget" is correct but the baseline number is fabricated from 30 regions

Already covered in P1-G. Noting here that even with the corrected 5-region cardinality (125 séries for billing metrics), the observation "well under 100k budget" holds — so the functional conclusion is fine but the derivation is wrong.

---

### P2-5 — All WIs: `chrono::next_month_first_utc_midnight()` does not exist in the chrono crate

(Confirmed by R5 independently; identical to R4 P1-9. Must be defined as a canonical internal helper in `crates/corelink-time/src/canonical.rs` or replaced with explicit chrono ops.)

---

## Cross-WI consistency verified (pass)

| Check | Status | Notes |
|---|---|---|
| 5 canonical SKUs consistent | PASS | All WIs use `cas_storage_gb_month, cas_egress_gb, cas_put_op_count, cas_get_op_count, ac_lookup_op_count` |
| R2 Object Lock retention math 7y = 2,557 days | PASS | 365×7+2 = 2,557 ✓ (all WIs claim 7y; calculation verified) |
| D1 batch ≤ 250 per Lote 10.5bis | PASS | WI-S10-001 line 344 states "D1 batch ≤ 250 per Lote 10.5bis" ✓ |
| `worker::send_future` (not tokio::spawn) in CF Workers | PASS | WI-S10-001 line 206 correctly cites `worker::send_future()` ✓ |
| 12 sign-off rows in every WI | PASS | All 7 WIs have 12 sign-off rows; HIGH_RISK cap compliant |
| 100k nightly property tests claim | PASS | All 6 applicable WIs claim it; WI-007 self-exempts (P2-2) |
| Chaos scenarios ≥ 10 per WI | PASS | All 7 WIs have ≥ 10 chaos scenarios |
| Idempotency key 35-char math | PASS | §6.1 format: 9+8+1+8+1+8 = 35 ✓ (§2 narrative wrong — see P1-B) |
| 5 canonical regions in CHECK constraints (WI-002, WI-004) | PASS | WI-002 line 314 and WI-004 correctly enumerate iad/fra/nrt/syd/gru |
| PlanTier names canonical per data_model.md | FAIL | All WIs use `free/team/enterprise/custom/trial`; canonical is `free/solo/team/business/enterprise` (R4 P0-1) |
| CloudEvents type prefix `dev.hugr.corelink.*` | PARTIAL | WI-005/006 correct; WI-001 wrong (P0-E); WI-002/003/004 TBD (check enum serialization) |

---

## Summary — priority fix sequence for bis cycle

1. **P0-A** (WI-002 region PK) — data corruption; fix PK first before any UPSERT logic changes
2. **P0-B** (WI-001 D1 CHECK mismatch) — 100% insert failure rate; fix immediately
3. **P0-C** (WI-007 TLA+ math) — misleading formal rigor claim; fix before Finance walkthrough
4. **P0-D** (CRITICAL vs HIGH severity + §3.X positions) — global fix, one grep pass
5. **P0-E** (WI-001 CloudEvents prefix) — coordinate with sprint contract §5.1 fix
6. **P1-A** (WI-001 staging table missing region) — required for correct drain routing
7. **P1-B** (WI-003 idempotency key format contradiction) — single paragraph fix
8. **P1-D** (WI-002 `age_hours > 6` off-by-one) — 2-char fix + test
9. **R4 P0-1** (PlanTier canonical) — broad multi-WI fix; coordinate with data_model.md
10. **R4 P0-4** (CTRL-AUTHZ-005 hallucinated) — add to security_model.md or replace refs
