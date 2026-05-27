---
type: audit
title: Agent R4 (Opus 4.7) review of S-10 WIs
date: 2026-04-26
reviewer: Agent R4 (Claude Opus 4.7)
sprint: S-10
target: 7 WIs
---

# Agent R4 — S-10 WI adversarial review

## Aggregate score: 6.7/10

The S-10 cycle reaches "structurally complete + lessons-aware" but has **systemic** P0 drift on (a) the canonical `Plan` tier list (every single WI uses an invented 5-tuple `free/team/enterprise/custom/trial` that contradicts `data_model.md §1` `free/solo/team/business/enterprise`), (b) invariant-registry position claims uniformly stamped "§3.X canonical position TBD" when the positions are *known* and documented (Lote 10.8bis P1-13 lesson **claimed but not absorbed**), and (c) severity drift on INV-BILLING-NO-LOSS / INV-BILLING-NO-DUP (registry says HIGH; every WI claims CRITICAL). WI-S10-007 has a hard math error on TLA+ state-space (claim 150k vs computed 30k) and a separate inconsistent claim of 15k earlier in the same WI. Two hallucinated control IDs (`CTRL-AUTHZ-005`, plus a CTRL-PRIV-002 mis-mapped to "pseudonymization") propagate across WI-S10-005/006/007. Cardinality math also drifts inside WI-S10-001 (5 regions canonical contradicted by "30 regions" used in séries calculation).

This is roughly the same defect surface as S-08 / S-09 round 1: the lessons matrix is *cited* with high fidelity but the lessons themselves are *not enforced* on first emission. Expect 2-3 bis cycles to reach SOTA.

## Per-WI scores

| WI | Score | Headline finding |
|---|---|---|
| WI-S10-001 (events emitter) | 7.0/10 | Plan-tier drift (claims §10 5-tuple but emitter doesn't use it; 30-region cardinality contradicts 5-region canonical); INV positions still "TBD"; otherwise solid Lote 10.6bis split-tier reasoning. |
| WI-S10-002 (counter aggregator) | 7.5/10 | Best of the 7. UPSERT logic has a self-defeating WHERE clause (line 409 `WHERE usage_counter.own_digest = excluded.own_digest`) that prevents legitimate UPSERT updates. INV positions TBD. |
| WI-S10-003 (Stripe adapter) | 6.0/10 | Plan tier list `free/team/enterprise/custom/trial` directly contradicts `data_model.md §1` `free/solo/team/business/enterprise`; every Postgres schema CHECK constraint will be wrong. Severity drift on INVs. |
| WI-S10-004 (reconciliation) | 7.0/10 | Layer 2/3 only "monthly" but Layer 1 daily reconcile uses `target_date = previous UTC midnight` (yesterday) — fine; but `corelink_billing_reconcile_drift_pct{region, layer, sku, tenant_tier}` = 5×3×5×5 = 375 séries collides with INV-OBS-CARDINALITY-BUDGET 20k per-metric ceiling (fine) but 600 séries baseline claim (line 660) lowballs total (real: ~750 from this metric alone). |
| WI-S10-005 (quota state machine) | 5.5/10 | Plan tier list wrong (`'free', 'team', 'enterprise', 'custom', 'trial'` line 344 CHECK). CTRL-PRIV-002 mapped to pseudonymization (wrong scope: it's data classification tags). 5 PlanTier × 5 PlanTier × 5 PlanTier = 125 séries cardinality `corelink_billing_quota_state_transitions_total{from_state, to_state, plan_tier}` is actually 5×5×5 (states × states × tiers) and `from_state == to_state` is impossible after idempotent dedup so true is more like 4×4×5=80, but base claim 125 is over-stated. |
| WI-S10-006 (replay endpoint) | 6.5/10 | CTRL-AUTHZ-005 hallucinated (security_model.md only defines CTRL-AUTHZ-001 and CTRL-AUTHZ-002). 3-of-3 sign-off via "Architect" role but no other WI in the program has approved an Architect-as-distinct-role-from-Security-Lead pattern. |
| WI-S10-007 (TLA+ + RBs) | 5.5/10 | TLA+ state-space math wrong twice in the same WI (claim 15k §0 line 62-63; claim 150k §6.1.3 line 375 / §1 line 239; computed value is 3,000 baseline or 30,000 with events-per-hour). RB-FM-151 created without canonical FM->RB mapping update in failure_modes.md (currently only RB-FM-302 listed at line 274). INV-BILLING-NO-LOSS/NO-DUP severity drift CRITICAL claim contradicts registry HIGH. |

## P0 findings (catastrophic — block sprint promotion)

### P0-1 — Plan tier 5-tuple is invented; contradicts canonical `data_model.md §1`

**Location**: WI-S10-003 lines 73, 113-124, 437; WI-S10-005 line 344 (CHECK constraint); WI-S10-001 line 211 implicitly via "5 tier × 30 region"; WI-S10-002 §6.1.16 line 448; WI-S10-004 §6.1.16 line 450; WI-S10-006 line 251; WI-S10-007 line 569.

**Evidence**: `data_model.md` line 68 establishes canonical 5 tiers as `free / solo / team / business / enterprise`. The same line *explicitly* says "Lote 10.7bis P0-7 fix — was 3 tiers; align com slo_catalog.md §3.1". `slo_catalog.md` lines 88-92 confirm: `free / solo / team / business / enterprise`. The S-10 system-reminder list "free/team/enterprise/custom/trial" is itself contaminated, but the ground-truth canonical source is `data_model.md`. Every WI that uses `'free', 'team', 'enterprise', 'custom', 'trial'` is wrong and will fail CHECK constraints once `subscription.plan_id REFERENCES plan(plan_id)` resolves against the actual `plan` table populated from `data_model.md`.

**Specific damage**:
- WI-S10-003 `CREATE TABLE plan (... CHECK (tier IN ('free', 'team', 'enterprise', 'custom', 'trial')))` (line 437) **rejects** any tenant on `solo` or `business`.
- WI-S10-005 `quota_state.plan_tier` CHECK (line 344) same.
- WI-S10-002 `usage_counter` rows are *not* tier-tagged in CHECK (good) but the §6.1.16 cardinality claim "5-tier canonical Plan reference" implicitly inherits the wrong list.

**Fix prescription**: Replace every occurrence of `('free', 'team', 'enterprise', 'custom', 'trial')` with `('free', 'solo', 'team', 'business', 'enterprise')`. Update WI-S10-003 §0 título, line 73 PlanTier enum (`Solo`, `Business` instead of `Custom`, `Trial`), line 113-124 enum variants, line 437 CHECK, line 547 design decision 9.10. Update WI-S10-005 line 344 CHECK. Update WI-S10-001/002/004/006/007 inheritance references. Add explicit cite to `data_model.md §1` (line 68) and `slo_catalog.md §3.1` (lines 88-92) in each WI's "5 PlanTier canonical" reference.

---

### P0-2 — Invariant-registry §3.X position uniformly claimed "TBD"; the positions are *known* (Lote 10.8bis P1-13 NOT absorbed)

**Location**: Every single WI. Grep `"§3.X canonical position TBD"`:
- WI-S10-001: lines 181, 186, 553, 577-578, 717
- WI-S10-002: lines 162, 167, 619, 644-645, 790
- WI-S10-003: lines 203, 209, 735, 760-761, 916
- WI-S10-004: lines 210, 216, 221, 644, 669-671, 823
- WI-S10-005: line 877
- WI-S10-006: lines 208, 688, 713, 864
- WI-S10-007: lines 237, 242, 614-618, 760

**Evidence**: `invariant_registry.md` lines 132-137 (§3.9 Billing) and lines 156-167 (§3.12 Sprint-driven Lote 9.4):
- INV-BILLING-NO-LOSS = §3.9 (line 136) — **HIGH** severity (NOT CRITICAL as every WI claims)
- INV-BILLING-NO-DUP = §3.9 (line 137) — **HIGH** severity (NOT CRITICAL)
- INV-BILLING-RECONCILE-3-LAYER = §3.12 (line 166) — HIGH, sprint origin S-10
- INV-BILLING-REPLAYABLE-FROM-EVENTS = §3.12 (line 167) — HIGH, sprint origin S-10
- INV-AUDIT-APPEND-ONLY = §3.6 (line 116) — CRITICAL ✓ (this WI claim is correct)

The Lote 10.8bis P1-13 lesson is "verify INV §3.X canonical position MUST be verified before commit; verify each WI's INV registry references against actual `invariant_registry.md` content." The WIs *cite* this lesson but each one explicitly leaves position as "TBD" — exactly the antipattern the lesson forbids.

**Fix prescription**: Replace "registry §3.X canonical position TBD" with concrete §3.9 / §3.12 position citations. Replace all "INV-BILLING-NO-LOSS (CRITICAL)" with "INV-BILLING-NO-LOSS (HIGH; registry §3.9 line 136)" and same for NO-DUP. Note that this severity correction will cascade into the SEV-1/SEV-2 alert math (since you can no longer claim CRITICAL invariant violation triggers automatic SEV-1; HIGH allows SEV-2 + Finance review).

---

### P0-3 — TLA+ state-space math wrong; two contradictory numbers in the same WI

**Location**: WI-S10-007 line 62-63 ("5 tenants × 5 SKUs × 24 hours × 5 regions = 15k states bounded") AND line 239, 297, 375, 581 ("5 tenants × 5 SKUs × 24 hours × 5 regions × 10 events/hour = ~150k states").

**Computed truth**:
- 5 × 5 × 24 × 5 = **3,000** (NOT 15k as line 62 claims)
- 5 × 5 × 24 × 5 × 10 = **30,000** (NOT 150k as lines 239/297/375 claim)

This is a 5× math error in both directions. State-space size matters because:
1. CI feasibility decision (refusal-merge threshold > 1M) depends on accurate baseline
2. "150k" is presented as comfortable headroom; actual 30k is *far* under threshold (fine, but the analysis is wrong)
3. The "≤ 30min em standard CI hardware" projection (line 371) is calibrated against 150k; with real 30k, it'll be much faster (good news but the spec is unreliable)

**Worse**: state space for TLC isn't `multiplicative-of-product` of CONSTANTS — it's the *reachable state graph* which depends on the action enabling structure. With 9+ actions and 14 VARIABLES, the *real* state graph could be `~10^6` to `~10^9` even with the bounded CONSTANTS shown. The naive `5×5×24×5` calculation is conceptually wrong — TLC explores reachable transitions, not Cartesian product of constants. The TLA+ LITERATE author should know this.

**Fix prescription**:
1. Reconcile both numbers. The correct *naïve* upper bound is 5×5×24×5×10 = 30,000 *for state variable assignments only*, but TLC's *reachable state graph* will be much larger because of `events_in_staging`, `events_in_r2`, `retry_queue` set/sequence variables.
2. Add a note that `events_in_staging`, `events_in_r2`, and `retry_queue` are unbounded variable cardinalities; introduce a `MaxConcurrentEvents` bound to prevent state-space explosion.
3. Re-compute under realistic bounds and validate via TLC dry-run before committing the WI.
4. The Hours = {0..23} is also conceptually wrong — TLA+ Hours should be the model's *time abstraction*, not the wall-clock hour-of-day; using {0..23} suggests states recur after 24 hours but billing periods span months.

---

### P0-4 — `CTRL-AUTHZ-005` is a hallucinated control ID

**Location**: WI-S10-005 line 202; WI-S10-005 line 891 sign-off matrix; WI-S10-006 line 218, 227, 246, 254, 273, 311, 439, 462, 563, 567, 718, 836, 845, 858, 879, 884, 897.

**Evidence**: `security_model.md` lines 243-244 (and surrounding) define **only** CTRL-AUTHZ-001 (Scope check by verb) and CTRL-AUTHZ-002 (Explicit tenant_id + assertion). There is **no** CTRL-AUTHZ-005 in the canonical security_model.md. WI-S10-006's entire role-protection narrative is anchored on a control that doesn't exist.

The sprint contract §5.6 R-S10-13 actually says "Replay endpoint protegido por role `billing_admin` (CTRL-AUTHZ-005)" — so the contract itself invents this CTRL ID. WI-S10-006 propagates the contract error into 18+ references and 1 design decision. WI-S10-005 line 202 propagates same.

**Fix prescription**:
1. Either (a) add CTRL-AUTHZ-005 to security_model.md §6 controls table with explicit definition: "Role-based access for sensitive ops (billing_admin, superadmin); MFA freshness; audit emission mandatory" — and update sprint contract + WIs consistently; or (b) replace CTRL-AUTHZ-005 references with CTRL-AUTHZ-001 + CTRL-AUTHZ-002 and explicit add-on "billing_admin role check + Mfa via S-03 RBAC inheritance".
2. Fix the sprint contract S-10 §5.6 R-S10-13 to match the chosen approach.

---

### P0-5 — UPSERT logic in WI-S10-002 has a self-defeating WHERE clause

**Location**: WI-S10-002 line 401-410, the atomic counter write SQL:
```sql
ON CONFLICT (tenant_id, sku, hour) DO UPDATE SET
  qty_bytes = excluded.qty_bytes,
  ...
WHERE usage_counter.own_digest = excluded.own_digest"  // idempotent: only UPDATE if digest matches
```

**Evidence**: The comment says "idempotent: only UPDATE if digest matches" — but if the digest already matches, the UPDATE is a no-op (same values). And if the digest *doesn't* match (e.g., late-arriving event added to the bucket changed the aggregate), the UPDATE *won't run*, leaving the stale row. This is the opposite of what the WI says it wants.

The WI also says (line 138-141) that re-aggregation should produce identical digest (deterministic) — true, but only if the input event set is identical. If the event set changes between cron fires (e.g., a late drain populates more events into the same hour bucket), the digest *will* differ and this WHERE clause silently rejects the UPDATE, causing INV-BILLING-NO-LOSS Layer-1 violation: events in R2 will exceed counter qty.

**Fix prescription**: Remove the `WHERE usage_counter.own_digest = excluded.own_digest` clause. Idempotency is already enforced by `(tenant_id, sku, hour)` PRIMARY KEY UNIQUE — re-aggregation produces same row identity; if digest differs, the *most recent* aggregate is correct (more events seen) and should overwrite. If you want digest-mismatch detection, do that with an *explicit* CounterDigestMismatch error path *before* the UPSERT, not a silent WHERE-fail.

---

### P0-6 — Cloudflare R2 Object Lock + bucket-lock Terraform resource names are unverified

**Location**: WI-S10-001 lines 293-318; WI-S10-004 lines 402-417.

**Evidence**: The WI uses `cloudflare_r2_bucket_lock_configuration` and `cloudflare_r2_bucket_lifecycle_rule` Terraform resources. Cloudflare R2 announced Object Lock GA in Feb 2025, but as of 2026-04 the canonical Terraform provider resource name for R2 Object Lock is `cloudflare_r2_bucket` with an `object_lock_configuration` nested block, NOT a separate `cloudflare_r2_bucket_lock_configuration` top-level resource. The WI may have copy-pasted from AWS S3 Terraform pattern (`aws_s3_bucket_object_lock_configuration`) without verifying CF's actual provider schema.

This is P0 because IaC apply will fail at `terraform plan` with "Resource type does not exist". Without verification of the actual cloudflare provider docs (per system-reminder: "R2 supports Object Lock + Lifecycle but NOT storage-class transitions"), this isn't compilable.

**Fix prescription**: Look up the current `cloudflare/cloudflare` Terraform provider docs (≥ 4.x) for R2 Object Lock; replace the resource declarations with the verified canonical form. Add a comment citing the provider version pinned in Cargo `infra/cloudflare/versions.tf`.

---

## P1 findings (significant — drift from canonical sources, lessons not absorbed)

### P1-1 — INV-BILLING-NO-LOSS / INV-BILLING-NO-DUP severity uniformly upgraded from HIGH to CRITICAL

**Location**: every WI's §1 invariants block, §6.1 hot-path discipline, §12 invariants validated section.

**Evidence**: `invariant_registry.md` line 136 says HIGH; line 137 says HIGH. WIs say CRITICAL.

The discrepancy isn't trivial: severity drives SEV-1 vs SEV-2 escalation, automatic invoice freeze rules, and PRR sign-off cap thresholds. CRITICAL invariants in the program have explicit TLA+ proof requirements (cf. invariant_registry.md §4.1 CI green). NO-LOSS/NO-DUP are listed in §4.2 PLANNED with TLA+ in S-10 — they *will* be CRITICAL-equivalent once `billing_atomicity.tla` is verified, but the registry hasn't promoted the severity yet.

**Fix prescription**: Choose one:
- (a) Promote both INVs to CRITICAL in `invariant_registry.md §3.9` *before* WIs commit (with ADR justifying the elevation) — this is the more honest path given S-10's HIGH_RISK lane forcing factors.
- (b) Lower WI claims to HIGH consistently and reword "SEV-1 alert" → "SEV-2 alert + Finance review" where appropriate.

WI-S10-007's §1 says "TLA+ formally proven" — this *is* the elevation justification. Combine option (a) with ADR-XXX referencing WI-S10-007 TLA+ verification.

---

### P1-2 — CTRL-PRIV-002 is mis-mapped to "pseudonymization for DSR" but actually means "Data classification tags"

**Location**: WI-S10-003 line 222, 259, 311, 312, 540, 766, 836, 853, 894; WI-S10-005 line 204, 212, 275, 297, 498, 727, 856, 869, 891; WI-S10-006 line 222, 297, 716, 758, 786, 803, 818, 836, 843, 856, 866, 884.

**Evidence**: `privacy_model.md` line 209: `CTRL-PRIV-002 | Data classification tags | Toda tabela/campo tem @classification=...`. This is a *labeling* control — every column gets a classification tag (PII, billing, etc.). It is **not** a pseudonymization mechanism. Pseudonymization belongs to a different control (S-11 sprint or DSR-specific control TBD).

**Fix prescription**: Replace CTRL-PRIV-002 references with either:
- "S-11 DSR pseudonymization procedure (cooperation TBD; CTRL-PRIV-002 data classification ensures audited fields are tagged)" — keeping classification distinct from pseudonymization, OR
- Add a new CTRL-PRIV-XXX in privacy_model.md explicitly for DSR pseudonymization and reference that ID.

---

### P1-3 — Stripe Idempotency-Key length 64-char internal limit is asserted but not justified against Stripe's actual 255-char limit

**Location**: WI-S10-003 line 178 (`IdempotencyKeyTooLong(usize)` error), line 366 (`assert!(s.len() <= 64, ...)`).

**Evidence**: Stripe API docs state Idempotency-Key MUST be ≤ 255 chars. The WI imposes a 64-char *internal* limit (1/4 of Stripe's max). The assertion message says "max 255 chars Stripe" which is the right Stripe limit, but the assertion fires at 64. The format is `corelink-{tenant_id_short}-{operation}-{event_hash_short}` = 9 + 8 + 1 + 8 + 1 + 8 = 35 chars (line 357 says "well under Stripe 255 limit") but assertion threshold 64 is also under 255 — fine, but the divergence between assertion limit and Stripe limit isn't documented as a deliberate safety margin.

This is P1 because: (a) future operation names longer than 8 chars may push the format past 64 (e.g., "subscription_cancel" = 19 chars), causing legitimate code to panic; (b) `tenant_id.short_form()` is "first 8 chars UUID" — but UUID has hyphens, so `short_form()` of `7f3a-bbb2-...` is `7f3a-bbb` (with hyphen) or `7f3abbb2` (without)? Specification ambiguous.

**Fix prescription**:
1. Document why 64 not 255: probably for log readability + URL-safety + DB column sizing. State this as ADR.
2. Specify canonical `short_form()` semantics: "first 8 hex chars of UUID, hyphens stripped".
3. Add proptest `prop_idempotency_key_format_under_64_chars` covering all 4 canonical operations.

---

### P1-4 — Cardinality math drift: WI-S10-001 "5 SKUs × 5 tier × 30 region = 750 séries" while WI-S10-002 says "5 regions canonical"

**Location**: WI-S10-001 line 213-214 ("5 SKUs × 5 tier × 30 region"), line 396 ("4 types × 30 regions = 120 séries"); WI-S10-002 line 232, 287 ("5 regions canonical: iad, fra, nrt, syd, gru").

**Evidence**: WI-S10-001 uses 30 regions in cardinality; every other S-10 WI uses 5 canonical regions. 30 may originate from S-04 region planning or be a stale draft number, but it's inconsistent.

**Fix prescription**: Update WI-S10-001 §1 line 213-214 + §6.1.12 line 396 to use 5 regions (matching WI-S10-002/004/006/007). Recompute: 4 types × 5 regions = 20 séries (NOT 120); 5 SKUs × 5 tier × 5 region = 125 séries (NOT 750). Total cardinality much smaller, well within INV-OBS-CARDINALITY-BUDGET.

---

### P1-5 — RB-FM-151 created without canonical FM→RB mapping update in failure_modes.md

**Location**: WI-S10-003 §6.1.9 line 533-536; WI-S10-004 line 488-489; WI-S10-007 line 388-405, 510, 631-633.

**Evidence**: `failure_modes.md` line 152 lists FM-151 (Stripe API outage) with mitigation "PAT-QUEUE-EVENTS-001 (retry)" — no RB. Line 274 lists `RB-FM-302 (billing leak) → mensal (reconciliation checks)` only. The 7 WIs *create* RB-FM-151 but the canonical failure_modes.md isn't updated to register the runbook.

**Fix prescription**: Add to failure_modes.md §X.X (next to RB-FM-302 line 274): `RB-FM-151 (Stripe outage; FM-151 mitigation) → semestral`. Update FM-151 row line 152 to add `+ RB-FM-151` after "PAT-QUEUE-EVENTS-001 (retry)". This is required for sprint promotion DoD.

---

### P1-6 — WI-S10-006 "3-of-3 sign-off (Finance + Legal + Architect)" not aligned to existing program sign-off matrix structure

**Location**: WI-S10-006 lines 234-237, 462-466, 671, 750, 858.

**Evidence**: The 12 sign-off cap (Lote 10.8bis P1-2) defines roles 1-12 (Owner / SRE / Security / Engineer×2 / QA / Product / Compliance / Privacy / Architect / Finance + Legal at sprint level). Architect is role 11. Finance is role 12. Legal is sprint-level not per-WI. The proposed "3-of-3 sign-off (Finance + Legal + Architect)" for `dry_run=false` requires Legal at the *per-WI / per-replay* level — contradicting the sprint-level Legal-only convention.

Also: the workflow uses CloudEvents `dev.hugr.corelink.billing.replay.signoff.v1` (line 463) but no sign-off-emitting Worker is specified. Who emits these events? Manual UI? CLI? It's a runtime mechanism that needs a defined Worker route or CLI tool.

**Fix prescription**:
1. Replace "Finance + Legal + Architect" with "Finance + Compliance Officer + Architect" (all per-WI roles in the 12-cap matrix); document Legal sign-off as the upstream DPA / Stripe contract review (sprint-level) which must be in place *before* dry_run=false is even attempted.
2. Specify the sign-off emit mechanism: probably an admin CLI `corelink billing-signoff --invoice X --role Architect` that authenticates via S-03 RBAC + emits the CloudEvent via WI-S09-004 audit chain. Add this to WI-S10-006 §6.1 and §13 Artifacts Produced.

---

### P1-7 — WI-S10-005 quota state machine has 5 enum variants but is called "4-state machine"; documentation drift

**Location**: WI-S10-005 line 28 título ("canonical 4-state machine"), line 100-112 enum (5 variants: Under80, SoftAlert, Ticket, HardBlock, GraceActive), line 248-253 narrative ("Why 4 states canonical").

**Evidence**: The WI consistently calls it a "4-state machine" but the QuotaState enum has 5 variants because GraceActive is added as a 5th. The narrative paragraphs (line 248-253) explicitly say "4 states canonical" then immediately note grace_active as a separate state with different semantics. CHECK constraint (line 343) accepts 5 values.

**Fix prescription**: Either (a) drop GraceActive as a top-level state and represent grace via a flag on quota_state (`grace_active BOOLEAN`) — keeping a true 4-state machine — and adjust transitions accordingly; or (b) rename to "5-state machine (including GraceActive)" and update all references including título, narrative, design decisions 9.1/9.2. Option (a) aligns with sprint contract §5.5 R-S10-10 which says "4 states".

---

### P1-8 — Layer-1 reconciliation drift formula collides with usage_counter_late accounting

**Location**: WI-S10-004 §6.1.3 lines 343-380 (Layer 1 reconciliation logic).

**Evidence**: Layer 1 computes `events_total = R2 events for target_date hour buckets`; `counter_total_combined = D1 counter + D1 counter_late`. Drift = (events − counter_combined) / events. But `usage_counter_late` is keyed by `original_event_ts` not by `target_date`, so a late-arriving event for hour H1 (originally) that arrives during hour H10 will be in `counter_late[H1]` *and* present in R2 (from the H1 hour bucket) — counted twice on the events side if R2 reads "all events with `evt.hour = target_date`", or counted zero times on counter side if `counter_late_aggregate_by_sku(region, target_date)` only matches `original_event_ts == target_date`.

The WI doesn't specify which: Layer 1 "uses target_date" but the late_aggregate function is left ambiguous. This produces a Layer-1 drift signal that's *artifact of the join semantics*, not a real accounting drift, leading to spurious SEV-2 alerts and false invoice freezes (Lote 10.6bis fail-CLOSED amplifies this).

**Fix prescription**: Clarify Layer 1 algebra. Probably correct: `events_for_hour H` should equal `counter_d1[H] + counter_late[H_original = H]` where the late counter is re-keyed against original_event_ts. State explicitly that late counter join is on `original_event_ts BETWEEN target_date AND target_date + 24h`. Add a Gherkin scenario covering "late event from yesterday arrives today; reconciliation Layer 1 still green for yesterday".

---

### P1-9 — `chrono::next_month_first_utc_midnight()` canonical primitive cited but its source/crate not specified

**Location**: WI-S10-002 line 213-214; WI-S10-003 line 261-263, 502; WI-S10-004 line 243-245; WI-S10-005 line 231-233, 501-502; WI-S10-006 line 259, 501; WI-S10-007 line 288-289.

**Evidence**: The primitive is cited 25+ times across the 7 WIs as a canonical `chrono::` function. The standard `chrono` crate (chrono 0.4.x) does **NOT** provide a `next_month_first_utc_midnight()` function. This is either (a) a hallucinated function, (b) a custom internal helper that the WIs forgot to point to, or (c) a typo/abbreviation.

**Fix prescription**: Either (a) implement it as a canonical helper in `crates/corelink-time/src/canonical.rs` and add to WI artifacts list; or (b) inline the actual chrono ops: `Utc::now().date_naive().with_day(1).unwrap().checked_add_months(Months::new(1)).unwrap().and_hms_opt(0,0,0).unwrap().and_utc()`. Whichever is chosen, add the **source path** (file + function signature) to the first-cited WI (WI-S10-002 design decision 9.13).

---

### P1-10 — WI-S10-005 does NOT have RB-BILLING-002 (late events triage) referenced despite sprint contract §15 R-005 risk mitigation cite

**Location**: Sprint contract S-10 line 242 mentions "runbook RB-BILLING-002 (late events triage)" as mitigation for late events > 6h causing silent revenue leak. WI-S10-002 and WI-S10-005 should both reference this RB.

**Evidence**: WI-S10-002 §6.1.11 (late-arriving events policy) doesn't reference RB-BILLING-002. WI-S10-005 doesn't either. WI-S10-006 references RB-BILLING-001 (replay procedure) but not RB-BILLING-002.

**Fix prescription**: Add RB-BILLING-002 (late events triage) to WI-S10-002 §13 Artifacts Produced; reference in §6.1.11. Decide which WI owns creation: WI-S10-002 is the natural home (it's the late-event router).

---

### P1-11 — Sprint contract §5.7 R-S10-15 metric names use **dots** (OpenTelemetry) but observability_model.md §4.1 mandates **underscores** (Prometheus)

**Location**: Sprint contract S-10 line 118: `corelink.billing.events_emitted_total{type, region}`, `corelink.billing.reconcile_drift_pct{layer, tenant_tier}`, etc.

**Evidence**: Lote 10.9bis P0-E lesson establishes Prom underscores canonical (NOT dots; dots = OpenTelemetry semantic conventions). Each WI correctly uses `corelink_billing_*` (underscores) — so WIs are right and the *sprint contract* itself drifted.

**Fix prescription**: Update sprint contract S-10 §5.7 R-S10-15 to use underscores: `corelink_billing_events_emitted_total{type, region}` etc. This is a sprint-contract-level fix, not per-WI, but should be tracked.

---

### P1-12 — Cardinality budget for `corelink_billing_reconcile_drift_pct{region, layer, sku, tenant_tier}` = 5×3×5×5 = 375 séries (single metric, fine) but baseline "~600 séries" claim is low

**Location**: WI-S10-004 §6.1.18 (line 459) and §10.s10.004.13 (line 660).

**Evidence**: Just `corelink_billing_reconcile_drift_pct` = 375. Plus 9 other metrics each adding 30-165 series. Sum is closer to 800-900 baseline, not 600. Within the 20k per-metric / 100k total INV-OBS-CARDINALITY-BUDGET — fine — but the baseline projection is lowballed.

**Fix prescription**: Recompute baseline cumulative cardinality precisely; update §10.s10.004.13.

---

## P2 findings (minor — wording, docs, cosmetic)

- **P2-1**: WI-S10-001 título is 1,000+ chars (line 28) — well past readable threshold; same anti-pattern in WI-S10-002/003/004/006/007. Cosmetic but reduces maintainability.
- **P2-2**: WI-S10-001 §22 line 657 "Cost saved by INV-BILLING-NO-LOSS: prevents revenue leak (potential millions undetected)" — qualitative claim without baseline; suggest "potential ~$X based on Y revenue at Z%-leak risk".
- **P2-3**: Each WI claims "100k nightly property test (HIGH_RISK SOTA bar; Lote 10.7bis P1-3)" but doesn't enumerate which CI workflow file runs them. Lote 10.7bis P1-3 lesson says the bar exists; the enforcement mechanism (CI yml file path) should be cited for traceability.
- **P2-4**: WI-S10-007 §6.1.13 line 438 "Property tests (TLA+ model checking is the primary; 0 TLA+-equivalent property tests)" — but earlier sections list property tests in §10.s10.007.x. Inconsistent — clarify whether WI-S10-007 has property tests or only model checking.
- **P2-5**: All WIs list "Engineer × 2" as roles 5-6 in sign-off matrix but the 12-cap convention is "Engineer (×2)" counted as 1 row in some matrices, 2 rows in others. Standardize per Lote 10.8bis P1-2 cap of 12.
- **P2-6**: WI-S10-006 line 87 `stripe_invoice_id: Option<StripeInvoiceId>` "override; default lookup via invoice_id" — but `ReplayRequest.invoice_id` is *internal* InvoiceId; mapping to StripeInvoiceId requires Neon lookup. Document this lookup is mandatory and how a developer overrides only when investigating Stripe-side state mismatch.
- **P2-7**: WI-S10-002 §22 (Cost Analysis) line 723 "Cloudflare Durable Objects: $0.15/1M requests + $12.50/GB-mo storage + duration." — DO pricing as of 2026 may differ; add as-of date or version-pin.
- **P2-8**: WI-S10-005 §1 enum `QuotaState` line 144 `GraceReason::ProductionIncidentMitigation` — service-attribution reason for grace seems out of scope vs the Lote 10.6bis canonical fail-OPEN at hot path discipline. Clarify.
- **P2-9**: Several WIs reference "Mfa" (capital M, lowercase fa) — should be "MFA" all-caps.
- **P2-10**: WI-S10-007 §10.s10.007.5 claims "TLA+ proves Layer-1 sub-property of INV-BILLING-RECONCILE-3-LAYER (drift ≤ 0.001)" but the spec only models reconciliation_layer_1_drift as a state variable without explicit transition rules that *bound* drift. The TLA+ Spec needs a `Reconcile` action that updates drift values *based on actual events ↔ counters comparison*; otherwise the invariant is trivially true (drift always 0 because it's never updated).

## Summary recommendations for the bis cycle

Priorities for fix-pass:

1. **P0-1** (Plan tier list) — touches 5+ WIs; do first; 1 hour fix once data_model is the source of truth.
2. **P0-2** (INV §3.X positions) — global s/§3.X canonical position TBD/§3.9 or §3.12 line N/; 30 min sed-style fix.
3. **P0-4** (CTRL-AUTHZ-005) — decide canonical: invent it formally in security_model.md *or* replace references. 1 hour.
4. **P0-5** (UPSERT WHERE clause) — single SQL fix in WI-S10-002. 15 min.
5. **P1-1** (severity HIGH→CRITICAL) — bundled with P0-2; either elevate registry or downgrade WIs.
6. **P0-3** (TLA+ math) — re-do state-space analysis seriously; possibly rewrite WI-S10-007 §1 TLA+ block. 4 hours.
7. **P0-6** (Cloudflare R2 Terraform) — verify provider docs; 1 hour.
8. **P1-2** (CTRL-PRIV-002) — fix mapping; 1 hour.
9. **P1-9** (chrono primitive source) — add canonical helper crate/module reference; 30 min.

Estimated total fix-pass: 12-15 hours of careful edit; not 3 bis cycles, but probably 2 (one for P0s, one for P1s).

The TLA+ specification in WI-S10-007 needs *expert review by someone who has actually run TLC* — the state-space math errors suggest the author hasn't validated the spec against a real TLC run.

---

**End audit.**
