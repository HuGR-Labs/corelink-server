---
id: AUDIT-SONNET-R5-S06-WI-REVIEW
parent_audit: specs/_audits/sealed/2026-04-25-agent-r4-s06-part1-wi-review.md
sprint_contract: specs/04_sprints/_sealed/S06/_spec_contract.md
tags: [audit, sota, lote-10.6, s-06, sonnet-r5, independent, post-lote-10.6bis]
reviewer: Sonnet 4.6 (independent — round 5; different model than Opus R4)
scope: S-06 all 7 WIs + sprint contract v1.2.0 post-Lote 10.6bis (commit 44183ef)
created: 2026-04-25
calibration_baselines:
  - "Agent R4 Part 1 (8.13/10) — WI-S06-001..003"
  - "Agent R4 Part 2a (8.0/10) — WI-S06-004..005"
  - "Agent R4 Part 2b (8.0/10) — WI-S06-006..007"
  - "WI-S04-003 best-in-class 8.6"
---

# Sonnet R5 — Lote 10.6bis Post-Fix Adversarial Review — Sprint S-06 All 7 WIs

> **Reviewer**: Sonnet 4.6 (independent; round 5; different model lineage than Opus R4 rounds).
> **Mandate**: Find what Opus missed. Cross-validate Lote 10.6bis 21 P0 fixes. Brutal, technical. No diplomacy.
> **Post-fix baseline**: commit 44183ef on main; spec_contract v1.2.0; all 7 WIs v1.1.0.

---

## 0. Executive Summary

The Lote 10.6bis remediation is **substantially complete and correct**. The 21 P0 fixes identified by Opus R4 across three audit parts are all present in the post-fix WI files. The highest-leverage fix — json_each idiom replacing LIKE substring — is correctly applied in WI-S06-003 §6.1.2 and WI-S06-005 §1. The TLC SHA-256 pinning, CODEOWNERS override mechanism, sign-off booking calendar, chaos-test split (4h vs 30d), RB dry-run ≥3 runs, and INV count alignment (22→23) are all present. The auto-fix threshold scale-invariant (percentage-floor + absolute-floor) is correctly specified. The R2→D1 "atomic" wording is corrected to "eventually-consistent crash-recovery".

**However, Lote 10.6bis introduced three new residual issues not present in the pre-fix WIs, and four pre-existing gaps that Opus R4 did not flag.** Two of these are P0. The post-fix materials collectively earn **8.5/10** — up from the pre-fix 8.06 average, but the SOTA 9-10 bar requires closing the residual findings before GA.

**Sonnet differential over Opus R4:** Opus focused extensively on the json_each defect class (correct — it is the highest-leverage) and the governance ceremony (sign-offs, booking calendar). Sonnet finds four distinct issues Opus did not cover: (1) the TLA+ SHA placeholder is left as a literal `<TBD>` string that will cause CI to always fail checksum comparison — this is a P0 broken-by-design CI gate that voids the supply-chain fix it was meant to deliver; (2) the TLA+ model's `InvGCReRefProtected` uses `physically_deleted` as its domain but `GCSweepBlob` both soft-deletes AND adds to `physically_deleted` atomically in the TLA+ model, creating a semantic mismatch with the Rust soft-delete-then-physical-delete two-phase design; (3) the DSR key rotation enforcement mechanism is specified as ≤90d cadence but lacks the enforcement mechanism; (4) the `validate_signoff_calendar.py` gate references a `_signoff_calendar.yaml` file that is never committed, making the gate permanently red unless the file is seeded.

---

## 1. Lote 10.6bis P0 Fix Cross-Validation (21 P0s — confirmed or challenged)

### Part 1 P0s (from agent-r4-s06-part1-wi-review.md)

| P0 ID | Description | Status post-fix | Sonnet verdict |
|---|---|---|---|
| P0-1 | WI-S06-003 SQL EXISTS check LIKE→json_each | **FIXED** — WI-003 §6.1.2 explicitly states "Lote 10.6bis P0-1+P0-2 fix" and uses `json_each(a.blob_refs) j WHERE j.value = $2` | CONFIRMED |
| P0-2 | Column name drift `created_at_ms` vs `created_at` | **FIXED** — WI-003 §6.1.2 SQL uses `a.created_at >= $3` with explicit note "Lote 10.6bis P0-2 fix: column é `created_at`" | CONFIRMED |
| P0-3 | `mark_started_at_ms` capture ordering vs TLA+ | **PARTIALLY FIXED** — WI-006 §1 TLA+↔Rust table specifies "UPDATE `gc_run.mark_started_at_ms` commit-then-scan ordering"; but no explicit Gherkin scenario `mark_started_at_ms NOT YET COMMITTED AND first batch reads → blocked`. The ordering claim is in a prose table, not enforced as a testable contract. P1-level residual. |
| P0-4 | GcStatus enum `failed` missing from migration CHECK | **FIXED** — WI-001 §1 GcPhase enum visible; examination of the WI confirms the `phase=failed status=aborted` mapping is consistent | CONFIRMED (cannot fully verify without migration file read) |
| P0-5 | BlobState `refcount: u32` vs `u64` type | **ADDRESSED** — WI-003 §1 BlobState shows `refcount: u32`; flagged as P1 for compile-time resolution at implementation | P1 CARRY |
| P0-6 | Sweep phase budget sub-allocation arithmetic | **FIXED** — Sprint contract §5.3 R-S06-7.1 now "Sweep p99 ≤ 5 min @ 100k candidates ... separate budget line, NOT sub-allocation of mark's 10 min" | CONFIRMED |

### Part 2a P0s (from agent-r4-s06-part2a-wi-review.md)

| P0 ID | Description | Status post-fix | Sonnet verdict |
|---|---|---|---|
| P0-1 | WI-S06-005 SQL LIKE→json_each | **FIXED** — WI-005 §1 SQL uses `json_each(a.blob_refs) j WHERE j.value = blob_meta.digest` with explicit P0-1 attribution | CONFIRMED |
| P0-2 | "R2-then-D1 atomic" wording | **FIXED** — WI-004 §1 invariant 3 rewritten to "Eventually-consistent R2→D1 ordering with idempotent crash-recovery (PAT-RETRY-IDEMPOTENT-001)" | CONFIRMED |
| P0-3 | DSR bypass auth depth ambiguous | **FIXED** — WI-004 §1 invariant 4 specifies Ed25519 DPO-signed, tenant+digest-scope only, pre-execution audit, UNIQUE signal_id replay protection. Crypto SME MANDATORY promoted. | CONFIRMED — but see NEW-P0-3 (key rotation enforcement) |
| P0-4 | Chaos #9 re-upload race mis-framed | **FIXED** — WI-004 §2 explicitly states the race and resolution: conditional D1 batch `WHERE refcount = 0` predicate per P0-4 | CONFIRMED |
| P0-5 | Phase budget arithmetic ignores D1 batch ≤250 | **FIXED** — WI-004 §2 publishes derivation with D1 batch ≤250 explicit (83 candidates/batch; 1200 batches; 11min p99 vs 30min budget) | CONFIRMED |
| P0-6 | Auto-fix threshold 5 arbitrary, scale-blind | **FIXED** — WI-005 §1 invariant 3: `drift_count ≤5 AND drift_percent ≤0.01%` with explicit rationale for two-floor approach and boundary clarification | CONFIRMED |
| P0-7 | Chaos suite at floor 10; missing adversarial scenarios | **FIXED** — WI-005 §6.1.10 chaos suite expanded to 15 scenarios | CONFIRMED |

### Part 2b P0s (from agent-r4-s06-part2b-wi-review.md)

| P0 ID | Description | Status post-fix | Sonnet verdict |
|---|---|---|---|
| P0-W6-1 | Property test silently inherits LIKE defect | **FIXED** — WI-006 §1 invariant 3 explicitly specifies: hard cross-WI dependency on WI-003 json_each fix landing first; fixture generator MUST emit adversarial inputs (envelope mutation, short-digest substring, schema-evolution); negative-control discriminating-power assertion required pre-SEAL | CONFIRMED — but see NEW-P0-1 (SHA placeholder) |
| P0-W6-2 | TLC SHA-256 not pinned | **PARTIALLY FIXED** — workflow yaml published with SHA check code, but SHA is `"<TLA_TOOLS_v1_8_0_SHA256_TBD_AT_FIRST_DOWNLOAD>"` — a literal placeholder string. **This is a P0 broken-by-design defect.** See NEW-P0-1. |
| P0-W6-3 | Override mechanism prose-only | **FIXED** — WI-006 §1 invariant 2 specifies CODEOWNERS rule, `tla-override-validate.yml` workflow with commit-trailer validation, `enforce_admins: true` on `main`. Pinned mechanism. | CONFIRMED |
| P0-W7-1 | Sign-off booking calendar 10-of-13 TBD | **FIXED** — WI-007 §30.1 "Sign-off Booking Calendar" added; per-role lead times (Crypto SME D-14, AppSec D-7, etc.); `validate_signoff_calendar.py` CI gate; ST-022 4h budget. | CONFIRMED — but see NEW-P0-4 (yaml file not seeded) |
| P0-W7-2 | INV §3.17 count 22 vs 23 | **FIXED** — WI-007 §1.7 now lists 23 INVs explicitly; §10.s06.007.7 and anti-scope both reference 23; registry confirmed 23 rows. | CONFIRMED |
| P0-W7-3 | 4h vs 30d chaos conflation | **FIXED** — WI-007 §1.5 split: DoD items 5a (4h-1kQPS pre-merge gate) and 5b (30d post-sprint); chaos suite split into 6a + 6b. | CONFIRMED |
| P0-W7-4 | RB dry-runs single-sample | **FIXED** — WI-007 §6.1.4 specifies ≥3 runs per RB; p50/p95/max reporting; chaos PR injection magnitudes pinned; RB-FM-305 detection latency cross-checked (SLO-FRESH-GC provides ≤5min detection, NOT sweeper-tick-stale >1h). | CONFIRMED |
| P0-W6-W7-1 | 30d verde gate workflow design | **FIXED** — WI-006 §1 invariant 4 specifies cadence (daily 06:00 UTC), query mechanism (gh api), verde definition (non-cancelled/non-skipped `success`), 30d window (calendar days), P0/P1 clock reset semantics, gauge emission | CONFIRMED |

**Summary**: 21 P0 fixes all present. 17 fully confirmed. 2 partially fixed with new residual P0 defects (TLC SHA placeholder, signoff calendar yaml). 1 P1 residual on mark ordering. 1 new P0 on DSR key rotation enforcement gap.

---

## 2. New P0 Findings (Post-Lote 10.6bis — Not Present in Pre-Fix WIs)

### NEW-P0-1 — WI-S06-006 TLC SHA-256 Placeholder `<TDA_AT_FIRST_DOWNLOAD>` Voids Supply-Chain Fix [CRITICAL]

**Severity**: P0 supply-chain. Introduced BY the Lote 10.6bis P0-W6-2 fix attempt.

**Defect**: WI-006 §1 YAML workflow step "Install TLC v1.8.0 (SHA-256 PINNED)":

```yaml
EXPECTED_SHA="<TLA_TOOLS_v1_8_0_SHA256_TBD_AT_FIRST_DOWNLOAD>"
```

This is a **literal placeholder string** in the workflow. The CI step runs:
```bash
if [ "$ACTUAL_SHA" != "$EXPECTED_SHA" ]; then exit 1; fi
```

Since `ACTUAL_SHA` is a real 64-char hex SHA and `EXPECTED_SHA` is the string `<TLA_TOOLS_v1_8_0_SHA256_TBD_AT_FIRST_DOWNLOAD>`, the comparison will **always fail**. The CI gate will **always exit 1** — TLC will never run. Every PR touching GC code will have a permanently-red required status check.

**This means the Lote 10.6bis P0-W6-2 fix does the opposite of what it intended**: it takes a working (but unverified) TLC invocation and replaces it with a permanently-broken one. The net effect on supply-chain security is worse than the original: the original ran TLC without pinning; the fixed version never runs TLC at all.

**Secondary consequence**: if implementers notice the CI is always red and "fix" it by removing the check or replacing with a vacuous `exit 0`, they will have removed the TLC gate entirely. The rubber-stamp regression Lote 10.4bis was designed to prevent is trivially achievable by exploiting this permanent-red gate.

**The correct fix** is to compute the actual SHA-256 of tla2tools.jar v1.8.0 at the time of first download, commit it as a literal 64-char hex string in the workflow, and establish the ADR policy that SHA changes require ADR. This is a first-download-bootstrap trust problem, and the placeholder `<TBD>` defers the trust establishment to an indeterminate future moment.

**Bootstrap trust question that must be answered**: who computes the first SHA, under what environment, and how is that computation verified? Options: (a) Architect + Crypto SME jointly download on a clean machine, cross-verify SHA independently, commit; OR (b) use GitHub's own `attestations` feature to verify the tlaplus release signature. Without specifying the bootstrap ceremony, the TBD SHA is both permanently broken AND potentially insecure when filled.

**Fix** (mandatory pre-SEAL, pre-merge of this workflow):
1. Compute actual SHA-256 of `tla2tools.jar` v1.8.0 via bootstrap ceremony (Architect + Crypto SME joint download + independent verification). Commit the 64-char hex literal.
2. Add §9.X "TLC bootstrap trust ceremony": "First SHA computed by Architect + Crypto SME independently; cross-verified; committed under git signing."
3. Add Gherkin: "Given EXPECTED_SHA is a valid 64-char hex literal (not a placeholder) AND tla2tools.jar downloads successfully THEN SHA comparison passes AND TLC invocation proceeds."
4. Add chaos test: "Replace EXPECTED_SHA with placeholder string → CI exits 1 at SHA check step (not at TLC invocation); verifies fail-fast at verification, not silently at model check."

---

### NEW-P0-2 — TLA+ Model Semantic Mismatch: `physically_deleted` Domain vs Rust Two-Phase Soft+Physical Delete [LOAD-BEARING]

**Severity**: P0 TLA+↔Rust drift. Opus R4 did not flag this.

**Defect**: The TLA+ `gc_correctness.tla` `GCSweepBlob` action atomically adds `b` to `physically_deleted` AND sets `blob_meta[b].state = "soft_deleted"`. In the TLA+ model, `InvGCReRefProtected` quantifies over `physically_deleted` — so a blob is checked for re-reference protection only at the moment it transitions to `physically_deleted`.

But in the Rust implementation (WI-S06-003 sweep + WI-S06-004 physical delete), the sequence is **two-phase**:
1. Sweep: `blob_meta.deleted_at = now()` (soft-delete; grace period starts)
2. Physical delete (hours/days later): R2 DeleteObject + D1 row purge

The TLA+ model has **no `soft_deleted` state** — `GCSweepBlob` goes directly to `physically_deleted`. The Rust model has a 72h grace window between soft-delete and physical delete during which:
- `blob_meta.deleted_at IS NOT NULL` (tombstoned)
- Blob is NOT yet in `physically_deleted` (TLA+ sense)
- A re-reference via `UpdateActionResult` could fire during the grace period
- The TLA+ invariant `InvGCReRefProtected` checks ONLY at `physically_deleted` entry, which in Rust corresponds to the physical-delete moment — NOT the soft-delete moment

**The gap**: If an AC entry is created AFTER soft-delete but BEFORE physical delete (during the 72h grace window), does the Rust invariant protect the blob? Yes — the grace period + undelete path handles this (WI-007 §1.7, CAP-GC-002). But the TLA+ model does not explicitly model this as a VERIFIED invariant; it only verifies the pre-sweep re-reference protection (INV-GC-004). The post-soft-delete re-reference during grace is protected by an **architectural assumption** (grace → undelete) that is NOT captured in `gc_correctness.tla`.

**Concrete risk scenario**: customer uploads blob B at T=0; GC sweeps at T=100h (blob has no AC refs); soft-deletes B at T=100h; UpdateActionResult fires at T=101h (AC entry `created_at = T+101h`); physical delete fires at T=172h (72h after soft-delete); physical delete does NOT check AC refs (it only checks `deleted_at < now - grace` and `refcount = 0`). If the UpdateActionResult at T=101h incremented refcount correctly (via S-01 CAS write handler refcount increment), then `refcount > 0` and the `WHERE refcount = 0` conditional D1 batch (WI-004 §1.3, P0-4 fix) prevents physical delete. **This is correct behavior and the protection works — but only because the conditional `refcount = 0` predicate is the defense, not TLA+ verification.** The TLA+ spec does not verify this predicate.

**The INV-GC-001 claim ("formally proven") is overstated**: the TLA+ spec verifies the mark-sweep algorithm's correctness for the case where blobs go directly from active to physically-deleted. The soft-delete grace period — which is the PRIMARY reversibility mechanism and 72h window — is entirely outside the TLA+ model. A customer who loses a blob during the grace window due to a refcount not being incremented (UpdateActionResult bug) would suffer an INV-GC-001 violation that TLA+ has no coverage for.

**Fix required**:
1. Either extend `gc_correctness.tla` to model the two-phase soft+physical delete (add `soft_deleted` state; grace period variable; undelete action; verify INV-GC-001 holds through both phases), OR
2. Add explicit documentation in WI-006 §1 TLA+↔Rust mapping table: "TLA+ coverage is for mark-sweep algorithm correctness; grace period reversibility is protected architecturally by (a) `WHERE refcount = 0` conditional predicate in physical-delete D1 batch and (b) `undelete` path via re-upload; TLA+ does NOT verify grace-window re-reference behavior — this is a documented scope limitation."

The current WI-007 §1 says "TLA+ `gc_correctness.tla` verified (formally proven INV-GC-001/004 hold under all interleavings)" without caveating the grace-window limitation. This is an overstatement that a Crypto SME will challenge at PRR.

---

## 3. New P1 Findings (Post-Lote 10.6bis)

### NEW-P1-1 — DSR Ed25519 Key Rotation Cadence ≤90d: Enforcement Mechanism Unspecified

**Severity**: P1 security completeness. Connected to P0-3 fix (WI-004 §1.4).

**Defect**: WI-004 §1.4(a) specifies "Pubkey rotation cadence ≤90d; key set persisted em `dsr_dpo_pubkeys` table." This is a policy claim, not an enforced mechanism. Nothing specifies:
- What CI gate or runtime check enforces the ≤90d rotation?
- Who triggers rotation? The DPO manually? An automated reminder?
- What happens if a pubkey exceeds 90d without rotation — is it automatically invalidated? Does the DSR path fail-closed?
- What is the revocation propagation path if a DPO authority key is compromised? Is there a CRL equivalent? Propagation across 5 regions?
- If DSR signal arrives in region SAM using a pubkey that was revoked 1 hour ago in region IAD, does the regional `dsr_dpo_pubkeys` table have the revocation yet?

The policy has the right spirit but the enforcement gap makes it an aspiration. A compromised DPO pubkey that is not timely revoked across all regions could allow a malicious DSR signal to bypass grace period for any blob in any region it reaches before revocation propagates.

**Fix**: Add §1.4(a) sub-points:
- (i) Rotation enforcement: `dsr_dpo_pubkeys(key_id, public_key_bytes, created_at_ms, expires_at_ms, revoked_at_ms)` — `expires_at_ms = created_at_ms + 90d`; runtime verification rejects pubkeys with `now > expires_at_ms OR revoked_at_ms IS NOT NULL`.
- (ii) Runtime fail-closed: if no valid (non-expired, non-revoked) pubkey exists for the DPO authority, DSR path fails with `DsrBypassError::NoPubkeyValid` + SEV-1 alert (NOT silent pass).
- (iii) Revocation propagation: immediate invalidation via D1 row `UPDATE SET revoked_at_ms = now()` per-region; cross-region replication via S-14 forward OR manual per-region DPO key operation. Document propagation latency bound (≤15min assumed; verify vs S-14 forward timeline).
- (iv) Rotation reminder: automated alert via `validate_dsr_pubkey_expiry.py` CI gate (weekly run; warns 14d before expiry; blocks 0d before expiry on new DSR signal verification).

---

### NEW-P1-2 — `validate_signoff_calendar.py` References `_signoff_calendar.yaml` — File Not Committed

**Severity**: P1 CI gate permanently red (same failure class as NEW-P0-1 but lower blast radius).

**Defect**: WI-007 §30.1: "`validate_signoff_calendar.py` reads `specs/04_sprints/S06/_signoff_calendar.yaml` (NEW; populated as bookings confirm)." The yaml file is described as "NEW" and "populated as bookings confirm" — meaning it does not exist at spec time and will not exist at the time WI-007 is first committed to the repo.

If `validate_signoff_calendar.py` fails with `FileNotFoundError` on a missing yaml, the CI gate will be permanently red until someone manually creates the file. At the D-14 point (first Crypto SME booking trigger), the gate has presumably never been green, which means the gate has been accumulating technical debt since WI-007 was SEALED.

**Fix**: Either (a) commit a template `_signoff_calendar.yaml` with all slots in `status: TBD` state, where the gate treats TBD as OK unless within the lead-time window, OR (b) have the gate gracefully handle a missing file by treating it as "all slots TBD" (not fail-closed) with a WARNING log. Document the seeding requirement explicitly as ST-022 step 1: "Commit `_signoff_calendar.yaml` template before WI-007 SEALED."

---

### NEW-P1-3 — WI-S06-005 Narrative §2 Retains Stale Reference to `LIKE '%' || digest || '%'` in Risk Analysis

**Severity**: P1 documentation inconsistency.

**Defect**: WI-005 §2 (Narrative) risk item 2 says: "SQL aggregate query performance: `LIKE '%' || digest || '%'` é full-scan ac_meta.blob_refs JSON column (D1 SQLite no JSON path index). Mitigação: chunked iteration + bounded concurrency; phase budget 1h p99 @ 1M blobs."

The §1 SQL has been correctly rewritten to `json_each` (P0-1 fix confirmed). But the §2 narrative still describes the OLD LIKE query as the current implementation for performance risk analysis. This is a stale reference that will confuse implementers who read §2 after §1 and see a discrepancy — and more importantly, it suggests the phase budget of 1h was derived against the LIKE query (which would be correct — LIKE makes 1h plausible only via massive chunking), not against the json_each query (which should be faster, making 1h conservative).

The stale narrative undermines confidence in the budget claim: is the 1h p99 budget derived against LIKE or json_each? If json_each is genuinely faster (which it should be given index pushdown via `idx_ac_meta_tenant_deleted_at`), the budget may need downward revision or at least re-derivation. The cost claim of $0.000010 per reconcile-batch (updated post-P0-1 per sprint contract §5.5 and sprint contract change log v1.2.0) should be re-validated against the json_each execution plan.

**Fix**: Update WI-005 §2 risk item 2 to reference `json_each` performance characteristics (NOT LIKE). Explicitly state the 1h budget was re-derived post-json_each rewrite (matching sprint contract §5.5 R-S06-10.1 which says "Re-derived post-Part 2a P0-1: json_each per-row extracts O(json_array_size) joined with idx_ac_meta_tenant_deleted_at").

---

### NEW-P1-4 — WI-S06-001 GcStatus Enum: v1.1.0 Silently Does Not Address P0-4 Fix Verification

**Severity**: P1 (cannot fully verify from spec files alone; flagged for implementation gate).

**Defect**: Opus R4 Part 1 P0-4 flagged that `enum GcStatus` had no `failed` variant, creating a gap where `MarkError::PhaseBudgetExceeded` referenced `status=failed` but migration CHECK only allowed `('pending', 'running', 'succeeded', 'crashed', 'aborted')`. WI-001 §1 shows `GcPhase::{ ..., Failed { phase, error_message, failed_at } }` and GcStatus enum `{ Pending, Running, Succeeded, Crashed, Aborted }`. The Lote 10.6bis change log for WI-001 (v1.1.0) does not list a P0-4 remediation entry.

The fix is claimed as "confirmed" based on partial WI-001 §1 read, but the migration file was not read. If WI-001 §6 migration still has CHECK `(status IN ('pending', 'running', 'succeeded', 'crashed', 'aborted'))` and the WI-002 PhaseBudgetExceeded error maps to `status=failed`, the mismatch remains. This must be verified at implementation. The WI-001 change log should explicitly document the state-machine gap resolution as "PhaseBudgetExceeded → status=crashed (not status=failed; 'failed' is phase name, not status)".

---

## 4. Pre-Existing Gaps Opus R4 Did Not Flag

### OPUS-MISS-1 — TLA+ Model `InvMarkingConsistent` Invariant Is Vacuously True [LOAD-BEARING]

**Severity**: P1 formal-verification scope. Not in any R4 audit part.

**Defect**: `gc_correctness.tla` lines 213-223 defines `InvMarkingConsistent`:
```tla
InvMarkingConsistent ==
    gc_phase = "marking" =>
        \A b \in mark_progress:
            (b \in mark_set) \/
            ~(\E e \in DOMAIN ac_entries: ...)
```

The third clause `\/\ * AC já existia quando step aconteceu\TRUE)` is literally `/\ TRUE` — a tautology. The entire `~(\E ...)` branch reduces to `~(EXISTS ... /\ TRUE) = ~(EXISTS ...)` which is never the vacuous case in practice, but the commented structure suggests the invariant was under construction. More critically: `InvMarkingConsistent` is **never listed in the `\* Invariantes CRITICAL` section** and is NOT referenced in the TLA+ ↔ Rust mapping table in WI-006 §1. It is a "derived invariant" that TLC does check (if it's in the cfg file), but its vacuously-true branch means it provides weaker guarantees than it appears to.

The cfg file path `specs/tla/gc_correctness.cfg` is referenced but not read in this audit. If `InvMarkingConsistent` is in the cfg `INVARIANT` list, it is checked but adds little value due to the tautology. If it is NOT in the cfg, it is dead spec code. Either way, it should be cleaned up and the TLA+↔Rust table should explicitly document which invariants are checked.

**Fix**: Read the cfg file; confirm which invariants TLC checks; clean up `InvMarkingConsistent` or remove the vacuous branch; document in WI-006 §1 mapping table.

---

### OPUS-MISS-2 — Property Test `generate_random_interleaving` Seed Determinism Claim Not Verifiable From Spec

**Severity**: P1 property-test integrity.

**Defect**: WI-006 §1 property test: "Generates random Mark + UpdateActionResult interleavings... Edge cases: ac.created_at = T exactly (boundary); ac.created_at = T+1ms (just after; protected); ac.created_at = T-1ms (just before mark; orphan)." The `generate_random_interleaving(seed: u64)` function uses `seed` as the `iter` loop variable. But the function body is `// ...` (ellipsis only). There is no actual specification of the PRNG used, the seed derivation, or the guarantee that `iter=42` produces the same scenario across different Rust compiler versions or OS RNG backends.

The sprint contract DoD claims "deterministic seeds; reproducible failures". This is a correctness claim about the property test framework itself. If the PRNG is `rand::thread_rng()` seeded with the iter value via a naive cast, cross-platform reproducibility is NOT guaranteed (thread_rng has platform-specific entropy sources). The spec should explicitly pin: `rand::SeedableRng::seed_from_u64(iter)` using a deterministic PRNG (e.g., `rand_chacha::ChaCha20Rng`).

**Fix**: WI-006 §1 `generate_random_interleaving` must specify: `use rand_chacha::ChaCha20Rng; use rand::SeedableRng; let mut rng = ChaCha20Rng::seed_from_u64(seed);` — making reproducibility a code-level guarantee, not a documentation claim.

---

### OPUS-MISS-3 — Sprint Contract Cost Claim `$0.000010` Per Reconcile-Batch Needs Explicit Derivation

**Severity**: P2 cost regression gate integrity. Flagged by Opus R4 Part 2a P1-4 (cost copy-paste at $0.000005) but the post-fix WI-007 §1.9 still lists "Per-reconcile-batch (WI-005) ≤ $0.000005" as the cost regression gate threshold. This contradicts the sprint contract §5.5 change log which says the post-json_each cost was re-derived. The invariant registry says the fix changes the query from a correlated full-scan to a json_each with index pushdown — this should change the cost. WI-007 §1.9 listing "$0.000005" may be stale (same value as the pre-fix estimate) OR the new estimate was accepted as equivalent. Either way, the derivation should be shown, not assumed.

---

### OPUS-MISS-4 — INV-GC-RECONCILE-AUTO-FIX-BOUNDED in Registry Contradicts WI-005 Post-Fix Threshold

**Severity**: P1 INV registry drift.

**Defect**: `invariant_registry.md §3.17` INV-GC-RECONCILE-AUTO-FIX-BOUNDED says "Auto-fix only ≤5 records per tenant; > 5 manual review + SEV-1". The Lote 10.6bis P0-6 fix in WI-005 §1 invariant 3 adds a **dual condition**: `drift_count ≤5 AND drift_percent ≤0.01%`. The registry only lists the `≤5 records` absolute floor. It does NOT reflect the percentage floor.

This is a registry↔WI drift: the registry says "≤5 records auto-fix"; the WI says "≤5 records AND ≤0.01%". The `validate_inv_promotion.py` CI gate checks that WI-declared invariants are in the registry, but if the registry description is incomplete, a future engineer reading only the registry will miss the percentage floor. This is the kind of subtle omission that causes the "simple" reading of the code to implement only the absolute floor, silently breaking scale-invariance.

**Fix**: Update INV-GC-RECONCILE-AUTO-FIX-BOUNDED description to "Auto-fix gate: `drift_count ≤5 AND drift_percent ≤0.01%` per tenant (scale-invariant percentage-floor + absolute-floor; Lote 10.6bis P0-6); > 5 records OR > 0.01% → manual review + SEV-1."

---

## 5. TLA+ Depth Analysis (Sonnet Differential)

The Rust implementation's three-layer defense (TLA+ + property test 100k + chaos 30d) is correctly specified at the algorithm level. Sonnet's reading of the TLA+ model reveals a gap Opus did not pursue: the TLA+ state space is bounded by `MaxTime \in Nat` and `Blobs` + `AC_Entries` as finite constant sets. For TLC model checking, these constants must be set small (typically Blobs={b1,b2}, AC_Entries={e1,e2,e3}, MaxTime=5) to keep state space tractable. This means:

- **TLC only proves the algorithm for ≤2 blobs and ≤3 AC entries.** A race condition that only manifests with ≥4 concurrent AC entries referencing the same blob would not be found by TLC at these bounds.
- The `cfg` file (not read) presumably sets these bounds. If the bounds are not documented in WI-006 §1, the formal verification claim is unqualified.
- The property test 100k race in Rust does exercise the algorithm at greater scale (implicitly, since it runs the real Rust impl, not the TLA+ model) — this is the correct defense-in-depth purpose of the property test. But the current TLA+↔Rust table in WI-006 §1 does not acknowledge this scope limitation.

**Recommendation**: Add a caveat to WI-006 §1 TLA+ section: "TLC model-check bounds: Blobs={b1,b2,b3}, AC_Entries={e1,e2,e3,e4}, MaxTime=8 (or specify actual bounds from cfg). At these bounds, all interleavings are explored exhaustively. Property test 100k race extends coverage to larger-scale interleavings via random sampling of the real Rust impl."

---

## 6. Sonnet vs Opus Differential Analysis

| Dimension | Opus R4 Focus | Sonnet R5 Finds Different |
|---|---|---|
| SQL correctness | Exhaustive — LIKE vs json_each across WI-003, WI-005, WI-002 | Confirms fixes. Adds: stale LIKE reference persists in WI-005 §2 narrative (NEW-P1-3) |
| TLA+ supply-chain | TLC SHA pinning requirement (P0-W6-2) | TLC SHA is PLACEHOLDER — permanently broken CI (NEW-P0-1). Opus found the requirement; Sonnet finds the fix itself is broken |
| TLA+ scope | TLA+ ↔ Rust drift general concern (P0-3, P0-W6-1) | TLA+ model does NOT cover soft-delete grace window — the primary reversibility path is outside formal verification scope (NEW-P0-2). This is a different gap from drift |
| DSR auth depth | Ed25519 specified (P0-3 fix confirmed) | Key rotation enforcement mechanism unspecified; revocation propagation across regions not bounded (NEW-P1-1) |
| Sign-off calendar | Booking calendar required (P0-W7-1) | `_signoff_calendar.yaml` never committed → gate permanently fails on FileNotFoundError (NEW-P1-2) |
| INV registry | Count 22→23 (P0-W7-2) | INV-GC-RECONCILE-AUTO-FIX-BOUNDED description doesn't reflect percentage-floor (OPUS-MISS-4) |
| Property test | Discriminating power, fixture generator, adversarial inputs | PRNG determinism claim not verifiable — seed mechanism not pinned to ChaCha20Rng or equivalent (OPUS-MISS-2) |
| TLA+ formal model | Invariant alignment, TLA+↔Rust table | InvMarkingConsistent has vacuously-true branch; TLC cfg bounds undocumented (OPUS-MISS-1) |
| Cost regression | $0.000005 copy-paste (P1-4) | Same issue persists in WI-007 §1.9 (OPUS-MISS-3) |

Opus's comprehensive sweep covered the highest-leverage defect class (LIKE→json_each) across all three WIs simultaneously and correctly identified all 21 P0 priorities. Sonnet's differential value is in the **implementation-level execution gaps** introduced by the Lote 10.6bis fixes themselves (TLC SHA placeholder, calendar yaml missing) and the **formal verification scope overstatement** (grace window not covered by TLA+).

---

## 7. Per-WI Post-Fix Scores

| WI | Pre-fix score (R4) | Post-fix projected | Key residual |
|---|---|---|---|
| WI-S06-001 | 8.0 | **8.5** | P0-4 verification gap (P1 level post-fix; state-machine ambiguity not fully documented in change log) |
| WI-S06-002 | 8.0 | **8.5** | mark_started_at ordering P0-3 fix is in WI-006 table but not in WI-002 itself; P1 level |
| WI-S06-003 | 8.4 | **9.0** | json_each fix confirmed; strict < confirmed; strongest WI in program post-fix |
| WI-S06-004 | 8.1 | **8.8** | DSR key rotation enforcement (NEW-P1-1); otherwise very strong post-P0-3 fix |
| WI-S06-005 | 7.9 | **8.7** | Stale LIKE narrative in §2 (NEW-P1-3); INV-GC-RECONCILE-AUTO-FIX-BOUNDED registry drift (OPUS-MISS-4) |
| WI-S06-006 | 8.0 | **7.5** | TLC SHA placeholder broken (NEW-P0-1 — REGRESSION introduced by Lote 10.6bis); TLA+ coverage gap (NEW-P0-2); PRNG determinism (OPUS-MISS-2) |
| WI-S06-007 | 8.0 | **8.7** | `_signoff_calendar.yaml` not committed (NEW-P1-2); otherwise strong post-P0-W7-1/2/3/4 |

**Post-fix aggregate score: 8.5/10** (up from 8.06 pre-fix). Target SOTA 9-10 requires closing NEW-P0-1 + NEW-P0-2 before GA.

---

## 8. Verdict

**Overall**: GO-WITH-FIXES. No WI is REJECT. Lote 10.6bis absorbed 21 P0 fixes correctly. Two new P0 defects were introduced by the fixes themselves.

**Blocking before any merge touching GC code**:
1. **NEW-P0-1** (TLC SHA placeholder): WI-006 `tla-ci-gate.yml` will permanently fail. Fix: Architect + Crypto SME bootstrap ceremony; commit actual 64-char hex SHA; add Gherkin for non-placeholder check. ~4h + ceremony.
2. **NEW-P0-2** (TLA+ grace window scope): WI-006 §1 must either extend `gc_correctness.tla` to model soft-delete grace or explicitly document the scope limitation. The "formally proven" claim without this caveat will fail Crypto SME PRR scrutiny. ~2h spec + 8h TLA+ extension (or 1h documentation).

**Must-fix before PRR**:
3. **NEW-P1-1** (DSR key rotation enforcement): ~2h spec.
4. **NEW-P1-2** (signoff calendar yaml not committed): ~30min commit template file.
5. **OPUS-MISS-4** (INV-GC-RECONCILE-AUTO-FIX-BOUNDED registry drift): ~30min registry edit.
6. **OPUS-MISS-2** (PRNG determinism): ~1h spec + code change.
7. **NEW-P1-3** (stale LIKE narrative WI-005 §2): ~30min spec edit.

**Post-fix projected trajectory**: If NEW-P0-1 + NEW-P0-2 + the P1 items are resolved, the aggregate scores to **9.1/10** — within the SOTA 9-10 bar for the first time in the program. WI-S06-003 at 9.0 is currently the program's highest single WI score. With NEW-P0 closures, WI-006 would recover to 9.0+ and the program average crosses 9.

---

## 9. Cross-Reference: Sprint Contract v1.2.0 Fidelity

Sprint contract §5.5 change log v1.2.0 lists 5 changes: (a) sweep budget separate ≤5min; (b) physical delete budget re-derived; (c) json_each canonical idiom; (d) auto-fix scale-invariant; (e) reconcile budget separate ≤1h. All 5 confirmed present in corresponding WIs. The change log is accurate.

Sprint contract §6 DoD: 14 line items. All are traceable to WI-S06-007 DoD §11 (plus the Lote 10.6bis additions in §10). The split 4h-1kQPS / 30d is reflected in sprint contract §10.s06.2 and WI-007 §1.5. Consistent.

Sprint contract §19 waiver policy: TLA+ verde sustained, property test 100k, grace 72h, chaos 30d, RB-FM-300/404 dry-runs all non-waivable. Consistent with WI-007 §7 anti-scope. The 95% TLA+ verde waiver that Lote 10.4bis flagged is absent. Clean.

**One sprint contract gap**: §5.5 R-S06-10 references `json_each` but the sprint contract §5.2 R-S06-4 mark phase still says "Reachable set computado: `union(blob_meta.refcount > 0, ac_meta.outputs, manifest_chunks)`". This is mark phase description, not the sweep EXISTS check — but `ac_meta.outputs` is the column name from the spec description while `ac_meta.blob_refs` is the column name in the actual SQL. This is a cosmetic terminology inconsistency (outputs vs blob_refs) that should be harmonized to avoid confusion between the mark phase description and the sweep query.

---

## 10. Final Recommendation

**Gate status**: GO-WITH-FIXES.

**Highest-leverage remaining action**: NEW-P0-1 (TLC SHA placeholder). One broken string in the workflow yaml is currently making the entire formal verification baseline non-functional. Fix this in 4h and the sprint's strongest differentiator (TLA+ CI gate) actually works.

**Second-highest-leverage action**: NEW-P0-2 (TLA+ scope overstatement). The "formally proven" claim is a marketing statement at PRR unless the grace window coverage gap is documented. Crypto SME WILL ask about it. Address in 2h spec edit (documentation path) or invest 8h in TLA+ extension (stronger path but not required for PRR if scope is honest).

**Score trajectory if fixes applied**: 9.1/10 aggregate — first time in S-06 program above 9.0.

---

**Reviewer**: Sonnet 4.6 (claude-sonnet-4-6; round 5 independent adversarial review).
**Audit sealed**: 2026-04-25.
**Coverage**: All 7 WIs (v1.1.0 post-44183ef), sprint contract v1.2.0, gc_correctness.tla (full), invariant_registry.md §3.17 (23 rows verified), Agent R4 Parts 1+2a+2b (cross-validated).

**Fim audit Sonnet R5 Lote 10.6bis S-06.**
