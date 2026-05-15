---
id: "AUDIT-2026-05-15-R4-OPUS-S06-PART1"
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
reviewer: "R4 Opus persona (Claude Opus 4.7, 1M ctx, adversarial independent reviewer — Wave 14 Lote 10.6 review dispatch)"
scope: "Lote 10.6 — Sprint S-06 Part 1 (WI-S06-001 .. WI-S06-004) on FROZEN v1.3.0 corpus post Lote 10.6bis + 10.6-tris remediation"
sprint_contract: "specs/04_sprints/S06/_spec_contract.md v2.0.0"
calibration_baselines:
  - "specs/_audits/2026-04-25-agent-r4-s06-part1-wi-review.md (S-06 part1 pre-bis 8.13/10)"
  - "specs/_audits/2026-04-25-sonnet-r5-s06-wi-review.md (post-bis 9.1/10 target)"
  - "WI-S04-003 best-in-class 8.6"
files_reviewed:
  - "specs/04_sprints/S06/work_items/WI-S06-001-worker-gc-binary-scheduler-degrade-mode.md (649 lines, v1.3.0 FROZEN)"
  - "specs/04_sprints/S06/work_items/WI-S06-002-mark-phase-multi-pass-scan-mark-started-at.md (653 lines, v1.3.0 FROZEN)"
  - "specs/04_sprints/S06/work_items/WI-S06-003-sweep-phase-soft-delete-inv-gc-004.md (663 lines, v1.3.0 FROZEN)"
  - "specs/04_sprints/S06/work_items/WI-S06-004-physical-delete-post-grace-r2-idempotent.md (463 lines, v1.3.0 FROZEN)"
cross_references:
  - "specs/04_sprints/S06/_spec_contract.md v2.0.0 (FROZEN AUDITED)"
  - "specs/04_sprints/S06/PRR-S06.md"
  - "specs/tla/gc_correctness.tla (Lote 5.13 + 7.1; L152-154 protect-if-`>=`)"
  - "specs/03_architecture/invariant_registry.md §3.17"
  - "specs/03_architecture/adrs/ADR-0042 (TLC SHA + scope limitations)"
  - "crates/corelink-gc/ (worker shipped v0.1.0..v2.0.0)"
tags: ["audit", "r4", "opus", "lote-10.6", "s-06", "wave-14", "dispatch", "review", "part1"]
---

# R4 Opus — Lote 10.6 S-06 Part 1 (WIs 001–004) Adversarial Review

> **Reviewer**: R4 Opus persona — independent adversarial reviewer; no diplomacy; cite framework principles for every finding.
> **Calibration**: WI corpus is post-Lote 10.6bis + 10.6-tris (21 P0 fixes already absorbed; v1.3.0 FROZEN/AUDITED). Findings here must be calibrated to a SEALED corpus — expect P0 density near zero unless a regression was introduced; expect P1 density low; primary surface is P2/P3 residual + deferred trait-abstraction items + ship-gate alignment + adversarial scenarios not yet executed.

---

## §1 Summary table

| WI | P0 | P1 | P2 | P3 | Verdict | One-line note |
|---|---|---|---|---|---|---|
| WI-S06-001 (worker + scheduler + degrade-mode) | 0 | 1 | 3 | 2 | APPROVE — minor P1 on charter `trait-abstraction-defer` boundary documentation | GcStatus 6-variant taxonomy resolved (Lote 10.6-tris); scheduler InMemory fake clean; ScheduleClock seam present. |
| WI-S06-002 (mark + atomic anchor) | 0 | 1 | 2 | 2 | APPROVE — P1 on `mark_started_at_ms` commit-before-scan ordering still implicit in spec narrative | Migration 0007 lifecycle CHECKs in place; canonical 250-row batch + 100ms jitter pinned. |
| WI-S06-003 (sweep + INV-GC-004 + audit fail-closed) | 0 | 0 | 2 | 2 | APPROVE — strongest spec in the part; load-bearing crypto query now `json_each`-canonical | Audit-emit-BEFORE-status-flip envelope verified by `audit_emit_failure_blocks_status_flip` test. |
| WI-S06-004 (physical delete + R2→D1 + DSR) | 0 | 1 | 2 | 2 | APPROVE — P1 on Ed25519 DPO public-key rotation runtime enforcement (specified but deferred-impl) | Strict `>` post-grace gate pinned via CountingPhysicalDeleteClock fixture; R2→D1 direction explicit. |

**Aggregate Part 1 verdict: APPROVE. 0 P0 / 3 P1 / 9 P2 / 8 P3 across 4 WIs.** Corpus is materially at the **9.0–9.3/10** SOTA target the program set as the post-Lote 10.6-tris projection. WI-S06-003 is the strongest sweep spec audited in the program to date; the JSON LIKE defect that dominated the pre-bis Part 1 review is closed and the canonical `json_each` idiom is now pinned via `prop_json_each_semantics_not_like` regression test.

---

## §2 Framework principles cited in this review

- **PRINC-INV-001** — INV-GC-001 (reachable never deleted) and INV-GC-004 (mark-phase-aware re-ref safe) are CRITICAL TLA+-verified obligations; any defect on the load-bearing query is P0.
- **PRINC-TLA-001** — Rust impl must cross-validate TLA+ obligations via property test ≥100k iter at nightly tier; the strict-`<` / protect-if-`>=` boundary must be sampled.
- **PRINC-SCHEMA-001** — D1 migrations are additive-only; CHECK constraints inline (not `ALTER TABLE ADD CONSTRAINT`); partial UNIQUE INDEX `WHERE status=...` for state-scoped uniqueness (Lote 10.5bis).
- **PRINC-AUDIT-FAIL-CLOSED** — audit emit failure on GC mutations rolls back the D1 batch; INV-OBS-AUDIT-CHAIN-INTEGRITY preserved.
- **PRINC-TENANT-CTX** — TenantCtx-enforced multi-tenant scoping in every SQL touching `blob_meta` / `ac_meta` / `gc_run` / `gc_candidates`.
- **PRINC-CHARTER-TRAIT-DEFER** — real binding (Cron DO / D1 / R2 / KV) + 100k nightly + cargo-fuzz + criterion can be consolidated alongside the PRR ship gate (WI-007) per autonomous execution charter; this is NOT a per-WI SEAL blocker but MUST be inventoried.
- **PRINC-DEGRADE-001** — PAT-DEGRADE-001 alignment; `gc-pause` config-singleton emergency stop.

---

## §3 Per-WI findings

### §3.1 WI-S06-001 — Worker GC binary + scheduler + degrade-mode

**Strengths.** Crate scaffolding `crates/corelink-gc/` ships 11 sub-modules with proper F-001 closure (every `GcWorker` owns scheduler state; no global mutable). `GcStatus` 6-variant + `GcPhase` 6-variant taxonomy resolves the pre-bis state-machine gap (Lote 10.6-tris OPUS-MISS-1 absorbed). Migration `0006_gc_run.sql` ships partial UNIQUE INDEX `WHERE status='running'` (mirrors WI-S05-004 multipart_sessions pattern; PRINC-SCHEMA-001 ✓). ScheduleClock seam present for deterministic tests. 94 tests parallel-safe.

**Findings.**

- **P1-001-1 [trait-abstraction-defer boundary not enumerated in §1 narrative]** — Cites `PRINC-CHARTER-TRAIT-DEFER`. The WI ships InMemory scheduler fake + ScheduleClock seam but the §1 narrative does not enumerate the exact list of deferred bindings (Cloudflare Cron DO shim, real D1 binding, 100k nightly tier expansion). The consolidation point is WI-S06-007 PRR ship gate, but a §1 sentence "Deferred to WI-007: [list]" would make the audit trail explicit. Recommendation: add a one-line cross-reference in WI-001 §1 narrative pointing to WI-007 §1.1 trait-defer inventory.

- **P2-001-1 [admin trigger surface stubbed without explicit S-13 forward-signal placeholder]** — Cites `PRINC-CHARTER-TRAIT-DEFER`. The manual admin trigger API is described as stubbed pending S-13 admin plane. The WI should explicitly cite the S-13 dependency as a `soft_blocker` and provide a placeholder handler signature so the consumer contract is auditable pre-S-13.

- **P2-001-2 [degrade-mode `gc-pause` config-singleton durability vs DO restart]** — Cites `PRINC-DEGRADE-001`. The spec says emergency-stop is via DO config-singleton, but does not explicitly state how the pause survives DO eviction/restart (KV-backed? D1-row-backed?). For an emergency pause this matters — if DO restarts and forgets pause, GC could resume during a SEV-0 incident.

- **P2-001-3 [back-off ramp on overload-detector not quantified]** — Cites `PRINC-DEGRADE-001`. `degrade overload-detector + back-off ramp` is listed in the sub-modules but the ramp constants (initial / max / multiplier) are not pinned in the WI. Recommendation: pin canonical values in §1 (e.g., initial 1s, max 60s, multiplier 1.5).

- **P3-001-1 [SLO `corelink_gc_worker_alive` heartbeat freshness not specified]** — Informational. Worker liveness should publish a heartbeat metric for DASH-GC; not blocking but useful to add in §5.

- **P3-001-2 [GcEventType enum 8 variants not cross-referenced to audit emission canonical event-string list version]** — Informational. The §1 lists 8 canonical event-strings; reference the audit_outbox WI-S01-004 cross-ref version to lock against drift.

### §3.2 WI-S06-002 — Mark phase (multi-pass scan + atomic anchor)

**Strengths.** Migration 0007 ships `gc_candidates` with composite-PK `(tenant_id, digest, mark_run_id)` tenant-leftmost (PRINC-TENANT-CTX ✓); 6 inline CHECK constraints including the canonical lifecycle partial-order (swept ≥ created); status domain `{candidate, swept, physically_deleted, protected_re_ref}` aligns with sweep + physical-delete consumers. Canonical pinnings `CANONICAL_BATCH_SIZE=250` (Lote 10.4bis D1 100KB envelope) + `CANONICAL_JITTER_MS=100` + `CANONICAL_PHASE_BUDGET_MS=10min` are canonical-locked in code. 49 new tests including 5 canonical property tests + 17 migration_canonical_0007.

**Findings.**

- **P1-002-1 [mark_started_at_ms commit-before-scan ordering still implicit in §1 narrative]** — Cites `PRINC-TLA-001`. The pre-bis P0-3 finding (TLA+ `GCMarkStart` requires anchor strictly precedes first `GCMarkStep`) is operationally resolved in code via `GcRunStore::transition_phase` immutable-set guard, but the §1 narrative still does not assert the explicit ordering invariant *"`mark_started_at_ms` is durably committed BEFORE the first batch read in `pass_blob_meta` executes"*. Property test `prop_atomic_anchor` covers the immutable-set semantics but not explicitly the WAL/D1 commit ordering. Recommendation: add a §1 invariant 1.x explicitly stating the commit-before-scan ordering, and a property test `prop_mark_anchor_commits_before_first_batch_read`.

- **P2-002-1 [reachable-set false-orphan classification claim not test-asserted at the §9.6 stated direction]** — Cites `PRINC-INV-001`. The §9.6 narrative says "false-reachable acceptable; false-orphan catastrophic". `prop_reachable_set_complete` test exists. But the failure-mode classification (false-orphan ⇒ INV-GC-001 violation, P0 production) deserves an explicit per-pass assertion that the UNION semantics is monotone-growing across the 3 passes (i.e., no pass can shrink the reachable set).

- **P2-002-2 [jitter 100ms pinned but D1 throttle adaptive ramp not specified]** — Cites `PRINC-DEGRADE-001`. Sustained D1 throttle should escalate jitter or trip a circuit-breaker; the WI mentions "circuit breaker se sustained throttle" but does not specify the trip threshold (consecutive throttles / window / ramp factor).

- **P3-002-1 [BlobDigest newtype rejection rationale fixed-length 64 not cross-cited to S-01 BLAKE3 canonical form]** — Informational. The BlobDigest newtype rejects upper-case/non-hex/wrong-length; cross-reference S-01 INV-BLAKE3-256-LOWER-HEX-64 to lock the canonical form.

- **P3-002-2 [criterion bench `bench_mark_1m_blobs` deferred to WI-007 but specific p99 capture method (warmup / sample size / measurement_time) not pinned]** — Informational. Add a one-line note pinning criterion config (`warmup=3s, sample_size=100, measurement_time=10s`).

### §3.3 WI-S06-003 — Sweep phase + INV-GC-004 + audit fail-closed

**Strengths.** This is the strongest spec in Part 1. (a) `protect-if-equal-or-newer` predicate canonically aligns with `gc_correctness.tla` L152-154 (Lote 10.6 cycle 4 fix is now load-bearing canonical); (b) `AcReferenceIndex` exposes the JSON-aware membership via `Vec<BlobDigest>` exact-match (the pre-bis P0-1 JSON LIKE defect is closed and `prop_json_each_semantics_not_like` is a regression pin); (c) audit-emit-BEFORE-status-flip envelope verified by `audit_emit_failure_blocks_status_flip` test — PRINC-AUDIT-FAIL-CLOSED ✓ on both protect AND sweep arms; (d) canonical config pins `GRACE_CAS_MS=72h / GRACE_AC_MS=24h / CANONICAL_SWEEP_PHASE_BUDGET_MS=5min` with constructor validator (`grace_cas_ms >= grace_ac_ms` regulatory floor — clever defensive coding); (e) `prop_inv_gc_004_protect_if_ge_strict_boundary` samples signed `ac_offset in -1000..=1000` straddling the TLA boundary.

**Findings.**

- **P2-003-1 [cross-tenant injection surfacing as `MarkError::Backend(cross_tenant_candidate)` is fail-closed but the error code is not in `error_taxonomy.md` `COR_*` registry]** — Cites `PRINC-TENANT-CTX`. The fail-closed direction is correct; a registry-level error code (e.g., `COR_GC_CROSS_TENANT_CANDIDATE`) would make the audit chain searchable.

- **P2-003-2 [SweepDecision `#[non_exhaustive]` 3-arm enum could grow to 4 once DSR-bypass arm is wired (S-11)]** — Informational forward-looking. The S-11 forward-signal DSR-bypass arm (grace = 0) is mentioned in WI-004, but WI-003 SweepDecision does not pre-declare a `DsrBypassImmediate` arm. Recommendation: pre-allocate the variant so S-11 wiring is additive-only.

- **P3-003-1 [Mann-Whitney 3-prong middleware-grade timing test for sweep deferred to WI-007 — explicit `|Δmedian| ≤ 5ms` budget not yet test-asserted in WI-003]** — Informational; charter trait-defer applies.

- **P3-003-2 [`prev_state` BlobState forensic snapshot field-set narrative not cross-cited to S-09 audit chain replay format]** — Informational. The `prev_state` capture is the load-bearing forensic trail; the field-set should be cross-cited to the canonical S-09 replay schema so any schema drift fires a compile-time error.

### §3.4 WI-S06-004 — Physical delete (post-grace + R2→D1 + DSR)

**Strengths.** (a) **Strict `>` post-grace gate** pinned via `CountingPhysicalDeleteClock` auto-advance fixture with explicit off-by-2 boundary seed — the off-by-one anti-pattern (data-loss bug) is regression-pinned; (b) **Conditional refcount=0 predicate** (re-upload race protection per Lote 10.6bis P0-4) — `physical_delete_skipped_re_referenced` audit arm covers the race; (c) **R2→D1 ordering** explicit (R2 DeleteObject FIRST then D1 row purge — orphan R2 detectable via WI-S06-005 reconcile orphan-detection arm; orphan D1 would lose audit trail — direction matters); (d) `R2BlobStore::delete_object_idempotent` follows S3/R2 404-as-success RFC; (e) `PhysicalDeleteConfig` canonical budget 30 min per §5.4 R-S06-9.1; (f) DSR signal scaffolding ships Ed25519 DPO-signed + tenant+digest-scoped + replay-protected via UNIQUE `signal_id` (PRINC-AUDIT-FAIL-CLOSED ✓).

**Findings.**

- **P1-004-1 [Ed25519 DPO public-key rotation runtime enforcement specified but deferred to S-11 — the `dsr_dpo_pubkeys` schema CHECK on `expires_at_ms` is pinned but the runtime fail-closed verifier is not yet in the WI-004 critical path]** — Cites `PRINC-CHARTER-TRAIT-DEFER`. Lote 10.6-tris NEW-P1-1 specifies the schema; the runtime enforcement path (key lookup at signal-receipt + reject if `now > expires_at_ms`) is the load-bearing crypto check. Recommendation: pin the runtime verifier signature in WI-004 §1 (or explicitly defer to S-11 with a forward-signal placeholder that fail-closes by default if the verifier is absent).

- **P2-004-1 [orphan R2 detection lifecycle: reconcile arm in WI-S06-005 detects but does not delete — the S-09 reclaim follow-up task is named but unticketed]** — Cites `PRINC-INV-001`. Orphan R2 (R2 has the object, D1 has no row) is a cost-only failure mode (data is "leaked" in R2 paying for storage without a customer reference). The reconcile arm `OrphanR2Detected` defers cleanup to S-09 — but no S-09 follow-up ticket ID is cited. Recommendation: file S-09 forward-ticket with the canonical reclaim contract.

- **P2-004-2 [bytes_reclaimed cost metric is customer-visible per WI-007 but the per-tier breakdown (hot/cold/glacier) is not enumerated in WI-004]** — Informational forward-looking to WI-007 / S-16.

- **P3-004-1 [PhysicalDeleteError `#[non_exhaustive]` 8-variant taxonomy is dense — recommend one-liner table mapping each variant to its retryability class (idempotent-retry / human-intervention / fail-closed)]** — Informational.

- **P3-004-2 [chaos test "R2 partial outage" scenario named in §1 but not enumerated in chaos suite §6.1 with explicit failure injection method]** — Informational.

---

## §4 Cross-cutting observations

1. **PRINC-CHARTER-TRAIT-DEFER consolidation hygiene.** Across WIs 001–004 the trait-defer narrative is consistent (consolidation at WI-007 PRR ship gate) but the per-WI §1 narratives are inconsistent in how they enumerate the deferred items. **Recommendation**: pin a canonical 3-line `Trait-abstraction-defer inventory` block in each WI §1 with `[deferred-item] → [WI-007 §X consolidation]` rows.

2. **TLA+ ↔ Rust cross-validation.** The pre-bis P0 anchors (JSON LIKE, column-name drift, anchor-before-scan ordering, GcStatus state-machine gap, sweep budget arithmetic) are all closed. The remaining TLA+ scope-limitation disclosure (soft-delete grace window NOT in TLA+; covered architecturally by WI-004 conditional refcount=0 + WI-005 reconcile orphan detection — Lote 10.6-tris NEW-P0-2) is explicitly published in ADR-0042 §A3 — ✓.

3. **Audit fail-closed envelope.** WI-003 + WI-004 + WI-005 all converge on the same "audit emit BEFORE state mutation; emit failure rolls back the D1 batch" pattern. **Recommendation**: extract this into a canonical PAT-AUDIT-FAIL-CLOSED resilience pattern documented in `specs/03_architecture/resilience_patterns.md` so future GC-adjacent WIs (S-07 eviction, S-11 DSR) inherit it explicitly.

4. **Strict-boundary fixture pattern.** The CountingPhysicalDeleteClock off-by-2 boundary seed pattern in WI-004 is the strongest off-by-one regression-pin in the program. **Recommendation**: extract as a canonical test pattern documented in `specs/06_quality/test_patterns.md`.

---

## §5 Top-of-pile recommendations (next-step concrete)

1. **WI-001 §1**: add `Trait-abstraction-defer inventory` block (P1-001-1).
2. **WI-002 §1**: add explicit `mark_started_at_ms commits before first batch read` invariant + property test (P1-002-1).
3. **WI-004 §1**: pin Ed25519 DPO pubkey rotation runtime verifier signature OR explicit S-11 fail-closed-by-default placeholder (P1-004-1).
4. Extract PAT-AUDIT-FAIL-CLOSED canonical resilience pattern (cross-cutting §4.3).
5. File S-09 forward-ticket for orphan-R2 reclaim (P2-004-1).

---

## §6 Verdict

**APPROVE Part 1 corpus for SEAL-as-is + Lote 10.6quater optional remediation pass for the 3 P1s.** The 3 P1s are NON-blocking for S-06 SEAL (corpus is already FROZEN AUDITED); they are recommendations for the cumulative INV §3.17 + cross-sprint hygiene track. The pre-bis P0 surface (JSON LIKE / column drift / anchor ordering / state-machine gap / budget arithmetic) is fully closed and regression-pinned via property tests. WI-S06-003 is materially the strongest sweep spec audited in the program and is the new best-in-class reference for INV-CRITICAL crypto-load-bearing WIs.

**Aggregate Part 1 score (calibrated against post-bis 9.1 target): 9.15/10.** WI-003 alone scores 9.4/10 (new best-in-class). WI-001 9.0, WI-002 9.0, WI-004 9.2.

---

**End R4 Opus Part 1 review.**
