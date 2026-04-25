---
id: "AUDIT-2026-04-25-AGENT-R4-S06-PART1"
type: "audit"
doc_status: "DRAFT"
audit_status: "ACTIVE"
created: "2026-04-25"
reviewer: "Agent R4 (Claude Opus 4.7, 1M context, independent reviewer — round 4)"
scope: "Lote 10.6 — Sprint S-06 Part 1 (WI-S06-001 .. WI-S06-003)"
sprint_contract: "specs/04_sprints/S06/_spec_contract.md v1.1.0"
calibration_baselines:
  - "specs/_audits/2026-04-25-agent-r4-s05-part1-wi-review.md (S-05 part1 8.10/10)"
  - "specs/_audits/2026-04-25-agent-r4-s05-part2-wi-review.md (S-05 part2 8.05/10)"
  - "WI-S04-003 best-in-class 8.6"
files_reviewed:
  - "specs/04_sprints/S06/work_items/WI-S06-001-worker-gc-binary-scheduler-degrade-mode.md (640 lines)"
  - "specs/04_sprints/S06/work_items/WI-S06-002-mark-phase-multi-pass-scan-mark-started-at.md (586 lines)"
  - "specs/04_sprints/S06/work_items/WI-S06-003-sweep-phase-soft-delete-inv-gc-004.md (648 lines)"
cross_references:
  - "specs/04_sprints/S06/_spec_contract.md (v1.1.0)"
  - "specs/tla/gc_correctness.tla (verified Lote 5.13 + 7.1)"
  - "specs/03_architecture/invariant_registry.md §3.4 + §3.17"
  - "specs/04_sprints/S04/work_items/WI-S04-002-d1-ac-meta-r2-bucket.md (ac_meta schema canonical)"
  - "specs/04_sprints/S04/work_items/WI-S04-001-reapi-actioncache-handlers.md (INV-AC-OUTPUTS-VALID)"
---

# Agent R4 — Lote 10.6 S-06 Part 1 (WIs 001–003) WI Review

> **Reviewer**: Agent R4 (independent SOTA reviewer; ruthless, technical, no diplomacy).
> **Calibration target**: User directive "average não serve. SOTA puro 9-10". Best in program: WI-S04-003 = 8.6. S-05 part1 average 8.10; S-05 part2 average 8.05.

---

## Veredito Geral

S-06 Part 1 is the **single most load-bearing trio in the entire program** — INV-GC-001 ("reachable never deleted") and INV-GC-004 ("mark-phase-aware re-ref safe") are both CRITICAL TLA+ obligations and a production violation of either is the canonical trust-loss-permanente failure mode. The materials show the program has **internalized most Lote 10.5bis lessons preemptively**: CHECK constraints inline (no `ALTER TABLE ADD CONSTRAINT chk_*`), partial UNIQUE INDEX `WHERE status='running'` (Lote 10.5bis state-scoped uniqueness lesson), `tenant_id NOT NULL` everywhere, TenantCtx-only enforcement claimed consistently, audit_outbox correctly cited as WI-S01-004 (not WI-S01-005), no `with_tenant_ctx!` in D1 contexts, BEGIN/COMMIT removed from migrations, ADR canonical path `specs/03_architecture/adrs/` used (verified ADR-0042 exists at that path), invariant registry §3.17 preemptively populated with the new GC INVs, validate_inv_promotion.py CI gate referenced. The TLA+ alignment story is the strongest in the program — the WIs cite `gc_correctness.tla` as the canonical source of truth, INV-GC-004 is literally implemented as the SQL EXISTS check the TLA+ formalizes, and the strict `<` semantics (sweep proceeds only if `ALL ac.created_at < mark_started_at` ≡ `NOT EXISTS ac.created_at >= mark_started_at`) **does match** the TLA+ `GCSweepBlob` action's `~(\E e: ac_entries[e].created_at >= mark_started_at)` enabling condition exactly. The audit fail-closed design (D1 batch atomic; sweep ROLLBACK if audit_outbox INSERT fails) is the right shape and is correctly framed as preserving both INV-GC-001 and INV-OBS-AUDIT-CHAIN-INTEGRITY. WI-S06-003 is materially the strongest of the three: it explicitly emphasizes Crypto SME MANDATORY EMPHATIC (correct for INV-GC-004 cripto-coordenado boundary), publishes the SQL EXISTS check in §6.1.2, ships a 100k race property test directly traceable to sprint contract DoD §6, and frames the strict-`<`-vs-`>=` boundary scenario explicitly in Gherkin §8 with a `T == T` boundary test that resolves to ProtectedReRef (conservative — favorable to reachable). 

But six classes of defects keep the trio short of the **9-10 SOTA bar** and below the WI-S04-003 ceiling. **First (P0, the load-bearing one): the INV-GC-004 SQL EXISTS check uses `LIKE '%' || $2 || '%'` against the `ac_meta.blob_refs` JSON column.** This is wrong on three independent axes simultaneously and is the **single most dangerous defect in the trio** because it is in the load-bearing security-critical query. WI-S04-002 §188+§200 explicitly establishes that `ac_meta.blob_refs` is a **JSON TEXT column** (with denormalized `blob_refs_count`); WI-S04-002 §244 says "Reconcile query (S-06 GC): WHERE tenant_id = ? AND blob_refs JSON contains $digest → full scan; OK as batch op" — the canonical guidance is **JSON-aware membership**, not raw substring. The substring `LIKE` is (a) **collision-prone** if any digest is a substring of another encoding (BLAKE3-256 hex digests are 64 chars so unlikely to collide on hex content but the JSON wrapper introduces `"`, `,`, whitespace which the LIKE doesn't anchor against), (b) **performance-pathological** — leading-`%` LIKE forces a full table scan with no index utility on `blob_refs` (this is exactly what WI-S04-002 §200 calls out as "full-scan acceptable as batch op", but sweep is **NOT** a batch op in the same sense — sweep does **one EXISTS per gc_candidate**, so the per-sweep query on a large `ac_meta` table is O(N_ac) per candidate × N_candidates, which is O(N_ac × N_candidates), not the O(N_ac) batch reconcile WI-S04-002 anticipated), and (c) **semantically incorrect** — `blob_refs` is a JSON array of digests, the canonical SQLite/D1 idiom is `EXISTS (SELECT 1 FROM json_each(ac.blob_refs) WHERE json_each.value = ?)` (JSON1 extension is native to SQLite/D1). The current SQL would mark a digest as "re-referenced" if **any other field in any JSON value contains the digest as a substring**, e.g., a future `metadata.parent_digest` field that happens to contain digest D as a parent reference would protect D from sweep — that is a false positive (favorable to reachable, so safe in the strict-`<` direction), but it is an unaudited semantic. More importantly, if `blob_refs` is ever changed to be a JSON object instead of an array (or wrapped in a metadata envelope), the LIKE check silently breaks **without compile-time or schema-check failure**. This is the SQL injection-shape defect class — not literal injection, but a string-typing-erasure that escapes the type system. The fix is a 1-line change to use `EXISTS (SELECT 1 FROM ac_meta a, json_each(a.blob_refs) j WHERE a.tenant_id = ? AND j.value = ? AND a.created_at >= ?)`. Crypto SME and any AppSec reviewer with SQL background will catch this on the first read.

**Second (P0): column name drift between WIs and the canonical schema.** The canonical `ac_meta` schema in `WI-S04-002` declares `created_at INTEGER NOT NULL` (line 78). WI-S06-002 §1 and WI-S06-003 §1/§6.1.2 use **`created_at_ms`** throughout the SQL, the Rust code (`ac_created_at_ms: u64`), the CHECK constraint comments, the property test descriptions, and the audit emit fields. This is column-name drift identical in defect class to the WI-S04 column-name issues that prior R4 audits flagged. The schema column is `created_at` (with implicit unit unix-ms per the `ms` suffix being added inconsistently elsewhere). Either the canonical `WI-S04-002` schema needs to be **renamed via migration** to `created_at_ms` (which would be a breaking schema change for an already-shipping WI), or **all WI-S06 references must be `created_at` not `created_at_ms`**. The mismatch is in the EXACT load-bearing query (WI-S06-003 §6.1.2), so the SQL won't compile against the actual schema. This is **not subtle** — `sqlx::query_as!` would fail at compile time at the point WI-S06-003 lands — but the spec ships incoherent.

**Third (P0): `mark_started_at_ms` capture race vs the TLA+ obligation has a subtle but real gap.** The WI-S06-002 §1 invariant 1 specifies the atomic capture as `UPDATE gc_run SET mark_started_at_ms = unixepoch_ms() WHERE run_id = X AND mark_started_at_ms IS NULL`. Idempotency is correct. But TLA+ `gc_correctness.tla` `GCMarkStart` action (line 102-110) sets `mark_started_at' = now` **at the moment of phase transition `idle → marking`**. The TLA+ contract is that `mark_started_at` is captured **before any blob is visited by `GCMarkStep`**. The WI-S06-002 §6.1.4 sub-task ST-003 ("mark_started_at_ms atomic capture (SQL UPDATE WHERE IS NULL)") and §1 invariant 1 specifies the SQL UPDATE but **does not** specify it must execute **before** the first multi-pass scan batch begins. If the SQL UPDATE happens **after** the first batch (e.g., the batch loop captures `mark_started_at_ms` lazily on first batch boundary), then a `UpdateActionResult` that fires **between** the first batch read and the SQL UPDATE has `created_at < mark_started_at_ms` — but the AC entry was created **after Mark already saw a blob with no refs**, exactly the race INV-GC-004 is supposed to protect against, and the EXISTS check would NOT fire. The TLA+ obligation requires `mark_started_at` **strictly precedes** any `GCMarkStep`. The Rust code must establish: **`UPDATE gc_run SET mark_started_at_ms = ...` MUST commit before the first `SELECT FROM blob_meta` in the multi-pass scan**. WI-S06-002 §1 / §9.4 says "captured at phase start; persisted em gc_run; TLA+ obligation MarkPhaseStart action" but does not explicitly say "persisted **before** scan begins; capture-then-scan ordering enforced via WAL flush / D1 commit boundary". This is the kind of ordering invariant that will silently break under reorder-friendly compilers or async-runtime stealing. Add an explicit Gherkin scenario `When mark_started_at_ms not yet committed AND first batch reads → 503 Service-Unavailable; first batch BLOCKED until mark_started_at_ms commit` and a property test that races the SQL UPDATE vs the first scan batch.

**Fourth (P0): GcStatus enum vs migration CHECK constraint inconsistency — `failed` is missing.** WI-S06-001 §1 declares `enum GcStatus { Pending, Running, Succeeded, Crashed, Aborted }` (5 variants) and migration 006 declares `CHECK (status IN ('pending', 'running', 'succeeded', 'crashed', 'aborted'))`. But (a) WI-S06-001 §1 declares `enum GcPhase { ..., Failed { phase, error_message, failed_at } }` and migration 006 `CHECK (phase IN (..., 'failed'))` — `'failed'` is a valid **phase**, but (b) WI-S06-002 §1 `MarkError::PhaseBudgetExceeded` and §6.1.10 says "PhaseBudgetExceeded error → status=failed; manual re-run; SEV-2 alert" — but `'failed'` is **not** a valid `status` per the migration CHECK. WI-S06-003 §1 mentions "phase=failed status=aborted" in §8 Gherkin (correct mapping: status='aborted') but inconsistently elsewhere. Either: (a) add `'failed'` to the GcStatus enum + migration CHECK + migration update, or (b) rename WI-S06-002 §6.1.10 to "PhaseBudgetExceeded → status=crashed (worker effectively failed mid-phase) OR status=aborted (graceful failure)". The status state machine is currently underspecified: when a phase fails ungracefully, what's the status? The `Failed` phase variant has rich error metadata but no corresponding status — that's a state machine gap.

**Fifth (P0): WI-S06-003 `BlobState` audit prev_state captures `refcount: u32`, but `blob_meta.refcount` is canonical source for INV-GC-001/004 yet the Rust struct in WI-S06-003 §1 doesn't specify whether refcount is captured **at sweep moment** (post-soft-delete, refcount unchanged) or **as a snapshot at audit emit time**.** The audit emission contract `prev_state` is the forensic trail required by S-09 audit chain integrity. If refcount is racy (between RETURNING and audit_outbox INSERT) then the audit chain shows an inconsistent prev_state. Solution: the D1 UPDATE `RETURNING size_bytes, refcount, last_referenced_at, created_at` captures refcount atomically at the UPDATE moment — that's correct and the WI specifies RETURNING. **But** the WI's `BlobState` struct includes `refcount: u32` while `blob_meta` schema (per S-01) might use `refcount: u64` (need verification — this audit could not confirm; flag as P1 verification). If types mismatch silently, sqlx::query_as! will fail at compile time.

**Sixth (P0): sweep phase budget `5 min @ 100k candidates` calculation is internally inconsistent with sprint contract `10 min p99 @ 1M blobs`.** Sprint contract §5.5 specifies `Mark p99 ≤ 10 min @ 1M blobs`. Mark is the multi-pass scan phase. Sweep is downstream. WI-S06-003 §6.1 says "Phase budget 5 min p99 @ 100k candidates per run (sub-allocation of 10 min sprint contract; mark gets 5 min, sweep gets 5 min)". But sprint contract §5.5 R-S06-5 says "Mark p99 ≤ 10 min para 1M blobs" — that is **mark alone**, not mark+sweep combined. WI-S06-002 §6.1.10 "Phase budget 10 min p99 @ 1M blobs" matches the sprint contract for mark. WI-S06-003 §6.1 then claims "sub-allocation of 10 min sprint contract; mark gets 5 min, sweep gets 5 min" — this is **incorrect arithmetic**: WI-S06-002 has the full 10 min for mark, leaving 0 for sweep under that sprint contract item. Either the sprint contract has a separate sweep budget (need to verify), or one of the two WI budgets needs adjustment. The 5 min @ 100k candidates is plausible on its own merits but needs to be a **separately specified SLO**, not a sub-allocation of mark's 10-min budget.

**Beyond the six P0s:** WI-S06-001's risk register has 12 rows (meets SOTA floor); WI-S06-002 has 12; WI-S06-003 has 12 — all meet the ≥10 floor. Chaos suites: WI-S06-001 has 10 (meets floor), WI-S06-002 has 10 (meets floor), WI-S06-003 has 12 (exceeds — appropriate for the load-bearing WI). 13-row sign-off correctly itemized in all three; Crypto SME advisory in WI-001/002 (defensible — they are infrastructure-and-orchestration, not cripto boundary), MANDATORY EMPHATIC in WI-003 (correctly applied). Mann-Whitney 3-prong middleware-grade `|Δmedian| ≤ 5ms` in WI-003 (correctly graded — sweep is internal observability, not customer-facing crypto path). Cargo-fuzz 1h CI nightly in WI-002 + WI-003 (good). Property tests 10k iter PR + 100k nightly (sprint contract DoD met explicitly in WI-003 §10.s06.003.1). Cost regression gate per-op declared in all three WIs; TCO 12m projection numeric in all three. The compact §9-32 form is **NOT** used in this trio (unlike WI-S05-003 which compressed §9-32) — full SOTA layout is preserved. This trio handles the §9-32 expansion better than S-05 part1 did.

The trio earns a **Part 1 average of 8.13/10** — slightly above the S-05 part 1 average (8.10) but materially **at-or-below WI-S04-003's 8.6 ceiling**. WI-S06-003 lands at **8.4** (strongest; the most load-bearing WI in the program by far, and it is materially the strongest spec — explicit SQL EXISTS, explicit strict-`<` boundary scenario in Gherkin, explicit Crypto SME MANDATORY EMPHATIC, 100k race property test traceable to sprint contract DoD; held back by the JSON column LIKE defect, the column-name drift, and the budget arithmetic), WI-S06-002 lands at **8.0** (strong intent and clean structure; held back by mark_started_at_ms ordering ambiguity vs TLA+, the JSON column query absent from §1 entirely — mark phase scans `ac_meta.outputs[*]` but does not specify the JSON traversal idiom which is the same defect class as WI-003's LIKE, the sweep-phase-budget arithmetic conflict, and a missing phase-completion / phase-abort transition discipline), WI-S06-001 lands at **8.0** (strong scaffolding work; held back by GcStatus/Phase state-machine gap, manual admin trigger surface mostly stubbed, and ADR-0042 forward published but the published file content was not reviewed in this audit because it was generated as part of Lote 10.6 — adversarial review of that ADR is part of WI-007's PRR ship gate). **All three are GO-WITH-FIXES for Lote 10.6bis; none REJECT;** WI-S06-003 must absorb the JSON LIKE rewrite (P0 cripto-load-bearing); WI-S06-002 must absorb the mark_started_at_ms ordering invariant explicit; WI-S06-001 must close the GcStatus/Phase enum/CHECK gap. The single highest-leverage Lote 10.6bis action is **rewriting the WI-S06-003 §6.1.2 SQL EXISTS check from `blob_refs LIKE '%' || $2 || '%'` to `EXISTS (SELECT 1 FROM ac_meta a, json_each(a.blob_refs) j WHERE a.tenant_id = $1 AND j.value = $2 AND a.created_at >= $3)` — this is a 1-line edit that converts an unaudited string-typing-erasure substring match into a JSON-aware membership test that survives schema evolution, is index-friendly via `idx_ac_meta_tenant_created_at` (verify exists in WI-S04-002), and removes the false-positive collision risk class.**

**Aggregate Part 1 score: 8.13/10.**

---

## Per-WI Numerical Score

| WI | Score | Cripto | Complete | Clarity | SOTA | Internal | Prior-WI | Customer | TLA+ | Verdict |
|---|---|---|---|---|---|---|---|---|---|---|
| WI-S06-001 | **8.0** | 7.5 | 8.5 | 8.5 | 8.0 | 7.5 | 8.0 | 8.5 | 8.0 | pass-with-fixes (P0) |
| WI-S06-002 | **8.0** | 7.5 | 8.5 | 8.5 | 8.0 | 7.5 | 7.5 | 8.5 | 7.5 | pass-with-fixes (P0) |
| WI-S06-003 | **8.4** | 8.5 | 9.0 | 8.5 | 9.0 | 8.0 | 7.5 | 9.0 | 9.0 | pass-with-fixes (P0 cripto) |

**Average: 8.13/10.** (S-05 part1 was 8.10; S-04 part1 was 8.05; WI-S04-003 best-in-class 8.6.)

Breakdown axes (0-10):
- **Cripto rigor**: WI-003 leads (explicit TLA+ obligation citation, strict-`<` semantics derived correctly, Crypto SME MANDATORY EMPHATIC, 100k race property test traceable to DoD); held back by the JSON column LIKE defect. WI-001/002 are not cripto-boundary in the same sense (orchestration + scanning), so 7.5 is correct grading.
- **Completeness**: 32 sections in all three; no §9-32 compression. WI-003 has 14 Gherkin scenarios (best in program), 12 chaos, 12 risk. WI-002 has 12 Gherkin, 10 chaos, 12 risk. WI-001 has 10 Gherkin, 10 chaos, 12 risk. All meet sprint contract HIGH_RISK ≥10 chaos floor.
- **Clarity**: WI-001 is operationally readable (cron + degrade + checkpoint flow is concrete); WI-002 is dense with multi-pass scan logic but the §1 narrative is well-structured; WI-003 publishes the SQL EXISTS check inline in §6.1.2 which is **the cleanest cripto rationale in the program** when read in isolation. The JSON LIKE defect undermines the clarity claim.
- **SOTA-adherence**: 13-row sign-off + Mann-Whitney 3-prong + cargo-fuzz 1h + property test 10k+100k + cost regression gate + TCO 12m present in all three. WI-003's MANDATORY EMPHATIC Crypto SME is correctly applied for INV-GC-004 cripto-coordenado boundary; WI-001/002 advisory is defensible.
- **Internal consistency**: WI-001 has the GcStatus/Phase state-machine gap (failed); WI-002 has mark_started_at_ms ordering ambiguity vs TLA+; WI-003 has the JSON LIKE defect + column-name drift + sweep budget arithmetic conflict. All three need P0 fixes.
- **Prior-WI consistency**: ac_meta.created_at vs created_at_ms drift (WI-S04-002 canonical); blob_refs JSON column query semantics (WI-S04-002 § 244 idiom is explicit JSON contains; WI-003 substring LIKE diverges); audit_outbox correctly cited as WI-S01-004 (Lote 10.4bis lesson absorbed); ADR canonical path correct (`specs/03_architecture/adrs/`); INV §3.17 promotion populated.
- **Customer-facing readiness**: persona narratives are concrete in all three (Bazel CI dev invisible, DevOps ops-readiness, on-call incident, customer re-upload undelete in WI-003 — that scenario is the customer-trust anchor for CAP-GC-002 reversibility). SLA addenda explicit. WI-003's "Persona 2 — Customer who deleted file by mistake" is the cleanest customer-empathy frame in the program.
- **TLA+ alignment**: WI-003 leads (explicit `gc_correctness.tla InvGCReRefProtected` citation, strict-`<` derived from `~(\E ... ac_entries[e].created_at >= mark_started_at)` correctly); WI-002 is held back by the ordering ambiguity (TLA+ `MarkPhaseStart` requires capture **before** any `GCMarkStep`; Rust spec doesn't pin commit ordering); WI-001 is good but lacks an explicit TLA+ ↔ Rust action-mapping table.

---

## P0 Findings (must-fix before Lote 10.6bis SEAL)

### P0-1 — WI-S06-003 INV-GC-004 SQL EXISTS check uses `LIKE '%' || $2 || '%'` against JSON column [LOAD-BEARING CRIPTO]

**Severity**: P0 (cripto-load-bearing; the single highest-leverage defect in the trio). **WI**: WI-S06-003 §1 invariant 1, §6.1.2.

The SQL EXISTS check that enforces INV-GC-004 — the single most-feared invariant in the program — is currently:

```sql
SELECT EXISTS (
    SELECT 1 FROM ac_meta
    WHERE tenant_id = $1
      AND blob_refs LIKE '%' || $2 || '%'         -- digest é em ac.outputs
      AND created_at_ms >= $3
) AS reference_after_mark;
```

This is wrong on three independent axes:

1. **Semantic incorrectness vs the canonical schema.** WI-S04-002 §188 establishes `blob_refs TEXT NOT NULL — JSON array of digests`. WI-S04-002 §200 explicitly states D1 supports JSON1 (`SQLite JSON1 extension nativo`). WI-S04-002 §244 says the canonical S-06 reconcile pattern is "`WHERE tenant_id = ? AND blob_refs JSON contains $digest → full scan`". The substring `LIKE` is **not** the canonical idiom and silently breaks if `blob_refs` evolves to be a JSON object envelope, gets a metadata field added that contains digest references for any other reason (e.g., parent_digest), or if any future denormalization adds digest substrings to the JSON.

2. **Performance pathology.** A leading-`%` LIKE forces a full table scan with **no index utility**. Sweep does **one EXISTS per gc_candidate** (per WI-S06-003 §6.1.2). For tenant_A with 100k candidates and 1M `ac_meta` rows, the per-tenant sweep is O(100k × 1M) = O(10^11) row-comparisons. WI-S06-003 §14.s06.003.4 claims `per-sweep p99 ≤ 50ms` — this is **physically impossible** under a leading-`%` LIKE on 1M rows. The performance gate would catch the regression in benchmark, but the spec is currently incoherent on its own SLO.

3. **False-positive collision risk class.** BLAKE3-256 hex digests are 64 chars; any two distinct digests cannot be substrings of one another (different lengths impossible at fixed length 64). But the JSON wrapper introduces `"`, `,`, whitespace, escape sequences. The LIKE `%abcd...%` matches the digest **as JSON value** (correct case) AND **as substring of any longer string**. With pure hex digests at fixed length 64 this is benign in practice, but the spec has no anchoring discipline (no `"DIGEST"` framing; no JSON1 extraction). If any future field stores digest **prefixes** (e.g., a 16-char short digest for display), the LIKE fires false-positive — favorable-to-reachable direction (no data loss), but **noise in the protected_re_ref counter** that breaks the SEV-2 alert threshold "protected_re_ref rate > 5% sustained = mark phase too slow OR concurrent UpdateAR storm" — a false signal that the on-call team will chase.

**Fix** (Lote 10.6bis):

Replace the `LIKE` clause with the canonical D1/SQLite JSON1 idiom:

```sql
SELECT EXISTS (
    SELECT 1 FROM ac_meta a, json_each(a.blob_refs) j
    WHERE a.tenant_id = $1
      AND j.value = $2                            -- exact JSON value match; type-safe
      AND a.created_at >= $3                       -- column name fix: canonical is `created_at`, not `created_at_ms`
) AS reference_after_mark;
```

Performance fix: ensure index `idx_ac_meta_tenant_created_at` exists on `ac_meta(tenant_id, created_at)`. Verify via direct check of WI-S04-002 §1 indexes; if absent, add to migration as a deploy-blocking pre-req.

Add property test `prop_inv_gc_004_json_blob_refs_evolution` that validates the EXISTS check survives a schema change to wrap `blob_refs` in `{"refs": [...], "metadata": {...}}` JSON object — should be a compile-time or migration-time failure, not a silent semantic drift.

Add explicit Crypto SME review checkpoint: the canonical SQL must be reviewed against `gc_correctness.tla InvGCReRefProtected` for semantic equivalence before SEAL.

This fix closes the single most dangerous defect class in the trio. Estimated effort: 2h SQL rewrite + 4h property test + 4h benchmark re-validation + 4h Crypto SME review = ~14h total.

### P0-2 — Column name drift: `created_at_ms` (WI-S06) vs `created_at` (WI-S04-002 canonical)

**Severity**: P0 (compilation-blocking + cross-WI consistency). **WIs**: WI-S06-002 §1 invariant 4, §6.1.4 + §1 SQL `mark_started_at_ms`; WI-S06-003 §6.1.2 SQL `created_at_ms >= ?`, §6.1.3 `RETURNING ... created_at_ms`, §6.1.4 audit emit, §8 Gherkin.

WI-S04-002 (canonical `ac_meta` schema) declares:

```sql
created_at          INTEGER     NOT NULL,
```

(line 78 of WI-S04-002; verified in this audit). The column is named **`created_at`** with implicit unit unix-ms.

WI-S06-002 and WI-S06-003 use `created_at_ms` throughout SQL, Rust struct fields (`ac_created_at_ms: u64`), CHECK constraint comments, audit emit fields, and Gherkin scenarios. `sqlx::query_as!` macro will **fail at compile time** at the load-bearing INV-GC-004 EXISTS check (P0-1's fix is a no-op against this defect — both must be applied together).

Same defect class also applies to:
- `mark_started_at` (TLA+ canonical name) vs `mark_started_at_ms` (WI-S06 canonical) — these are arguably defensible as separate columns in a separate `gc_run` table; but the strict-naming-discipline lesson from S-04 R4 audits suggests pinning the convention now: **all timestamps are stored as INTEGER unix-ms; column name is `<name>_ms`** (which would require renaming `ac_meta.created_at` → `ac_meta.created_at_ms` via migration), OR **all timestamps drop the `_ms` suffix and rely on documentation** (which would require renaming all S-06 references back to `created_at`, `mark_started_at`).

**Fix** (Lote 10.6bis): pick one. Recommended path: **preserve `WI-S04-002` column naming `created_at`** (it's already shipped/SEALED-or-near-SEAL); **rewrite all WI-S06 SQL references to use `ac_meta.created_at`**; **keep WI-S06's own new columns (`gc_run.mark_started_at_ms`, `gc_candidates.mark_started_at_ms`) with the `_ms` suffix** because they are new and naming there is settable now. Document this in a new ADR-0043-timestamp-column-naming-convention OR in WI-S06-001 §9 design decisions.

Estimated effort: 1h SQL rewrite across WI-S06-002 + WI-S06-003 + 1h Gherkin update + 1h migration test = ~3h.

### P0-3 — `mark_started_at_ms` capture ordering vs TLA+ `GCMarkStart` precedence

**Severity**: P0 (TLA+ obligation alignment; ordering invariant is the load-bearing assumption of the entire INV-GC-004 proof). **WI**: WI-S06-002 §1 invariant 1, §6.1.4, §9.4, §17 ST-003.

The TLA+ `gc_correctness.tla` `GCMarkStart` action (lines 102-110) sets `mark_started_at' = now` **as a precondition** of transitioning `gc_phase = "idle" → "marking"`. The first `GCMarkStep` (line 118-129) requires `gc_phase = "marking"`. By TLA+ atomic action semantics, `mark_started_at` is established **strictly before** any blob is visited. This is the load-bearing assumption that makes INV-GC-004's strict-`<` comparison (`ac.created_at >= mark_started_at` triggers protection) work: any AC entry created after `mark_started_at` has `ac.created_at > mark_started_at` and is caught.

WI-S06-002 §1 invariant 1 specifies the SQL UPDATE WHERE IS NULL atomic capture but does **not** specify it must commit **before** the first multi-pass scan batch. WI-S06-002 §6.1.4 (mark_started_at_ms atomic capture) is a sub-task before §6.1.5 (multi-pass scan), but the spec doesn't pin the commit-then-scan ordering at the **D1 transaction boundary** — it pins it only at the **task ordering** level.

**Race scenario**: 
1. Worker invocation begins; reads `mark_started_at_ms IS NULL` from gc_run.
2. Worker issues `UPDATE gc_run SET mark_started_at_ms = T WHERE run_id=X AND mark_started_at_ms IS NULL` — D1 returns success but the WAL is not yet flushed (D1 implementation detail; CF Workers async runtime).
3. Worker begins multi-pass scan; reads `blob_meta` rows. First batch result includes blob B with no current AC ref.
4. Concurrent `UpdateActionResult` fires at T+1ms creating new ac_entry referencing B; AC entry's `created_at = T+1`.
5. Mark step records B in `mark_progress` but NOT in `mark_set` (no AC ref seen).
6. Sweep phase: gc_candidate B has `mark_started_at_ms = T`; SQL EXISTS check `WHERE created_at >= T` finds the AC entry with `created_at = T+1`; PROTECTED. 

The above scenario actually **works** — the EXISTS check catches it because T+1 > T. But the race that **fails** is the **inverted** ordering:

1. Worker issues UPDATE but D1 commit is delayed.
2. Worker begins scan **concurrently** with the UPDATE commit.
3. Concurrent `UpdateActionResult` fires; creates AC entry with `created_at = T-1ms` (before the worker's intended `mark_started_at` but after the **actual scan** began).
4. Worker scan reads blob_meta for B; **does not** see the new AC entry yet (read snapshot pre-AC-entry-commit).
5. UPDATE finally commits with `mark_started_at = T`.
6. Sweep: gc_candidate B has `mark_started_at = T`; EXISTS check `created_at >= T` looks for AC entries — finds one with `created_at = T-1`, which is `< T`, so returns FALSE. Sweep proceeds; B is soft-deleted.
7. **INV-GC-004 violation**: B was re-referenced (AC entry created during the scan) but the EXISTS check missed it because `mark_started_at` was set after scan began.

The TLA+ `GCMarkStart` atomic precondition prevents this by construction. The Rust spec must enforce: **`mark_started_at_ms` UPDATE commits before any blob_meta SELECT in the scan loop**. The current spec allows reordering by async runtime / D1 commit-batching.

**Fix** (Lote 10.6bis):

(a) Specify the ordering invariant explicitly in WI-S06-002 §1 invariant 1 (rename to `INV-GC-MARK-STARTED-AT-COMMIT-PRECEDES-SCAN`):

> The mark phase MUST execute the SQL UPDATE to set `gc_run.mark_started_at_ms` AND wait for D1 commit acknowledgement BEFORE issuing the first SELECT against `blob_meta`, `ac_meta`, or `manifest_chunks` in the multi-pass scan. The `MarkPhase::execute` impl MUST `.await` the UPDATE result, verify the affected_rows count == 1 (or 0 for idempotent re-run reading existing value), and only then begin the scan loop.

(b) Add property test `prop_mark_started_at_commit_precedes_scan` that:
- Forks a concurrent UpdateActionResult fiber.
- Asserts that scan results are deterministic with respect to the UPDATE commit boundary.
- 100 random interleavings; INV-GC-004 violation count must be 0.

(c) Add chaos test `chaos_mark_started_at_commit_race`: simulate D1 commit delay (mock D1 with 100ms commit latency); verify worker blocks on commit before scan begins.

(d) Document the TLA+ ↔ Rust action mapping table in WI-S06-002 §1:

| TLA+ action | Rust impl | Ordering invariant |
|---|---|---|
| `GCMarkStart` | `UPDATE gc_run SET mark_started_at_ms = ... WHERE IS NULL` + `.await` commit | Commits before scan begins |
| `GCMarkStep(b)` | Multi-pass scan batch reads `blob_meta` / `ac_meta` / `manifest_chunks` | Reads after `mark_started_at_ms` commit |
| `GCMarkToSweep` | gc_run.phase = 'sweep'; mark phase ends | Atomic D1 UPDATE |
| `GCSweepBlob(b)` | sweep phase EXISTS check + soft-delete + audit emit | D1 batch atomic |

This table is **the canonical TLA+ ↔ Rust contract** and should be in every GC WI's §1 narrative.

Estimated effort: 4h spec rewrite + 8h property test + 4h chaos test + 4h ADR-0042 update = ~20h.

### P0-4 — `GcStatus` enum + migration CHECK constraint missing `'failed'` variant; status state-machine gap

**Severity**: P0 (state machine correctness; underspecified failure mode). **WIs**: WI-S06-001 §1 (Rust enum + SQL CHECK), §6.1.4, §9.x; WI-S06-002 §6.1.10 (PhaseBudgetExceeded → status=failed); WI-S06-003 §1 SweepError, §8 Gherkin "phase=failed status=aborted".

WI-S06-001 §1 declares:
```rust
pub enum GcStatus {
    Pending, Running, Succeeded, Crashed, Aborted,
}
```
and migration 006:
```sql
CHECK (status IN ('pending', 'running', 'succeeded', 'crashed', 'aborted'))
```

But:
- WI-S06-002 §6.1.10: "PhaseBudgetExceeded error → status=**failed**" — `'failed'` is not a valid status per the enum or the CHECK.
- WI-S06-003 §8 Gherkin scenario "Degrade-mode gc-pause": "worker transitions phase=**failed** status=**aborted**" — phase='failed' is a valid `GcPhase`, status='aborted' is valid; this Gherkin is consistent.
- WI-S06-002 §6.1.16 chaos #5: "Tenant size 5M blobs → phase budget exceeded → SEV-2 alert" — implicit status mapping; not specified.

State machine is currently:
- **Phase**: idle → mark → sweep → physical_delete → reconcile → completed | failed (failure capturing the phase that failed).
- **Status**: pending → running → (succeeded | crashed | aborted).

The "phase failed; status?" question has no canonical answer. PhaseBudgetExceeded is not a crash (worker is alive, it ran out of budget) and not aborted (no operator intervention). The best fit would be `status='failed'` (new variant) OR `status='crashed'` (overload semantics: any non-graceful termination, including budget exceedance). 

**Fix** (Lote 10.6bis):

(a) Decide: add `Failed` variant to `GcStatus` enum or overload `Crashed`?

Recommendation: **add `Failed` variant** with semantics "phase exceeded budget OR returned an error that did not result in worker process death". `Crashed` retains semantics "worker process died mid-phase; restart needed". `Aborted` retains semantics "operator-initiated halt via degrade-mode". This gives a clean 6-state status machine.

(b) Migration update:
```sql
CHECK (status IN ('pending', 'running', 'succeeded', 'crashed', 'aborted', 'failed'))
```

(c) WI-S06-002 §6.1.10 update: "PhaseBudgetExceeded → status=failed; phase=failed; failed_reason='phase_budget_exceeded'; failed_phase='mark'".

(d) Add explicit Gherkin scenario in WI-S06-001 §8: "Phase failed status mapping" with truth table of (phase, status) → next-state.

(e) Add property test `prop_gc_status_state_machine`: 1000 random transitions; only valid transitions accepted; invalid transitions rejected.

Estimated effort: 2h migration update + 2h Rust enum update + 2h Gherkin/property test = ~6h.

### P0-5 — Sweep phase budget arithmetic conflicts with sprint contract §5.5

**Severity**: P0 (SLO arithmetic). **WIs**: WI-S06-002 §6.1.10 (`Phase budget 10 min p99 @ 1M blobs`); WI-S06-003 §6.1 (`Phase budget 5 min p99 @ 100k candidates per run; sub-allocation of 10 min sprint contract`).

Sprint contract §5.5 R-S06-5: "Mark p99 ≤ 10 min para 1M blobs (tenant size real); benchmark CI." This is mark alone.

WI-S06-002 §6.1.10 takes the full 10 min for mark. WI-S06-003 §6.1 then claims it has 5 min as "sub-allocation of 10 min sprint contract; mark gets 5 min, sweep gets 5 min" — this is incorrect arithmetic. Either (a) sweep needs a separate SLO line in the sprint contract (likely the right answer; sweep is a different phase with different cost model), or (b) WI-S06-002 must reduce its mark budget to 5 min (which is a 2× tightening that may not be benchmark-feasible at 1M blobs).

**Fix** (Lote 10.6bis):

(a) Add explicit sprint contract §5.5 R-S06-X line: "Sweep p99 ≤ 5 min @ 100k candidates per run (not a sub-allocation of mark budget; separately budgeted)".

(b) WI-S06-003 §6.1 rewrite: "Phase budget 5 min p99 @ 100k candidates (sprint contract §5.5 separately budgeted from mark's 10 min @ 1M blobs)".

(c) Add benchmark `bench_sweep_100k_candidates` to WI-S06-003 §13 artifacts (parallel to WI-S06-002's `bench_mark_1m_blobs`).

Estimated effort: 1h spec rewrite + 6h benchmark = ~7h.

### P0-6 — WI-S06-002 mark phase scan also has the JSON `blob_refs` query problem

**Severity**: P0 (same defect class as P0-1; likely silently inherited). **WI**: WI-S06-002 §1 invariant 2, §6.1.3 multi-pass scan (does not publish SQL).

WI-S06-002 §1 invariant 2: "Reachable set união: `union(blob_meta WHERE refcount > 0, ac_meta.outputs[*], manifest_chunks WHERE blob_digest IN cas_blobs)`. Tenant-scoped strict (Lote 10.4bis lesson)."

The expression `ac_meta.outputs[*]` is a TLA+-style array spread, not SQL. The actual SQL needed to enumerate AC reachability is:

```sql
SELECT DISTINCT j.value AS digest
FROM ac_meta a, json_each(a.blob_refs) j
WHERE a.tenant_id = $1
  AND a.deleted_at IS NULL  -- if ac_meta has soft-delete column
```

WI-S06-002 §6.1.3 lists "Multi-pass scan: 3 passes (blob_meta + ac_meta + manifest_chunks) tenant-scoped strict" but **does not publish the SQL** for the ac_meta pass. The mark phase therefore ships incoherent on the most important reachability query — and worse, if a downstream implementer copies the WI-S06-003 LIKE pattern, the mark phase reachable set will be **wrong** (substring matches against JSON producing false-reachable, which is in the safe direction — favorable to reachable — but introduces noise in the candidates_count metric).

The "superset-safe (false reachable acceptable; false orphan unacceptable)" claim in §9.6 is correct as a design principle, but it tolerates buggy SQL rather than specifying correct SQL.

**Fix** (Lote 10.6bis):

(a) WI-S06-002 §6.1.3 rewrite to publish the canonical SQL for the ac_meta pass:

```sql
-- Pass 2: ac_meta reachability (extract all blob digests referenced via blob_refs JSON)
SELECT DISTINCT j.value AS digest
FROM ac_meta a, json_each(a.blob_refs) j
WHERE a.tenant_id = $1
  AND (a.expires_at IS NULL OR a.expires_at > unixepoch_ms())
ORDER BY a.action_digest, j.key
LIMIT 250 OFFSET ?  -- batch boundary
```

Note: per WI-S04-002 schema there is no `ac_meta.deleted_at` (ac_meta uses `expires_at` for lifecycle); verify and align.

(b) Add property test `prop_mark_ac_meta_pass_jsonsql` validating the JSON traversal correctness against the WI-S04-002 canonical schema.

(c) Add explicit comment in §1: "The ac_meta pass uses SQLite/D1 `json_each()` — NOT `LIKE`-substring (lesson learned WI-S06-003 P0-1)."

Estimated effort: 2h SQL publish + 4h property test = ~6h.

---

## P1 Findings

### P1-1 — WI-S06-001 INV-GC-MARK-STARTED-AT-IMMUTABLE vs WI-S06-002 INV-GC-MARK-STARTED-AT-ATOMIC: invariant duplication in registry

**Severity**: P1 (registry hygiene). **WIs**: WI-S06-001 §12 declares `INV-GC-MARK-STARTED-AT-IMMUTABLE`; WI-S06-002 §12 declares `INV-GC-MARK-STARTED-AT-ATOMIC`; both populated in `invariant_registry.md §3.17` lines 334 and 336.

The two INVs say semantically near-identical things:
- IMMUTABLE: "captured ONCE atomically; immutable post-capture"
- ATOMIC: "SQL UPDATE WHERE IS NULL atomic capture; idempotent re-run preserves"

They are not orthogonal. "Immutable" is a property OF the timestamp (it doesn't change after capture). "Atomic" is a property of the capture mechanism (SQL UPDATE WHERE IS NULL). Both are needed — but they should be a single composite INV with two clauses, not two INVs.

**Fix** (Lote 10.6bis): merge into `INV-GC-MARK-STARTED-AT-CAPTURE` with composite definition: "(a) atomic capture via SQL UPDATE WHERE IS NULL idempotent; (b) immutable post-capture; (c) commits before any scan SELECT (P0-3 ordering invariant)". Update both WIs and the registry. Run `validate_inv_promotion.py` to confirm.

### P1-2 — WI-S06-001 manual admin trigger surface fully stubbed (501) with ambiguous staging-only semantics

**Severity**: P1 (operational readiness). **WI**: WI-S06-001 §6.1.6, §23.

§6.1.6: "Manual trigger admin API stub (`POST /v1/admin/gc/trigger?tenant_id=X&region=Y`); S-13 admin plane forward; staging stub returns 501 in non-staging envs."

This is defensible (S-13 is the canonical admin plane; rolling stubs into S-06 is the right call), but the spec is ambiguous on:
- Does the stub authorize PAT scope `gc:trigger`? §8 Gherkin says "Given admin PAT with gc:trigger scope" — but PAT scope `gc:trigger` is not registered in any S-03 PAT scope catalog reference. Verify.
- Does the stub emit audit events? §6.1.8 says "Audit emission outbox: `cas.gc.manual_trigger`" — but the stub is a 501. A 501 stub should not be emitting `manual_trigger` events; it should be emitting `manual_trigger_unsupported`.
- Does the stub enforce per-tenant rate limit? §15 chaos #6 says "Manual trigger flood → rate limit S-13 forward enforces" — but if the stub is 501, there is nothing to flood. The chaos test as specified is unimplementable in this WI's deliverable scope.

**Fix** (Lote 10.6bis): clarify the staging-stub contract:
(a) Staging env: stub authorizes PAT scope `gc:trigger`, executes the worker (real path), emits `cas.gc.manual_trigger` audit event.
(b) Non-staging envs: stub returns 501 + emits `cas.gc.manual_trigger_unavailable` audit event for forensic.
(c) Rate limit: in staging, naive 10/min per tenant; in non-staging, N/A (501 path).
(d) §15 chaos #6 must be staging-only; mark accordingly.

### P1-3 — WI-S06-001 + WI-S06-002 do not specify tenant scoping for cron tick

**Severity**: P1 (multi-tenant correctness). **WIs**: WI-S06-001 §1 ScheduleConfig; WI-S06-002 §6.1.

Cron fires per-region (5 regions). Within a region, the worker iterates over all tenants. Concurrency is `max_concurrent_tenants: u32 = 4`. But the spec doesn't specify:
- **Tenant ordering**: alphabetical? hash-shuffled? round-robin? If alphabetical, tenant `aaa-corp` always sweeps first, tenant `zzz-corp` always last — for a 1000-tenant environment with a 30-min worker budget, the late-alphabet tenants may consistently miss the daily window.
- **Per-tenant timeout**: if tenant_A's mark phase exceeds 10 min, do we abort and move to tenant_B, or block the queue?
- **Cross-tenant isolation**: 4 concurrent tenants — are they on 4 separate D1 connections (with separate read snapshots), or sharing one connection? D1's CF Workers deployment has connection-pooling semantics that affect this.

**Fix** (Lote 10.6bis): WI-S06-001 §6.1 add explicit tenant-iteration discipline:
- Hash-shuffle ordering using `hash(tenant_id, gc_run.run_id)` as sort key (avoids alphabetical bias; deterministic per-run; reorderable across runs).
- Per-tenant timeout = phase budget (10 min mark + 5 min sweep + ...); on timeout, mark gc_run as failed, move to next tenant.
- 4 concurrent tenants on 4 separate D1 connections; document explicitly in §9 design decisions.

### P1-4 — WI-S06-003 BlobState.refcount type unspecified vs WI-S01-001 blob_meta canonical

**Severity**: P1 (Rust/SQL type coherence; sqlx::query_as! compile-time check). **WI**: WI-S06-003 §1 BlobState struct.

§1 declares:
```rust
pub struct BlobState {
    pub digest: String,
    pub size_bytes: u64,
    pub refcount: u32,                          // captured at sweep moment for audit
    pub last_referenced_at_ms: u64,
    pub created_at_ms: u64,
}
```

This audit could not verify the canonical `blob_meta` schema (WI-S01-001 was not in scope). If `blob_meta.refcount` is `INTEGER` (SQLite stores as i64) and Rust uses `u32`, then a tenant with > 2^32 references on a hot blob would overflow the audit prev_state forensic. Unlikely at scale today (2^32 = 4.3B references is well above any realistic hot-blob ceiling) but worth pinning.

**Fix** (Lote 10.6bis): align BlobState.refcount with `blob_meta.refcount` SQL type. If SQL is INTEGER, Rust should be `i64` or `u64`. Verify against WI-S01-001/002.

### P1-5 — WI-S06-002 bench `bench_mark_1m_blobs` has no fixture-prep cost accounting

**Severity**: P1 (CI cost). **WI**: WI-S06-002 §10.s06.002.2, §13 artifact `bench_mark_1m_blobs`.

A criterion benchmark that creates 1M blob_meta + ac_meta + manifest_chunks fixtures per run will (a) take many minutes to prep, (b) write significant data to D1 in CI (cost regression — D1 write is $1/M ops), (c) require teardown. The WI specifies the benchmark target (≤ 10 min p99) but not the fixture-prep cost gate.

**Fix** (Lote 10.6bis): add fixture-prep budget to §22 cost analysis and §14 quality standards. Recommended: criterion bench uses **synthetic in-memory fixtures** (mock D1) for routine CI runs; **real D1 fixture-prep** only for nightly + pre-release verification.

---

## P2 Findings

### P2-1 — WI-S06-002 §10.s06.002.7 manifest chunks 3-hop traversal absent SQL example

The spec says "Manifest chunks 3-hop traversal validated (S-05 multipart integration)" but does not publish the join SQL. This is operational transparency missing.

### P2-2 — WI-S06-001 ADR-0042 forward published but content not adversarially reviewed in this audit

ADR-0042 was generated as part of Lote 10.6 (verified to exist at `specs/03_architecture/adrs/ADR-0042-gc-worker-scheduler.md`). Adversarial review of the ADR content is part of the WI-S06-007 PRR ship gate, not this trio's review. Flag for Lote 10.6 ship-gate review.

### P2-3 — WI-S06-001 risk register R-007 ("phase transition race atomic D1 UPDATE") and R-001 ("two concurrent runs same (tenant, region)") overlap

Both rows describe similar race scenarios with the same mitigation (partial UNIQUE WHERE status='running'). Consolidate into a single risk row; reuse the ID or split semantically.

### P2-4 — WI-S06-003 §27 Knowledge Transfer "External-facing blog post" leaks defense-in-depth strategy

`"How CoreLink prevents GC data loss: TLA+ + property test 100k race + audit fail-closed"` — this is a competitive advantage marketing post, but it also signals to adversaries that the program's defense relies on TLA+ + property test + audit. Consider whether the blog post should be released after S-20 GA or held until BYOK (S-14) ships.

### P2-5 — WI-S06-002 §15 chaos suite lists 10 scenarios but §6.1.16 lists 10 — both labeled "10". Sub-counting verified consistent.

### P2-6 — WI-S06-003 §17 ST-016 "Crypto SME review iter (TLA+ obligation alignment)" budgets 4h — Crypto SME independent review of TLA+ + property test 100k + SQL EXISTS + canonical_bytes equivalence likely needs 16-24h. Same staffing-realism criticism as S-05 part2 audit.

### P2-7 — WI-S06-003 §6.1.7 audit emission events list 4 specific events but §6.1.8 metrics list `corelink.gc.sweep.audit_emit_failed_total` — that metric is tied to the fail-closed path; verify `corelink.gc.sweep.audit_emit_succeeded_total` (or equivalent counter for the happy path) is also tracked for ratio observability.

### P2-8 — WI-S06-002 + WI-S06-003 use `unixepoch_ms()` SQL function — verify D1 supports this (SQLite `unixepoch()` returns seconds; `unixepoch('now', 'subsec')` returns seconds with milliseconds fractional; `unixepoch('now') * 1000` is the canonical ms). Cross-check the actual D1 dialect support.

---

## Cross-WI Consistency Check (handoffs WI-001 → WI-002 → WI-003)

| Handoff | Status | Notes |
|---|---|---|
| **WI-001 `gc_run` table** → **WI-002 mark phase reads** | OK | WI-002 §1 reads `gc_run.mark_started_at_ms` correctly; sets atomically via SQL UPDATE WHERE IS NULL. |
| **WI-001 `gc_run.last_checkpoint_at_ms`** → **WI-002 + WI-003 idempotent resume** | OK | Both WIs reference checkpoint; WI-002 §8 Gherkin "Worker crash mid-batch resume" specifies resume-at-batch boundary; WI-003 §8 Gherkin "Worker crash mid-sweep idempotent resume" specifies AlreadySwept no-op. |
| **WI-001 `degrade-mode probe per batch`** → **WI-002 + WI-003 batch loop** | PARTIAL | WI-001 §6.1.5 says "PAT-DEGRADE-001"; WI-002 §6.1 does not list the per-batch degrade probe in §6.1 In-scope; WI-003 §6.1 also does not list it. Both should include per-batch degrade probe as explicit sub-task. |
| **WI-002 `mark_started_at_ms` capture** → **WI-003 EXISTS check** | OK SEMANTICALLY but P0-3 ordering risk | The atomic capture + EXISTS check pair is correct in spec; the ordering invariant (commit-before-scan) must be made explicit per P0-3. |
| **WI-002 `gc_candidates` output** → **WI-003 sweep input** | OK | WI-002 §1 GcCandidate matches WI-003 §1 sweep input. `mark_run_id FK` linking is consistent. |
| **WI-002 `gc_candidates.status='candidate'`** → **WI-003 sets `status='swept'` or `'protected_re_ref'`** | OK | Status state machine on gc_candidates: candidate → swept | protected_re_ref → physically_deleted (WI-S06-004 forward). Migration 007 CHECK supports all 4 states. |
| **WI-001 `audit_outbox` (WI-S01-004)** → **WI-002 + WI-003 audit emit** | OK | All three WIs cite WI-S01-004 audit_outbox correctly; Lote 10.4bis lesson absorbed. |
| **WI-001 `partial UNIQUE WHERE status='running'`** → **WI-002 + WI-003 phase transitions** | OK | Single running gc_run per (tenant, region); phase transitions atomic D1 UPDATE. |
| **WI-002 phase budget `10 min @ 1M blobs`** + **WI-003 phase budget `5 min @ 100k candidates`** → **sprint contract §5.5** | INCONSISTENT (P0-5) | Sweep budget needs separate sprint contract line; not sub-allocation of mark budget. |
| **WI-001 GcStatus enum** → **WI-002 + WI-003 status setting** | INCONSISTENT (P0-4) | `'failed'` not in enum/CHECK; WI-002 §6.1.10 sets it. |
| **WI-S01-001 `blob_meta` schema** → **WI-003 BlobState audit prev_state** | UNVERIFIED (P1-4) | refcount type alignment needs cross-check. |
| **WI-S04-002 `ac_meta.created_at`** → **WI-002 + WI-003 SQL `created_at_ms`** | INCONSISTENT (P0-2) | Column name drift; SQL won't compile. |
| **WI-S04-002 `ac_meta.blob_refs` JSON** → **WI-003 `LIKE '%' || $2 || '%'`** | INCORRECT IDIOM (P0-1) | Use `json_each()` JSON1 idiom. |
| **TLA+ `gc_correctness.tla`** → **WI-002 + WI-003 semantics** | OK SEMANTICALLY | Strict-`<` derivation correct; `~(\E... created_at >= mark_started_at)` ↔ `NOT EXISTS ... created_at >= mark_started_at_ms` ↔ Rust SweepDecision::ProtectedReRef. The `T == T` boundary scenario in WI-003 §8 is correct (T >= T true → protected). Held back by P0-3 ordering invariant gap. |
| **invariant_registry.md §3.17** | OK | All declared INVs populated; validate_inv_promotion.py would pass (modulo P1-1 INV duplication). |

---

## Comparison to Canonical SOTA Bar (WI-S04-003 best-in-class 8.6)

WI-S04-003's score (8.6) was earned by:
- Clean TLA+ ↔ Rust mapping for INV-AC-MERKLE-DETERMINISTIC.
- Published canonical_bytes layout (the lesson Lote 10.5bis later codified).
- Explicit Crypto SME mandatory sign-off with concrete review checkpoints.
- Internal consistency across §1 / §6 / §12 (no contradictions).
- Cargo-fuzz 1h with two targets.
- Mann-Whitney 3-prong cripto-grade (|Δmedian| ≤ 0.5ms).

**WI-S06-003 misses the 8.6 bar by 0.2 points** primarily because:
1. The SQL EXISTS check uses an unaudited string-typing-erasure substring match (P0-1) — WI-S04-003's canonical_bytes are explicitly published; WI-S06-003's load-bearing SQL is implicitly broken.
2. Column-name drift `created_at_ms` vs canonical `created_at` (P0-2) — WI-S04-003's schema column references match canonical.
3. The TLA+ ↔ Rust action mapping is implicit (P0-3 gap) — WI-S04-003 has explicit canonical_bytes byte-equal CI test.
4. Mann-Whitney is middleware-grade (|Δmedian| ≤ 5ms) — defensible (sweep is internal); WI-S04-003 was cripto-grade (|Δmedian| ≤ 0.5ms) for the AC sig path.

**WI-S06-002 misses by 0.6 points** primarily because:
1. mark_started_at_ms ordering ambiguity vs TLA+ (P0-3) — WI-S04-003 had no equivalent ordering ambiguity.
2. JSON `blob_refs` query SQL not published (P0-6) — WI-S04-003 published canonical_bytes inline.
3. Sweep phase budget arithmetic conflict (P0-5) — WI-S04-003 had no equivalent contract drift.

**WI-S06-001 misses by 0.6 points** primarily because:
1. GcStatus/Phase state-machine gap (P0-4) — WI-S04-003's state machine was complete.
2. Manual admin trigger surface fully stubbed with semantic ambiguity (P1-2) — WI-S04-003 had no equivalent stub-vs-real boundary.
3. Tenant-scoping discipline for cron iteration unspecified (P1-3) — WI-S04-003 had no equivalent multi-tenant orchestration scope.

**Path to 9.0+ for next iteration**: close P0-1 + P0-2 + P0-3 (the cripto-load-bearing trio), publish the TLA+ ↔ Rust action-mapping table in WI-S06-002 §1, and elevate Mann-Whitney to cripto-grade for the EXISTS check timing path (since INV-GC-004 is cripto-coordenado boundary). Estimated post-fix score: WI-S06-003 → 9.0; WI-S06-002 → 8.7; WI-S06-001 → 8.5.

---

## TLA+ ↔ Rust Alignment

**Verified against `specs/tla/gc_correctness.tla` (read in full, 225 lines)**:

| TLA+ construct | Rust impl per WI-S06 trio | Alignment |
|---|---|---|
| `mark_started_at` (single timestamp variable) | `gc_run.mark_started_at_ms` (single column per gc_run) + `gc_candidates.mark_started_at_ms` (denormalized per candidate) | ✅ Semantically aligned. The denormalization is fine because gc_candidates rows are immutable post-mark-phase. |
| `GCMarkStart`: `mark_started_at' = now` (atomic action) | `UPDATE gc_run SET mark_started_at_ms = unixepoch_ms() WHERE run_id=X AND mark_started_at_ms IS NULL` | ⚠️ Atomicity OK; **ordering relative to scan SELECT NOT pinned** (P0-3). |
| `GCMarkStep(b)`: visit one blob, conditionally add to mark_set | Multi-pass scan (3 passes batched 250 rows + jitter 100ms) | ✅ Aligned at the granular level; cycle detection in manifest_chunks pass per WI-005 reuse. |
| `GCSweepBlob(b)` enabling: `~(\E e: ac_entries[e].created_at >= mark_started_at)` | SQL `NOT EXISTS (SELECT 1 FROM ac_meta a, json_each(a.blob_refs) j WHERE a.tenant_id=? AND j.value=? AND a.created_at >= ?)` | ⚠️ Semantically aligned; **idiom currently LIKE-substring not json_each (P0-1); column name drift (P0-2)**. |
| `GCSweepBlob(b)` enabling: `now - blob_meta[b].last_referenced_at >= GracePeriod` | Grace period enforced at **physical-delete** (WI-S06-004), NOT at sweep | ⚠️ TLA+ models this at sweep step; Rust splits into sweep (soft-delete, no grace check) + physical-delete (post-grace check). The mapping is defensible (soft-delete is reversible; physical-delete is the irreversible step that needs grace) but the **TLA+ ↔ Rust action mapping is not documented**. Lote 10.6bis: add explicit table per P0-3 fix (d). |
| `InvGCReachableNeverDeleted` | INV-GC-001 enforced via SQL EXISTS (P0-1) + soft-delete reversibility + audit fail-closed | ✅ Semantically; held back by P0-1. |
| `InvGCReRefProtected` | INV-GC-004 enforced via SQL EXISTS strict-`>=` returning protected_re_ref status | ✅ Strict-`<` derivation correct (`NOT EXISTS ... >= mark_started_at` ≡ `ALL ... < mark_started_at`); `T==T` boundary scenario correct in §8 Gherkin (T >= T → protected). |
| `physically_deleted` set | `cas_blobs` row purged + R2 DeleteObject (WI-S06-004 forward; out of scope this trio) | ✅ Out of scope. |

**TLA+ → Rust translation gaps to close in Lote 10.6bis**:
1. **Ordering invariant** (P0-3): `mark_started_at_ms` UPDATE commits before any scan SELECT.
2. **Action mapping table** in WI-S06-002 §1 (or §9 design decisions): TLA+ action → Rust impl → ordering invariant.
3. **Grace-vs-soft-delete semantic split** documentation: TLA+ `GCSweepBlob` ≡ Rust sweep (soft-delete) + Rust physical-delete (post-grace) composed.

---

## Verdict per WI

### WI-S06-001 — `8.0 / 10` — **GO-WITH-FIXES (P0)**

Strong scaffolding and degrade-mode design; partial UNIQUE WHERE status='running' correctly applied; CHECK constraints inline; ADR-0042 forward published. P0 fixes: state-machine gap (`failed` status missing), manual admin trigger semantic ambiguity (P1-2), tenant-scoping discipline (P1-3). ADR-0042 content review deferred to WI-007 PRR.

### WI-S06-002 — `8.0 / 10` — **GO-WITH-FIXES (P0)**

Strong intent and clean structure; TLA+ obligation cited correctly. P0 fixes: mark_started_at_ms ordering invariant explicit (P0-3), JSON `blob_refs` SQL published (P0-6), column-name drift (P0-2), sweep budget arithmetic (P0-5).

### WI-S06-003 — `8.4 / 10` — **GO-WITH-FIXES (P0 cripto-load-bearing)** — strongest of trio

The most load-bearing WI in the program; correctly flagged with Crypto SME MANDATORY EMPHATIC; explicit strict-`<` derivation; 100k race property test traceable to sprint contract DoD; audit fail-closed correctly framed. P0 fixes: SQL EXISTS rewrite from LIKE to json_each (P0-1; THE highest-leverage fix in Lote 10.6bis), column-name drift (P0-2), sweep budget separately specified (P0-5). With these fixes, this WI lands at projected **9.0 / 10**.

---

## Aggregate Part 1 Score

**8.13 / 10.**

Slightly above S-05 part1 (8.10), within the same band as the S-04/S-05 part-1 averages. WI-S06-003 is materially the strongest single WI (8.4) but does not yet clear the WI-S04-003 ceiling (8.6). The trio is **GO-WITH-FIXES for Lote 10.6bis**; no WI is REJECT.

---

## Lote 10.6bis P0 Fix Plan

| ID | Title | WI | Severity | Effort | Owner |
|---|---|---|---|---|---|
| **P0-1** | Rewrite WI-S06-003 SQL EXISTS check from LIKE to `json_each()` | WI-S06-003 | P0 cripto-load-bearing | ~14h | Engineer + Crypto SME |
| **P0-2** | Resolve column-name drift `created_at_ms` vs canonical `created_at` | WI-S06-002 + WI-S06-003 | P0 compile-blocking | ~3h | Engineer |
| **P0-3** | Specify mark_started_at_ms commit-precedes-scan ordering invariant + TLA+ ↔ Rust action mapping table | WI-S06-002 (+ WI-S06-001 minor) | P0 TLA+ obligation | ~20h | Architect + Engineer |
| **P0-4** | Add `'failed'` to GcStatus enum + migration CHECK; pin status state machine | WI-S06-001 | P0 state machine | ~6h | Engineer |
| **P0-5** | Add sprint contract §5.5 sweep budget separately from mark; rewrite WI-S06-003 §6.1 | WI-S06-003 + sprint contract | P0 SLO arithmetic | ~7h | Architect |
| **P0-6** | Publish ac_meta JSON traversal SQL in WI-S06-002 mark phase pass 2 | WI-S06-002 | P0 same-defect-class as P0-1 | ~6h | Engineer |
| **P1-1** | Merge `INV-GC-MARK-STARTED-AT-IMMUTABLE` + `INV-GC-MARK-STARTED-AT-ATOMIC` into composite INV | registry + WIs | P1 hygiene | ~1h | Architect |
| **P1-2** | Clarify manual admin trigger staging-stub contract | WI-S06-001 | P1 ops | ~3h | Engineer + SRE |
| **P1-3** | Specify tenant-iteration ordering / per-tenant timeout / D1 connection isolation for cron | WI-S06-001 | P1 multi-tenant | ~4h | Architect |
| **P1-4** | Verify BlobState.refcount type vs blob_meta SQL | WI-S06-003 + WI-S01-001 | P1 type coherence | ~1h | Engineer |
| **P1-5** | Add bench fixture-prep cost gate; mock-D1 default | WI-S06-002 | P1 CI cost | ~3h | Engineer |

**Total Lote 10.6bis P0 fix budget**: ~68h (within a 1-week budget for one engineer + Architect + Crypto SME pair-review of P0-1).

**Highest-leverage single fix**: P0-1 (SQL EXISTS rewrite). Estimated post-fix score: WI-S06-003 → 9.0, trio average → 8.5+.

**Crypto SME mandatory non-waivable** for P0-1 fix review (TLA+ obligation alignment + canonical_bytes-equivalent SQL semantic check + adversarial review of json_each performance characteristics under D1 query planner).

---

## Reviewer Notes (process)

- TLA+ spec read in full (225 lines verified); `InvGCReRefProtected` semantics aligned with WI-S06-003 strict-`<` derivation.
- WI-S04-002 schema cross-checked for `ac_meta.created_at` (line 78) and `blob_refs JSON` framing (lines 73, 188, 200, 244).
- ADR canonical path verified at `specs/03_architecture/adrs/`; ADR-0042 exists at the canonical path.
- Invariant registry §3.17 verified populated with 22 new GC INVs (preemptive Lote 10.6 promotion).
- Audit baselines compared: S-05 part1 (8.10), S-05 part2 (8.05), S-04 part1 (8.05), WI-S04-003 (8.6 best-in-class).
- **Single most dangerous defect**: WI-S06-003 §6.1.2 SQL EXISTS uses LIKE substring against JSON column. **Single highest-leverage fix**: rewrite to json_each() — closes a cripto-load-bearing defect class with a 1-line SQL change.

**Fim audit Agent R4 Lote 10.6 S-06 Part 1.**
