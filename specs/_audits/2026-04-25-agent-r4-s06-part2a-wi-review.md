---
id: "AUDIT-2026-04-25-agent-r4-s06-part2a"
type: "audit_report"
doc_status: "FINAL"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-25"
reviewer: "agent-r4 (Claude Opus 4.7 1M)"
scope: "S-06 Part 2a — WI-S06-004, WI-S06-005 (focused 2 WIs)"
parent_audit: "specs/_audits/2026-04-25-agent-r4-s06-part1-wi-review.md"
sprint_contract: "specs/04_sprints/S06/_spec_contract.md v1.1.0"
tags: ["audit", "r4", "s06", "part2a", "wi-004", "wi-005", "adversarial"]
---

# S-06 Part 2a Adversarial WI Review — WI-004 (Physical Delete) + WI-005 (Refcount Reconcile)

> Round 4 adversarial. Operational-correctness focus. Cripto load-bearing already covered in Part 1.
> Reference SOTA bar: 9-10 (user directive: "Average não serve. SOTA puro 9-10.").
> Part 1 aggregate: 8.13/10. Part 2a target: meet or exceed; absorb P0-1 LIKE-vs-json_each lesson.

---

## 0. Executive Summary

WI-004 and WI-005 are the **operational closure** of the S-06 GC trio — the irreversible side (physical delete post-grace) and the integrity side (refcount reconcile + auto-fix). Both are HIGH_RISK with `FF-HR-011 + FF-HR-005`. Both are well-structured, have published Gherkin, chaos suites at the contract floor (10), and absorb most Lote 10.4bis/10.5bis lessons explicitly (alarm re-arm, R2-then-D1 atomic ordering, TenantCtx-only, audit fail-closed, partial UNIQUE state-scoped, ADR-0042 forward).

**Headline finding: WI-S06-005 §6.1 / §1 invariant 1 SQL aggregate inherits the WI-S06-003 P0-1 defect class — `blob_refs LIKE '%' || blob_meta.digest || '%'` against the JSON column.** The defect is **worse here than in WI-003** because reconcile is the *audit of last resort* for refcount integrity — a wrong aggregate produces false drifts, false auto-fixes, and (in the perverse case) **silently masks real drift** by inflating both stored and expected counts in correlated ways. This is P0 and Lote 10.6bis must absorb the json_each rewrite uniformly across WI-002, WI-003, WI-005.

**WI-004's strongest defect** is the `R2-then-D1 atomic` framing — the WI text says "atomic" but R2 DeleteObject is a *separate network call* from D1 batch and **cannot be atomic** in the ACID sense; the spec's recovery story (preserve D1 row on R2 fail; reconcile catches D1-orphan-on-R2-success) is correct, but the §1 wording "R2 DeleteObject + D1 row purge atomic" misleads readers into believing 2PC. P0-correctness; P1-clarity.

**Aggregate Part 2a score: 8.0/10** (slightly below Part 1's 8.13). Both GO-WITH-FIXES for Lote 10.6bis. Neither REJECT.

---

## 1. Per-WI Scores

### 1.1 WI-S06-004 — Physical Delete Worker — **8.1/10**

| Dimension | Score | Notes |
|---|---|---|
| Rigor | 8.0 | Strict-`<` boundary explicit; chaos #1 explicitly tests `deleted_at_ms = exact boundary` not deleted; PAT-RETRY-IDEMPOTENT-001 cited; phase budget arithmetic published (60ms × 100k / 8 = 12.5min). |
| Completeness | 8.0 | 10 chaos at floor; 5 prop tests; 7 metrics; DSR bypass scenario present but staging-stub admitted; missing checkpoint-resume idiom on chaos #10. |
| Clarity | 7.5 | "R2-then-D1 atomic" wording misleading (NOT 2PC); §6.1.6 D1 row purge is two DELETEs (`blob_meta` + `gc_candidates`) framed as atomic batch — consistent with sprint contract D1 batch ≤250, but row count discipline absent. |
| SOTA-adherence | 8.0 | 13 sign-offs; 10 chaos floor; PERT 31h; STRIDE delta absent (no §28 STRIDE+LINDDUN delta — same gap as Part 1 trio); cost TCO present ($913/yr); Mann-Whitney 3-prong absent (acceptable — no perf-regression-vs-baseline gate for irreversible delete). |
| Internal consistency | 8.5 | §6.1.9 bounded concurrency 8 + §1 invariant 3 R2-then-D1 + §28 risk register all reference same patterns; phase-budget §1 narrative #7 internally consistent with §6.1.13 chaos #7. |
| Prior-WI consistency | 8.5 | WI-S01-004 audit_outbox cited correctly; ADR-0042 forward (no new ADR needed); TenantCtx-only repeated; alarm re-arm Lote 10.4bis lesson absorbed §6.1.2; WI-S06-003 sweep state='swept' contract honored §6.1.3. |

**Overall**: solid spec, operationally complete, clarity loss on "atomic" misuse and DSR bypass auth depth.

### 1.2 WI-S06-005 — Refcount Reconcile + Auto-Fix — **7.9/10**

| Dimension | Score | Notes |
|---|---|---|
| Rigor | 7.0 | **P0**: SQL aggregate uses `LIKE '%' || blob_meta.digest || '%'` — same defect class as WI-003 P0-1. Auto-fix threshold 5 records is admitted-arbitrary (§9.3 "configurable env") — no rationale for 5 vs 10 vs %-of-total. |
| Completeness | 8.0 | 10 chaos at floor; 5 prop tests; 7 metrics + PagerDuty integration; SEV escalation 0.1% / 1% matches sprint contract §5.5 exactly; RB-FM-302 stub committed §10.s06.005.8. |
| Clarity | 8.5 | §1 published SQL inline (good practice — exposes the LIKE defect for review, which is *why* it must be fixed); §1 narrative #5 race-vs-concurrent-writes published reconcile_started_at_ms snapshot semantics. |
| SOTA-adherence | 8.0 | Sustained drift < 0.1% 7d staging cited §10.s06.005.3 (sprint contract DoD §10.s06.5 traced); STRIDE delta absent; Mann-Whitney 3-prong absent (acceptable — analytics workload not perf-regression gated). |
| Internal consistency | 8.0 | SEV None/Sev2/Sev1 enum matches §1 narrative #4 + chaos #1/#2/#3; auto-fix threshold 5 consistent §1 / §6.1.5 / §28 anti-pattern. |
| Prior-WI consistency | 8.0 | WI-S04-002 ac_meta column drift not re-checked (need verify `deleted_at_ms` IS NULL vs `deleted_at IS NULL` — §1 SQL line 105 uses `deleted_at_ms IS NULL`, but the WI title §0 row 39 uses `deleted_at IS NULL` — internal drift, P1). ADR-0042 forward correct; WI-S06-003 audit fail-closed pattern reused correctly. |

**Overall**: strong intent and SEV/PagerDuty story; held back **decisively** by the inherited LIKE defect (P0) and the threshold-5 arbitrary admission.

### 1.3 Aggregate Part 2a — **8.0/10**

Slightly below Part 1's 8.13 average. The single highest-leverage Lote 10.6bis action is the **json_each rewrite extending across WI-002, WI-003, AND WI-005** uniformly, plus the WI-004 "atomic" wording fix.

---

## 2. P0 Findings (must-fix Lote 10.6bis)

### P0-1 — WI-S06-005 §1 SQL aggregate inherits the WI-S06-003 LIKE-vs-json_each defect [LOAD-BEARING REFCOUNT]

**Severity**: P0. **WI**: WI-S06-005 §1 invariant 1 (lines 92-105), §6.1.3 (chunked iteration alludes to same query).

**Defect** (verbatim from §1):
```sql
SELECT
    blob_meta.tenant_id,
    blob_meta.digest,
    blob_meta.refcount AS stored_refcount,
    (SELECT COUNT(*) FROM ac_meta
     WHERE ac_meta.tenant_id = blob_meta.tenant_id
       AND ac_meta.blob_refs LIKE '%' || blob_meta.digest || '%'
       AND ac_meta.deleted_at_ms IS NULL) AS expected_refcount
FROM blob_meta
WHERE blob_meta.tenant_id = ?
  AND blob_meta.deleted_at_ms IS NULL;
```

**Why this is worse than WI-003's LIKE defect**:

1. **Reconcile is the audit of last resort.** WI-003 sweep with LIKE produces favorable-to-reachable false positives (no data loss, just `protected_re_ref` noise). WI-005 reconcile with LIKE produces **false drift signals in both directions**: (a) substring collisions inflate `expected_refcount` → false drift detected → false auto-fix `UPDATE blob_meta SET refcount = expected_refcount` → **the auto-fix corrupts the canonical refcount based on a wrong SQL aggregate**. The fail-closed audit chain captures the auto-fix decision but the decision itself is wrong.

2. **Performance pathology is severe.** The query is a correlated subquery: for each `blob_meta` row (per tenant, ~100k-1M scale), execute `SELECT COUNT(*) FROM ac_meta WHERE blob_refs LIKE '%digest%'` — a full scan of `ac_meta` per blob row. O(N_blob × N_ac) with leading-`%` LIKE. WI-005 §14 / §22 claims phase budget 1h p99 @ 1M blobs and per-reconcile-batch cost ≤ $0.000005. Both are **physically impossible** at this query shape on D1 at 1M blob scale with 100k+ ac_meta rows. The Cloudflare D1 query timeout (30s default) will trip well before phase budget; cost claim ignores the per-blob scan multiplier.

3. **Silent schema-evolution break** (same as WI-003 P0-1): if `blob_refs` JSON shape evolves to `{"refs": [...], "metadata": {...}}`, the LIKE silently matches metadata strings and reconcile produces silent drift forever.

4. **Auto-fix amplification.** The auto-fix is `UPDATE blob_meta SET refcount = expected_refcount`. If `expected_refcount` is wrong by +1 due to a substring collision in another field, the auto-fix *writes the wrong value into the canonical column*. Subsequent reconciles see consistency (because both stored and expected agree, because we just wrote the wrong value). **This converts a transient SQL semantic defect into a permanent canonical-state corruption.**

**Fix** (mandatory Lote 10.6bis):

```sql
SELECT
    blob_meta.tenant_id,
    blob_meta.digest,
    blob_meta.refcount AS stored_refcount,
    (SELECT COUNT(*)
     FROM ac_meta a, json_each(a.blob_refs) j
     WHERE a.tenant_id = blob_meta.tenant_id
       AND j.value = blob_meta.digest
       AND a.deleted_at_ms IS NULL) AS expected_refcount
FROM blob_meta
WHERE blob_meta.tenant_id = ?
  AND blob_meta.deleted_at_ms IS NULL;
```

**Additional asks**:
(a) Add property test `prop_reconcile_json_blob_refs_evolution` validating json_each survives schema evolution.
(b) Add chaos scenario "blob_refs JSON envelope evolution" — same as WI-003 P0-1 fix #4.
(c) **Verify `ac_meta.blob_refs` index exists** (WI-S04-002 §244 says full-scan acceptable as batch op; reconcile is a batch op, so full-scan is acceptable IFF chunked + bounded; verify `idx_ac_meta_tenant_deleted_at` covers tenant pre-filter).
(d) Re-derive the phase budget with json_each: per-blob extracts `O(json_array_size)` and joins; chunked iteration (e.g., 1k blobs/chunk) with bounded concurrency 4-8; re-publish realistic 1M-blob phase budget (likely 30-60min realistic, but on better algorithmic ground).
(e) **Re-derive the cost claim** ($0.000005 per reconcile-batch is incoherent — D1 reads at $0.001/M rows × correlated subquery scan = orders of magnitude higher).

**Impact**: highest-leverage Lote 10.6bis action for Part 2a. Without this fix, WI-005 ships a self-corrupting reconcile. WI-S06-006 TLA+ CI gate cannot validate INV-GC-003 against this query shape.

---

### P0-2 — WI-S06-004 §1 / §6.1 "R2-then-D1 atomic" wording misuse [CORRECTNESS-FRAMING]

**Severity**: P0 (correctness-framing; not implementation-correctness — the recovery semantics are correct, but the wording is dangerous for downstream implementers).

**Defect**: §1 invariant 3 says "Atomic R2-then-D1 ordering"; §0 title says "R2 DeleteObject + D1 row purge atomic"; §6.1.4 says "R2-then-D1 atomic ordering". **R2 DeleteObject is a separate network call from the D1 batch and cannot be ACID-atomic** with D1. The spec's actual semantics (verbatim §1 invariant 3):

```
- On R2 fail → preserve D1 row (no orphan ref); next hourly cron retries.
- On D1 fail post R2 success → R2 deleted but D1 row remains; next cron tick re-attempts D1 purge (idempotent: D1 row already gone? skip).
```

This is **eventually-consistent crash-recovery via idempotent retry**, not 2PC atomicity. The recovery semantics are correct. The wording is wrong and dangerous: a downstream implementer reading "atomic" may attempt to wrap R2 + D1 in a single transaction (impossible in Cloudflare Workers — R2 client is async network, D1 batch is internal RPC), or worse, build downstream code assuming the cross-system invariant.

**Concrete risk**: WI-S06-005 reconcile (P0-1 above) is *the recovery mechanism* for WI-S06-004's R2-success / D1-fail orphan case. If a future engineer reads "atomic" and assumes the orphan case is impossible, they may strip reconcile's orphan detection logic — and the orphan goes undetected forever.

**Fix** (Lote 10.6bis):
1. Rename §1 invariant 3 from "Atomic R2-then-D1 ordering" to "**Eventually-consistent R2→D1 ordering with idempotent crash-recovery (PAT-RETRY-IDEMPOTENT-001)**".
2. Re-title §0 to "R2 DeleteObject + D1 row purge **with idempotent crash-recovery**" (drop "atomic").
3. Add explicit §1 paragraph: "R2 + D1 are **not** transactionally atomic; cross-system consistency relies on (a) R2-first ordering ensures D1 never references missing R2 *during the visibility window* and (b) WI-S06-005 reconcile detects the R2-success/D1-fail orphan case within 24h."
4. Cross-reference WI-S06-005 reconcile as the recovery path explicitly in §6.1.4.

**Impact**: clarity-correctness. No code change. Single-page edit. But the wording is load-bearing for downstream implementer assumptions.

---

### P0-3 — WI-S06-004 §6.1.8 / §1 invariant 4 DSR erasure bypass S-11 forward auth verification ambiguous [SECURITY]

**Severity**: P0. **WI**: WI-S06-004 §1 invariant 4 ("DSR signal verified pre-bypass; S-11 forward auth"); §6.1.8 ("DSR erasure bypass signal handler (S-11 forward; staging stub OK)"); §7 anti-scope ("DSR bypass without signal verification"); §28 risk register row 5 "DSR bypass timing race L M MEDIUM".

**Defect**: the WI repeatedly cites "S-11 forward auth" but **never specifies what the auth verification is**. §6.1.8 admits "staging stub OK". The Gherkin §8 scenario "DSR erasure bypass immediate" says only "Given DSR signal received for tenant T digest D (S-11 forward stub)". Anti-scope §7 says "DSR bypass without signal verification" but does not state what verification gates the bypass.

This is a **bypass-of-grace-period authorization** — the most security-sensitive control surface in the WI (the only path that turns the irreversible delete from a 72h grace-protected operation into an immediate-delete operation). DSR signals must be:

(a) **Authenticated**: signed by a known DPO authority (not just a queue write).
(b) **Authorized**: limited to specific tenant + digest scope (not global wildcard).
(c) **Audited pre-execution**: emit `corelink.gc.physical_delete.dsr_bypass_received` with full provenance BEFORE bypass executes, so a malicious bypass leaves a forensic trail even if it succeeds.
(d) **Replay-protected**: DSR signal IDs must be unique + persisted to prevent re-execution (the spec acknowledges replay attack §2 "DSR erasure bypass replay attack" but resolves it as "idempotent (already deleted)" — that's correct for the second-call no-op, but not for a *new* digest replay where the DSR record is forged from a previous valid signal).

**Concrete attack**: insider with write access to the DSR signal queue (S-11 not-yet-implemented) crafts a signal `{tenant: victim, digest: blob_X}` where blob_X is reachable. Without auth verification, WI-004 bypass-immediate-deletes blob_X. Audit chain captures the bypass but the data is gone.

**Fix** (Lote 10.6bis):
1. Add §1 invariant 4 sub-points explicitly:
   - (a) DSR signal must carry `signed_payload` with DPO authority signature; verify via Ed25519 or HMAC-with-rotated-key against known DPO pubkey set.
   - (b) DSR signal scope must be `(tenant_id, digest)` pair; reject wildcard/all-digest.
   - (c) Pre-execution audit emit `corelink.gc.physical_delete.dsr_bypass_received` BEFORE bypass.
   - (d) DSR signal ID persisted (UNIQUE constraint on `dsr_signal_processed.signal_id`); replay rejected.
2. Add anti-scope: "❌ DSR signal without DPO signature; ❌ DSR signal wildcard scope".
3. Add Gherkin scenario: "DSR signal with invalid DPO signature → REJECTED + audit emit `dsr_bypass_invalid_signature` + SEV-1 alert".
4. Add chaos scenario: "DSR signal forged (invalid signature) → bypass refused; audit captures attempt".
5. **Crypto SME advisory promoted to MANDATORY** for DSR signal verification path (currently advisory per §30 sign-off row 13).

**Impact**: this is the only path in the GC trio that bypasses the irreversible-delete safety net. Auth depth must be specified pre-implementation, not deferred to S-11.

---

### P0-4 — WI-S06-004 §6.1.13 chaos #9 "customer re-upload race post-grace boundary" mis-frames the race [CORRECTNESS]

**Severity**: P0. **WI**: WI-S06-004 §6.1.13 chaos #9; §8 Gherkin "Customer re-upload race post-grace".

**Defect**: chaos #9 says "physical-delete fires for digest D em region_sam at T; customer CAS write handler INSERTs same digest D at T+10ms; CAS write handler S-01 (Lote 10.1 pattern reused) creates fresh blob_meta row; gc_candidates fresh row (status='candidate' next mark cycle); no race interference (atomic per-row D1 ops)". This is **partially wrong**:

(a) **Physical delete is post-grace** (`deleted_at_ms < now - 72h`), so the blob has been soft-deleted ≥72h ago. A "customer re-upload" at this point is a *fresh INSERT* of the digest, not a "re-reference" of the previous incarnation. The CAS write handler S-01 contract (per Lote 10.1) is *content-addressed* — same digest, same content, INSERT-OR-IGNORE semantics. So the race is *not* "fresh blob_meta row"; it's "INSERT-OR-IGNORE returns existing row OR inserts new".

(b) **The race that actually matters** is: physical-delete fires at T (R2 DeleteObject succeeds at T+5ms; D1 batch begins at T+10ms); customer CAS write fires at T+7ms (R2 PutObject succeeds at T+12ms; D1 INSERT begins at T+15ms). At T+15ms, D1 sees `blob_meta` row still present (physical-delete D1 batch not yet committed at T+15ms < T+10ms+batch_latency). Customer CAS INSERT-OR-IGNORE returns existing row. Customer thinks blob is cached. But physical-delete D1 batch commits at T+25ms — **the customer-cached pointer is now dangling** (blob_meta row deleted; R2 deleted; customer's ac_meta now references a non-existent blob).

(c) WI-001 CAS write handler must also detect this race — but WI-001 is in S-01, not S-06. The §6.1.13 chaos #9 frames the race as resolved by "atomic per-row D1 ops", but the cross-row + cross-system race is **not** resolved by per-row atomicity.

**Fix** (Lote 10.6bis):
1. Rewrite chaos #9 to frame the race correctly: "physical-delete D1 commit T+25ms; customer CAS write INSERT-OR-IGNORE D1 commit T+15ms (sees stale row); customer ends with ac_meta row pointing at dangling blob_meta row".
2. Specify resolution: physical-delete must use **conditional D1 batch** that re-checks blob_meta row state at commit time (e.g., `DELETE FROM blob_meta WHERE digest = ? AND refcount = 0 AND deleted_at_ms < (now - grace_period_ms)`) — the `refcount = 0` predicate fails if customer's CAS write incremented refcount in the race window.
3. Cross-reference: WI-S06-005 reconcile catches the orphan if the conditional DELETE *succeeds* but customer's INSERT was already committed (hard-to-reach race; reconcile is the safety net).
4. Add property test `prop_physical_delete_re_upload_race` racing 1k physical-delete vs CAS write threads; assert no dangling ac_meta.

**Impact**: this race is the operational equivalent of the WI-003 INV-GC-004 race (mark-vs-UpdateAR), but in the post-grace physical-delete window. The current chaos #9 framing under-specifies the resolution.

---

### P0-5 — WI-S06-004 §1 narrative #7 phase budget 30 min @ 100k arithmetic does not account for D1 batch ≤250 row Lote 10.5bis lesson [CAPACITY]

**Severity**: P0. **WI**: WI-S06-004 §1 narrative #7; §6.1.6 D1 row purge (atomic batch); §3 SLA "Phase budget 30 min p99 @ 100k candidates"; §14.s06.004.4 "phase ≤ 30 min p99 @ 100k".

**Defect**: §1 narrative #7 derivation: `R2 DeleteObject ~50ms each + D1 purge ~10ms = 60ms × 100k = 100min linear; parallelize 8 concurrent → ~12.5 min`. This arithmetic **ignores the D1 batch ≤250 row Lote 10.5bis constraint** (sprint contract: D1 batch ≤250 rows). Each physical-delete is a D1 batch of (`DELETE blob_meta` + `DELETE gc_candidate` + `INSERT audit_outbox`) = 3 rows per candidate. At 250 rows per batch, that's ~83 candidates per batch. At 100k candidates, that's ~1200 D1 batches.

**Realistic arithmetic**:
- Per-candidate: R2 DeleteObject 50ms (per-call; bounded concurrency 8 = ~6.25ms amortized).
- Per-candidate: D1 batch share at 83-candidates-per-batch = D1_batch_latency / 83. D1 batch p99 ~50-100ms (per WI-S04-005 envelope), so per-candidate D1 share = ~0.6-1.2ms.
- Audit outbox: Lote 10.5bis lesson says audit emit is part of same D1 batch (fail-closed pattern), so already counted.
- **Total per-candidate amortized**: ~7.5ms (not 60ms). 100k × 7.5ms = 750s = 12.5min. ← **Coincidentally lands at the same number as the WI claims, but for completely different reasons.**

The WI's arithmetic is wrong in its components but right in its conclusion. **This is a fragile coincidence.** A future change to bounded concurrency from 8 to 4 (e.g., R2 rate limit tightens) breaks the conclusion silently.

**Fix** (Lote 10.6bis):
1. Re-publish phase budget arithmetic using D1 batch ≤250 explicitly: `100k candidates / 83 candidates-per-batch = ~1200 batches; 1200 batches × D1_batch_p99(100ms) / bounded_concurrency(8) = 15s D1 work; R2 DeleteObject 100k × 50ms / 8 = 625s ≈ 10.4min; total ≈ 11min p99 @ 100k`.
2. Add explicit budget headroom: 30min p99 budget vs 11min derived = 2.7× headroom; SEV-2 alert at 25min (sustained).
3. Add chaos scenario: "D1 batch p99 latency degrades to 500ms (5×) → phase budget exceeded at 100k → SEV-2 alert".
4. Cross-reference Lote 10.5bis D1 batch ≤250 lesson explicitly in §1.

**Impact**: the budget claim is right but the derivation is wrong; future change-impact analysis will be silently incorrect. Single-page edit.

---

### P0-6 — WI-S06-005 §6.1.5 auto-fix threshold "≤5 records" arbitrary; rationale absent; failure mode under-specified [SAFETY]

**Severity**: P0. **WI**: WI-S06-005 §1 invariant 3, §6.1.5, §9.3 ("Auto-fix threshold 5 records (configurable env)"), §28 risk register row 2.

**Defect**: 5 records is admitted-arbitrary in §9.3. The threshold gates an **auto-correcting UPDATE on the canonical refcount column**. Wrong threshold = either too-aggressive auto-fix masks systemic bugs (raise threshold) OR too-conservative auto-fix floods PagerDuty (lower threshold).

The WI has no:
- Rationale for 5 vs 10 vs 1 vs %-of-total.
- Specification of what happens if reconcile detects EXACTLY 5 drifts (boundary): is it ≤5 (auto-fix) or <5 (auto-fix; ≥5 manual)? §1 invariant 3 says "≤ 5 records: auto-fix" — `≤5` is the operative bound.
- Specification of failure mode: if auto-fix UPDATE fails (D1 throttle), is the drift counted as detected-but-not-fixed? Re-attempted next cron? Escalated to SEV-1?
- Tenant-size sensitivity: a tenant with 10 blobs and 5 drifts has 50% drift; a tenant with 100k blobs and 5 drifts has 0.005% drift. Same threshold absurdly mis-fires across scale.

**Fix** (Lote 10.6bis):
1. Rationalize threshold as **percentage-floor + absolute-floor**: e.g., `auto_fix IF (drift_count ≤ 5 AND drift_percent ≤ 0.01%)`; `manual_review IF (drift_count > 5 OR drift_percent > 0.01%)`. This is scale-invariant.
2. Add explicit boundary clarification: `≤5` is operative bound; `=5` auto-fixed; `=6` manual review.
3. Specify auto-fix failure mode:
   - D1 throttle → exponential backoff retry (3 attempts) → on persistent fail, downgrade to "drift detected, fix pending"; emit SEV-2; do **not** escalate to SEV-1 (auto-fix failure ≠ refcount integrity broken).
   - On 3-attempt persistent fail, persist drift in `gc_drift_pending` table; next reconcile cron re-attempts.
4. Add chaos scenario: "auto-fix UPDATE fails with D1 throttle → retry exhausted → drift_pending persisted → next cron re-fixes".
5. Add property test `prop_auto_fix_threshold_scale_invariant` validating percentage+absolute floor at 10, 1k, 100k tenant sizes.

**Impact**: auto-fix is the dangerous part of WI-005 (canonical-state mutation). Threshold rationalization is non-negotiable for SOTA bar.

---

### P0-7 — WI-S06-005 chaos suite at floor 10; missing key adversarial scenarios [COMPLETENESS]

**Severity**: P0. **WI**: WI-S06-005 §6.1.10 chaos suite (10 scenarios listed).

**Defect**: 10 scenarios is the sprint contract HIGH_RISK floor, not the SOTA bar. WI-S04-003 (8.6 ceiling) ships >10. The 10 listed cover happy-path SEV transitions but miss:

- **Auto-fix race with concurrent UpdateAR**: reconcile detects drift D=3; auto-fix UPDATE refcount fires; concurrent UpdateAR fires +1; final state refcount = expected_refcount (auto-fix) but UpdateAR's +1 lost OR refcount = expected_refcount + 1 (UpdateAR after auto-fix) → next reconcile sees +1 drift → ping-pong.
- **Reconcile vs physical-delete race**: WI-004 physical-delete fires mid-reconcile for tenant T digest D; reconcile snapshot at `reconcile_started_at_ms = T_start`; physical-delete commits at T_start+5min; reconcile aggregate at T_start+10min sees blob_meta row gone but ac_meta still present → expected_refcount > 0, stored_refcount = ??? (row deleted) → query returns NULL → drift detection logic must handle NULL (NULL ≠ 0 in SQL three-valued logic).
- **Cross-region drift reconciliation**: 5 regions, per-region reconcile; do per-region drifts aggregate to global drift correctly? §1 narrative claims "Global drift % = drifts_detected / blobs_scanned" — across 5 regions running independently, this is per-region not global.
- **DSR-bypass impact on reconcile**: WI-004 DSR bypass deletes blob; ac_meta still references digest; reconcile sees drift (stored_refcount = NULL because blob_meta row gone; expected_refcount > 0). Is this a real drift or expected behavior? If real, auto-fix mis-fires.
- **Refcount manipulation attack chaos #9**: "customer admin manipulates refcount" — but the threat model needs to specify *how* (admin endpoint S-13? D1 direct? sqlx prepared blocks injection but admin endpoints might allow legitimate refcount UPDATE). Specify the threat surface.

**Fix** (Lote 10.6bis):
1. Expand chaos to ≥13 (matching the SOTA-bar of Part 1 trio's WI-S06-003).
2. Add 5 scenarios above explicitly.
3. Add Mann-Whitney 3-prong absent — acceptable (analytics workload, not perf-regression-gated), but note the absence in §15 with justification.

**Impact**: at floor-of-10, WI-005 is contract-compliant but below SOTA. The auto-fix-vs-UpdateAR race (chaos #11 above) is the most operationally dangerous missing scenario.

---

## 3. P1 Findings

### P1-1 — WI-S06-005 internal column-name drift `deleted_at` vs `deleted_at_ms`

§0 row 39 title says `count(ac_meta where blob_refs contains digest AND deleted_at IS NULL)`; §1 SQL line 105 uses `ac_meta.deleted_at_ms IS NULL`. WI-S04-002 canonical column is `deleted_at_ms` (`_ms` suffix). Update §0 title to match. Same defect class as Part 1 P0-2 (column-name drift).

### P1-2 — WI-S06-004 chaos count at floor 10; matches Part 1 trio gap

Same as P0-7 above scaled down: 10 is contract floor, SOTA bar is 13+. Add scenarios:
- Cron tick missed (CF Workers DO alarm not fired) → next tick double-load 200k candidates → phase budget exceeded.
- bytes_reclaimed metric drift (sum mismatch vs blob.size_bytes ground truth at reconcile time).
- Cron clock skew across regions (5 regions, NTP drift 1s) → grace boundary ambiguous at 1ms-from-boundary.

### P1-3 — WI-S06-004 §6.1.7 bytes_reclaimed tracking lacks tenant-aggregation atomicity

`bytes_reclaimed_total{tenant_id, region}` is per-(tenant,region). Cross-region rollup to per-tenant total is **not specified** in WI-004 (deferred to S-09 forward dashboard?). If two regions both reclaim same tenant blob (impossible by partition, but specify), the rollup is wrong. Add explicit "per-region partition; cross-region rollup is sum-aggregate at observability layer (S-09)".

### P1-4 — WI-S06-004 §22 / WI-S06-005 §22 cost TCO arithmetic identical at $913/yr — copy-paste tell

Both WIs claim TCO 12m ≈ $913/yr with identical formula `5 × 100 × 1k × 365 × $0.000005`. WI-005 reconcile at this cost is incoherent given P0-1 fix (correlated subquery × 1M blobs). Re-derive both costs with correct arithmetic post-P0-1 fix.

### P1-5 — WI-S06-005 §6.1.2 "cron 03:00 UTC + jitter ±10min" overlaps with WI-S06-001 mark/sweep cron 02:00

Sprint contract §5.x specifies daily mark cron; this WI says "after mark/sweep cron 02:00" implicitly. Add explicit dependency: reconcile cron MUST start ≥30min after mark/sweep latest completion to avoid snapshot inconsistency. Spell out the temporal contract.

### P1-6 — WI-S06-005 §1 narrative #5 race-vs-concurrent-writes resolves to "minor drift acceptable (window < 1s)"

The 1s window is asserted, not derived. With UpdateAR p99 latency ~10ms and 1000 concurrent writers, the inflight window is effectively unbounded for high-throughput tenants. Either (a) bound the window via reconcile_started_at_ms snapshot used as `created_at < snapshot` predicate in the EXISTS check (mirroring WI-S06-003 mark_started_at_ms pattern), or (b) accept higher drift tolerance and adjust SEV thresholds.

### P1-7 — WI-S06-004 §28 risk register 10-row at floor; SOTA bar is 12+

Same defect class as Part 1 trio. Add: DSR bypass auth defect L L CRITICAL L LOW; reconcile-orphan-undetected M L MEDIUM L LOW (depends on WI-005 ship).

---

## 4. P2 Findings

- WI-004 §6.1.11 "7 metrics emitted" is correct count (8 listed); minor.
- WI-005 §17 sub-task list omits ID prefixes (`ST-001 module skeleton | 1` lacks `ST-001` label structure consistent with WI-004).
- WI-005 §31 Change Log says "1.0.0 / 2026-04-25 / Gustavo (Lote 10.6)" — should be Lote 10.6 *or* Lote 10.6bis post-fix; note will need bump on Lote 10.6bis.
- WI-004 §32 anti-patterns list duplicates §7; consolidate.
- WI-004 §3 persona 3 "DPO (DSR erasure)" is the only WI with DPO persona; cross-reference S-11 for full DPO journey.
- Both WIs lack STRIDE+LINDDUN delta in §26 (Part 1 trio has same gap).
- WI-005 §10.s06.005.5 "CRITICAL alert global_drift > 1%" — sprint contract §5.5 says SEV-1 is per-tenant >1%, not global >1%. Reconcile alert thresholds need re-validation against sprint contract.

---

## 5. Cross-WI Consistency Check

### 5.1 Inheritance from WI-001..003 + Lote 10.4bis/10.5bis

| Pattern | WI-004 | WI-005 |
|---|---|---|
| TenantCtx-only (Lote 10.4bis) | YES §1.5, §7 | YES §1.4, §7 |
| Audit fail-closed (WI-S06-003) | YES §1.6, §6.1.10 | YES §1.5, §6.1.7 |
| Alarm re-arm at start (Lote 10.4bis) | YES §6.1.2 | implicit (cron DO §6.1.2) |
| ADR canonical path | YES (ADR-0042 forward) | YES (ADR-0042 forward) |
| audit_outbox = WI-S01-004 | YES | YES |
| D1 batch ≤250 (Lote 10.5bis) | **MISSING explicit reference** (P0-5) | **MISSING explicit reference** |
| Partial UNIQUE WHERE state | N/A (no new uniqueness constraints) | N/A |
| CHECK inline (Lote 10.5bis) | N/A (no new tables) | N/A |
| INV §3.17 promoted | YES (4 NEW INVs) | YES (2 NEW INVs) |
| validate_inv_promotion CI gate | YES §10 | YES §10 |
| Crypto SME mandatory | NO (advisory §30 row 13) — **P0-3 says promote MANDATORY for DSR** | NO (advisory §30) — acceptable (no cripto path) |
| TLA+ alignment | implicit (post-grace = post-DeleteCASBlob trigger) | YES INV-GC-003 |
| 13-row sign-off | YES §30 | YES §30 |
| Cost TCO | YES §22 ($913/yr) — **P1-4 says re-derive** | YES §22 ($913/yr) — **P1-4 says re-derive** |
| STRIDE+LINDDUN delta | NO | NO |
| Mann-Whitney 3-prong | NO (irreversible; n/a) | NO (analytics; n/a) |

**Consistency assessment**: WI-004 + WI-005 inherit the major Lote 10.4bis/10.5bis lessons that apply to their domain. The Lote 10.5bis D1 batch ≤250 lesson is **silently absorbed** but never explicitly cited — this is a P0-5 finding for WI-004 (phase budget arithmetic broken without it) and a P1 for WI-005 (chunked iteration mentions "bounded" but not 250).

### 5.2 Cross-WI invariant chain (WI-001 → WI-002 → WI-003 → WI-004 → WI-005)

WI-001 orchestration → WI-002 mark phase → WI-003 sweep phase → **WI-004 physical-delete (irreversible)** → **WI-005 reconcile (audit-of-last-resort)**.

The chain is operationally complete. Key cross-WI dependencies validated:
- WI-004 consumes `gc_candidates` status='swept' (produced by WI-003 §6.1.x) — present.
- WI-005 verifies `blob_meta.refcount` consistency (denormalized counter maintained by S-01 CAS write + WI-S04-005 UpdateAR + S-04 DeleteAR + WI-003 sweep) — present.
- WI-005 reconcile is the recovery path for WI-004 R2-success/D1-fail orphan (P0-2 framing fix) — implicit; P0-2 fix makes explicit.
- WI-005 reconcile drift detection fires if WI-002 mark / WI-003 sweep miscounts refcount — present implicitly.

**Gap**: no explicit cross-WI invariant cataloging chain handoffs. This is a Lote 10.6bis nice-to-have, not P0.

---

## 6. Verdict per WI

### 6.1 WI-S06-004 — GO-WITH-FIXES (Lote 10.6bis)

**Score**: 8.1/10. Operationally strong. Held back by:
- P0-2 "atomic" wording misuse (1-page edit; high impact for downstream implementer assumptions).
- P0-3 DSR bypass auth depth ambiguous (security-load-bearing; specify pre-impl).
- P0-4 chaos #9 re-upload race mis-framed (under-specified resolution).
- P0-5 phase budget arithmetic ignores D1 batch ≤250 (right answer wrong derivation).
- P1-2/P1-3/P1-7 chaos count, bytes_reclaimed cross-region, risk register at floor.

### 6.2 WI-S06-005 — GO-WITH-FIXES (Lote 10.6bis)

**Score**: 7.9/10. Solid SEV/PagerDuty story. Held back **decisively** by:
- **P0-1 SQL aggregate LIKE-vs-json_each defect** (highest-leverage Lote 10.6bis fix; same as Part 1 P0-1 inherited; 1-line SQL rewrite + property test).
- P0-6 auto-fix threshold 5 arbitrary, scale-blind, failure mode under-specified.
- P0-7 chaos at floor (10) with 5 missing adversarial scenarios.
- P1-1 column drift; P1-4 cost re-derive; P1-5 cron timing; P1-6 race window 1s asserted.

### 6.3 Aggregate Part 2a Score — **8.0/10**

Slightly below Part 1 (8.13). The drag is WI-005's inherited LIKE defect (P0-1) — the single most-leverage fix in the Part 1 audit cascading to Part 2a. Once P0-1 + P0-2 + P0-3 + P0-6 are absorbed in Lote 10.6bis, both WIs cross 8.5/10.

---

## 7. Lote 10.6bis P0 Fix Plan (Part 2a Action Items)

| Priority | WI | Defect | Fix | Effort | Owner |
|---|---|---|---|---|---|
| **P0-1** | WI-005 | SQL aggregate `LIKE '%digest%'` | Rewrite to `json_each(a.blob_refs) j WHERE j.value = digest`; add prop_reconcile_json_evolution; verify `idx_ac_meta_tenant_deleted_at`; re-derive phase budget; re-derive cost. | 2h spec + 1h prop test design | Gustavo + Crypto SME advisory |
| **P0-2** | WI-004 | "Atomic R2-then-D1" wording misuse | Rename to "Eventually-consistent R2→D1 with idempotent crash-recovery"; cross-ref WI-005 reconcile. | 30min spec edit | Gustavo |
| **P0-3** | WI-004 | DSR bypass auth depth ambiguous | Specify Ed25519/HMAC signature; tenant+digest scope; pre-exec audit; UNIQUE signal_id replay protection; promote Crypto SME MANDATORY. | 1.5h spec + 1h sec-review | Gustavo + Security Lead + Crypto SME |
| **P0-4** | WI-004 | Chaos #9 re-upload race mis-framed | Rewrite race scenario; specify conditional D1 batch with `refcount = 0` predicate; add prop_physical_delete_re_upload_race; cross-ref WI-005 reconcile recovery. | 1h spec + 1h prop test | Gustavo |
| **P0-5** | WI-004 | Phase budget arithmetic ignores D1 batch ≤250 | Re-publish with batches/8-concurrency derivation; add chaos D1 batch p99 5× degrade. | 30min spec | Gustavo |
| **P0-6** | WI-005 | Auto-fix threshold 5 arbitrary; scale-blind | Rationalize as percentage-floor + absolute-floor (`≤5 AND ≤0.01%`); specify auto-fix failure mode (3-attempt backoff → drift_pending); add prop_auto_fix_scale_invariant. | 1.5h spec + 1h prop test | Gustavo + SRE |
| **P0-7** | WI-005 | Chaos at floor 10 | Add 5 scenarios: auto-fix-vs-UpdateAR race, reconcile-vs-physical-delete race, cross-region drift aggregation, DSR-bypass-impact-on-reconcile, refcount-manipulation threat surface. | 2h chaos suite design | Gustavo + AppSec |

**Total Lote 10.6bis effort Part 2a**: ~13h spec + review iteration. Well within the 38h time-box hard limit per WI.

---

## 8. Sign-off

**Reviewer**: agent-r4 (Claude Opus 4.7 1M context).
**Verdict**: Both WIs **GO-WITH-FIXES** for Lote 10.6bis. Neither REJECT. Aggregate Part 2a score 8.0/10 (target: meet Part 1's 8.13 post-fixes).
**Blocking conditions for Lote 10.6bis seal**: P0-1 (LIKE→json_each), P0-3 (DSR auth depth), P0-6 (auto-fix threshold rationalization). The other P0s are 30min-1.5h edits each.
**Recommend**: Crypto SME advisory promoted to MANDATORY for WI-004 DSR signal verification path (P0-3). WI-005 keeps Crypto SME advisory (no cripto-load-bearing path).

**Next**: Part 2b (WI-006 TLA+ CI gate + WI-007 DASH-GC dashboard) when those specs land.

---

**End audit Part 2a.**
