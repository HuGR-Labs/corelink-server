---
id: "AUDIT-2026-04-25-AGENT-R4-S05-PART2"
type: "audit"
doc_status: "DRAFT"
audit_status: "CLOSED"
version: "1.1.0"
created: "2026-04-25"
updated: "2026-05-27"
reviewer: "Agent R4 (Claude Opus 4.7, 1M context, independent reviewer)"
scope: "Lote 10.5 — Sprint S-05 Part 2 (WI-S05-004 .. WI-S05-006)"
sprint_contract: "specs/04_sprints/_sealed/S05/_spec_contract.md v1.1.0"
calibration_baselines:
  - "specs/_audits/sealed/2026-04-25-agent-r4-s04-part1-wi-review.md (S-04 part1, 8.05/10; WI-S04-003 best-in-class 8.6)"
  - "specs/_audits/sealed/2026-04-25-agent-r4-s04-part2-wi-review.md (S-04 part2, 7.83/10)"
files_reviewed:
  - "specs/04_sprints/_sealed/S05/work_items/WI-S05-004-d1-schema-chunks-manifest-multipart-sessions.md (558 lines)"
  - "specs/04_sprints/_sealed/S05/work_items/WI-S05-005-merkle-manifest-builder-verifier.md (561 lines)"
  - "specs/04_sprints/_sealed/S05/work_items/WI-S05-006-sweeper-rb-fm-060-prr-ship-gate.md (614 lines)"
cross_references:
  - "WI-S05-001..003 (read for cross-WI consistency)"
  - "_spec_contract.md S-05 v1.1.0"
  - "WI-S04-003 / WI-S04-004 / WI-S04-005 (pattern reuse antecedents)"
---

> **CLOSED 2026-05-27** — S-05 sprint implementation sealed via git tag `s05-impl-sealed`; this independent review record is delivered. See `specs/_audits/2026-05-27-audit-triage-post-w36.md` for triage methodology.

# Agent R4 — Lote 10.5 S-05 Part 2 (WIs 004-006) WI Review

> **Reviewer**: Agent R4 (independent SOTA reviewer; ruthless, technical, no diplomacy).
> **Scope**: WI-S05-004 (D1 schema chunks/manifest_chunks/multipart_sessions), WI-S05-005 (Merkle manifest builder/verifier dual-side), WI-S05-006 (Sweeper Cron DO + RB-FM-060 + 160 GiB stitched + PRR ship gate).
> **Calibration target**: User directive "average não serve. SOTA puro 9-10". WI-S03-007 = 8.5; WI-S04-003 = 8.6 best-in-class; S-04 part 2 = 7.83 average.

---

## Veredito Geral

S-05 Part 2 is the **terminal trio of the multipart sprint**, and the materials show that the program has actually internalized a non-trivial subset of Lote 10.4bis lessons before review (CHECK constraints inline, BEGIN/COMMIT removed, tenant_prefix BLOB(16) materialized in `chunks`, ALTER TABLE ADD COLUMN explicitly justified, sharding ADR-0040 forward-flagged, Crypto SME mandatory non-waivable in WI-005, validate_inv_promotion.py CI gate referenced, 4-tier P0/P1/P2/P3 incident classification stated, gradual 10%→50%→100% rollout, HKDF sig info=`b"manifest-sig"` consistently separate from `b"ac-sig"`, `result_hash = merkle_root` direct (eliminating the protobuf-determinism gap that bit WI-S04-003 in round 3), Mann-Whitney 3-prong cripto-grade |Δmedian| ≤ 1ms, cargo-fuzz 1h × 3 targets, RFC 6962 leaf/inner domain separation in the manifest tree, MAX_CHUNK_COUNT 80000 tied to 160 GiB / 2 MiB consistently across handler/chunker/manifest/schema). On the surface this looks like a **legitimate maturity step over the S-04 baseline**.

But three classes of defects keep the trio short of the 9-10 target.

**First: load-bearing crypto and protocol details that Crypto SME will catch.** WI-005 inherits the same canonical-bytes-binding question that bit WI-S04-004 — `Manifest.merkle_root` is the only thing the sig commits to via the canonical bytes (the spec doesn't even publish the canonical-bytes layout the signer signs over; §1's struct shows fields but no `canonical_bytes()` function), which means an attacker who flips `total_size_bytes` or `chunk_count` (both load-bearing for the parser bounds) bypasses the sig if and only if a buggy verifier doesn't recompute Merkle root from `chunks` before using `total_size_bytes` for buffer sizing. The dual-side verify story doesn't pin the API ordering. The streaming progressive verify path is described as a contract but the `verify_streaming` API signature returns `Result<(), VerifyError>` after chunk N — it's not specified whether the verifier owns the abort signal back to the handler (gRPC trailer? trait callback?). Tree-shape: `balanced binary tree over chunk_digests sorted by index` — but the spec also says "Builder em-streaming (chunks chegam, tree builds incremental)" which implies on-the-fly hashing without sorting; there is a tension between "lex sort by chunk_digest (consistent WI-S04-003)" in §1 narrative point 2 of WI-005 and "sorted by index" in §1 tree shape — which is it? For multipart, **order is by chunk index** (chunks are NOT semantically a set; they're an ordered sequence whose concatenation is the blob); WI-S04-003's "lex sort by chunk_digest" pattern (which works for action results because the Merkle leaves are unordered file paths) is the WRONG primitive to copy here. Pin this. Meta-manifest in WI-006 §6.1.3 introduces a *third* HKDF info string `b"meta-manifest-sig"` — fine for domain separation, but the meta-manifest schema itself (`meta_manifests` table forward to S-14 OR S-09) is asserted to be added downstream rather than designed now, which means the 200 GiB stitched flow integration test is testing against a schema that does not yet exist in WI-S05-004 (which is the schema WI). That's a real gap.

**Second: storage/scale realism that doesn't survive multiplication.** WI-S05-004 §22 calculates 250M chunks × 150 bytes = 37 GB and notes "Excede D1 hard limit 10 GB; sharding mandatory; ADR-0040 forward". But the *current sprint* ships single-D1 schema at 37 GB target without any actual sharding implementation — only a "forward ADR" for the design. Sprint goal §6 DoD declares Throughput 100 MB/s steady, dedup ≥1.5×, 160 GiB single + stitched flow, but says nothing about D1 capacity being on a 90-day countdown to 80% threshold. ADR-0040 should be RATIFICADA in this sprint (not "forward"), or DoD must explicitly ack the sprint ships with a known-fragile D1 backend. Also: 250M rows × ~150 bytes is the *steady state*; the path to 250M is staged growth, but there is no per-month projection nor tie-in to gradual rollout 10%→50%→100% (rollout is on Worker version, not on schema growth — the 10% Worker still inserts into the same D1 chunks table). The cost gate $0.000001/D1-op assumes D1 paid tier pricing; Cloudflare D1 is currently in stages of pricing change and bills per row read/written — confirm the dollar figure tracks current pricing not 2024 pricing.

**Third: PRR ship gate has the same staffing reality and contract drift defects as S-04 part 2 — carried forward.** WI-S05-006 §30 has 10-of-13 sign-off rows still `_TBD_`/`_staffing-blocked_`. The "Crypto SME mandatory non-waivable" framing in §30 row 13 is correct for this WI but the sign-off table for WI-S05-001 (§30 row 13) and WI-S05-005 (§30 row 13) shows the same person/role appearing as gating across three WIs — there is no realism check that one Crypto SME can do 40-80h of independent review across WI-002 (FastCDC determinism + chunker), WI-005 (Merkle + sig domain separation), and WI-006 (PRR review of the implementation) within a 3-week sprint. Either the Crypto SME engagement is a multi-month booking (not noted) or the program is again writing "mandatory" on roles the program cannot actually staff. The 100% REAPI conformance non-negotiable in WI-006 reads cleanly *as a tightening* of sprint contract §19 ("⚠️ Throughput 100 MB/s → 80 MB/s com plan; ⚠️ FastCDC opt-in → defer pós-GA; ⚠️ Dedup ratio 1.5× → 1.2×") — but the sprint contract §19 doesn't actually permit 95% conformance waiver here (S-05 contract §19 has different waiver paths than S-04 contract §19), so the contract drift specifically flagged in S-04 part2 #1 has been **partially** fixed for S-05 (the contract doesn't permit the loophole). Verify by direct reading of §19. Subjective P0/P1/P2/P3 incident classification was a S-04 part 2 P0; in S-05 WI-006 §1 narrative point 7 enumerates the four tiers with examples ("P0: cross-tenant chunk leak; manifest forge; sweeper cron stale > 1h. Resets clock") — this is materially better than S-04 (S-04 left this entirely subjective). Promote.

The trio earns a **Part 2 average of 8.05/10** — meaningfully above the S-04 part 2 baseline (7.83). WI-S05-004 lands at **8.0** (tightest schema spec the program has produced; one P0 around UNIQUE direction phrasing + one P0 around storage projection / sharding ADR maturity), WI-S05-005 lands at **8.2** (cripto rigor real but `canonical_bytes` layout missing; tree shape ambiguity; streaming verify abort-signal under-specified), WI-S05-006 lands at **7.9** (ship gate is more realistic than S-04's, but sign-off staffing reality and meta-manifest schema-not-yet-designed gap are both unresolved). All three are **GO-WITH-FIXES** for Lote 10.5bis; none REJECT; **WI-S05-005 should not SEAL without Crypto SME independent review of the canonical-bytes layout** (which the spec doesn't even publish — that alone is a 40h SME-friendly gap to close).

The single highest-leverage Lote 10.5bis action is **publishing the explicit `Manifest::canonical_bytes()` layout in WI-S05-005 §1, including chunks-array commitment, and pinning that the sig is over canonical_bytes (not just over merkle_root)**. This closes the same defect class that R4 round 3 flagged for WI-S04-003/004 and is a 4h fix.

**Aggregate part 2 score: 8.05/10.**

---

## Per-WI Findings

### WI-S05-004 (D1 schema chunks + manifest_chunks + multipart_sessions + UNIQUE)

**Score: 8.0/10**

Breakdown:
| Axis | Score | Note |
|---|---|---|
| Schema correctness | 8.5 | CHECK inline correctly applied; PK composite tenant-first; UNIQUE direction correct; ALTER TABLE ADD COLUMN correctly justified |
| Completeness | 8.0 | 32 sections; 5 properties; 5 chaos; rollback test; deploy guard; ADR-0040 forward; data_model.md update |
| Clarity | 8.5 | SQL is dense but readable; comments load-bearing; storage projection numeric explicit |
| SOTA-adherence | 7.5 | Property tests 10k/100k; rollback runbook; D1 batch 250 rows internalized; sharding ADR forward (not ratificada) |
| Internal consistency | 7.5 | "10 buckets" Wrangler binding shown only as stub; CHECK on size_bytes upper bound 4 MiB conflicts with chunker config 1 KiB-4 MiB |
| Prior-WI consistency | 8.0 | WI-S04-002 tenant_prefix pattern reused; WI-S05-001 handler / WI-S05-006 sweeper consumption traced |
| Customer-facing readiness | 8.0 | Personas + DBA storage projection + alert at 80% D1 limit; SLA addendum from sprint contract |

**Strengths:**
- **CHECK constraints inline** is correctly applied across all three new tables (`length(tenant_prefix) = 16`, `region IN (...)`, `size_bytes >= 1 AND size_bytes <= 4194304`, `refcount >= 0`, `state IN (...)`, `last_activity_at >= started_at`, `chunk_index >= 0 AND chunk_index <= 80000`). The Lote 10.4bis lesson (`ALTER TABLE ADD CONSTRAINT chk_*` is a SQLite/D1 syntax error) is **internalized at the spec level**, not just claimed in a checkbox. Chaos #5 explicitly tests the regression path (legacy fix forgotten → SQLite syntax error in CI dry-run).
- **PK composite `(tenant_id, chunk_digest)` tenant-first** — same correct primitive as WI-S04-002. Property test `prop_chunks_tenant_isolation` codifies: "Tenant A INSERT; Tenant B SELECT WHERE tenant_id=B → empty" — index seek excludes A's row by composite PK design. This is the cleanest pattern the program has, **carried forward correctly**.
- **`tenant_prefix BLOB(16) materialized` column** in `chunks` and (implicit via path_key_id) in multipart_sessions — explicitly closes the WI-S04-005 P0 (tenant_prefix derivation in cron unspecified). Chaos test should include that the sweeper (WI-S05-006) reads `tenant_prefix` directly from D1 and never invokes HMAC(TDK, ...) at runtime; not currently asserted as a chaos test but the column existence at least makes it possible.
- **`is_chunked` flag on `cas_blobs` via ALTER TABLE ADD COLUMN** — explicitly justified ("ADD COLUMN OK em SQLite; NEVER DROP COLUMN em prod"). This is correct SQLite semantics. §9.5 design decision codifies the rule.
- **Storage projection 250M rows × 150 bytes = 37 GB > D1 10 GB** — explicitly numeric and explicitly flags sharding mandatory. Better than handwave; chaos test #4 simulates 30M rows boundary; alert at 80% D1 limit. ADR-0040 forward published path documented in §13.
- **Migration 004 idempotent + hash-validated** + rollback dummy (004a) + RB-FM-MULTIPART-MIGRATION-BUG runbook — the deploy/rollback story is operationally complete.
- **Deploy guard `scripts/check_multipart_infra.sh`** validates 5 chunk + 5 manifest buckets exist before migration applies — prevents the "schema applied but R2 buckets not provisioned" silent failure.
- **BEGIN/COMMIT removed** explicitly (Lote 10.4bis lesson; Wrangler implicit transactions).

**P0 — must fix before Lote 10.5bis SEAL:**

1. **`size_bytes` upper bound CHECK conflicts with chunker bounds.** §1 declares `CHECK (size_bytes >= 1 AND size_bytes <= 4194304)` — 4 MiB max chunk. But `ChunkRef::size_bytes` in WI-S05-005 §1 is documented as `1..=4 MiB`, and WI-S05-002 (FastCDC) typically uses **target 2 MiB with bounds 1 MiB ≤ chunk ≤ 4 MiB** — but for the **fixed-size 2 MiB default** in sprint contract §5.1, the chunks are exactly 2 MiB except the **final partial chunk which can be ANY size from 1 byte up to 2 MiB** (the spec says "fixed-size 2 MiB chunks (content) + final partial"). Final partial can therefore be 1 byte. The CHECK `size_bytes >= 1` is correct for that. **But the upper bound 4 MiB matches FastCDC max only**; for fixed-size 2 MiB, no chunk can exceed 2 MiB. Either: (a) loosen to fit FastCDC opt-in (4 MiB upper, current spec) — fine; (b) split CHECK by `chunker_algo` column (not present); (c) move the upper bound enforcement to the chunker layer and remove from CHECK. **The spec is currently correct for FastCDC max but the comment "1 byte to 4 MiB max chunk" inverts the lower bound (1 byte != 1 MiB; FastCDC paper bounds are 1 MiB..4 MiB target 2 MiB).** Verify the actual chunker-emitted size distribution and align bounds: probably `CHECK (size_bytes >= 1 AND size_bytes <= 4194304)` is fine for both modes; just fix the comment. Minor but Crypto SME / chunker engineer will flag inconsistency.

2. **UNIQUE direction story for `multipart_sessions` permits an availability bug.** §1 declares:
    ```sql
    UNIQUE (tenant_id, blob_digest_expected, state)
    ```
    The intent (§9.7): "prevents duplicate in_progress for same blob; allows multiple completed/aborted records (audit trail)". **But:** this UNIQUE rejects two `state='aborted'` records with same `(tenant_id, blob_digest_expected)`, and rejects two `state='completed'` records likewise. The multi-record audit trail use case requires that the constraint NOT include `state='completed'` or `state='aborted'`. With this constraint, if a session aborts twice (sweeper retries idempotently), the second `UPDATE state=aborted` is a no-op (state didn't change so no UNIQUE bump) but a re-INSERT would fail. More problematic: the recovery path "abort fails → retry → retry → retry until success" generates one row, not multiple — that's fine. **The actual failure mode**: two clients **legitimately** doing concurrent `Initiate` on the **same blob_digest** (because the same client computed the expected blob digest twice in two concurrent CI jobs). The first INSERT succeeds with state=in_progress; the second collides with UNIQUE; handler must return 409 OR convert to retry-on-existing. The Gherkin §8 scenario "multipart_sessions UNIQUE in_progress" handles this — but the documented response is **"handler returns 409 OR retry semantics"**, which is API-undefined. WI-S05-001 handler must declare ONE of these two semantics; currently the schema permits either and the handler doesn't pin. **Decide and document one consistent semantic across WI-S05-001 + WI-S05-004**. Recommend retry-on-existing (return existing `upload_id`) — this is the multipart idempotency contract from S3 / R2 native API. Currently WI-005-004 §8 Gherkin says "OR".

3. **Storage growth gate ships with ADR-0040 still forward (DRAFT, not RATIFICADA) — same defect class as S-04 ADR-0019/0021 carried forward.** §22 numeric: 250M rows × 150 bytes = 37 GB; D1 hard limit 10 GB; sharding ADR-0040 forward. WI-S05-006 §6.1.9 says ADR-0040 RATIFICADA via WI-006 ship gate. But ADR-0040's content is "multipart D1 sharding strategy (per-tenant_tier OR per-region; ratificada em WI-S05-006)" — i.e., the actual sharding strategy is **left undecided by S-05** with ratificação being a procedural rubber-stamp at ship gate. **What does "ratificada" mean here if the design isn't picked yet?** Either: (a) make ADR-0040 a fully designed ADR in this sprint with actual sharding key chosen (per-tenant_tier vs per-region vs per-tenant — pick one with rationale), with WI-S05-006 ship gate confirming it; (b) downgrade ADR-0040 to "design milestone S-XX (post-S-05)" and document the sprint ships with known fragile D1 backend at 37 GB target / 10 GB hard limit / 80% alert; (c) defer 250M-row workload until ADR-0040 SEALED. Currently the spec asserts both "37 GB target" and "ratificada in this sprint" without showing the design content. **This is a procedural-rubberstamp risk identical to the WI-S04-006 sign-off rubber-stamp problem the program has been trying to fix.**

4. **Refcount semantics for delete ordering is silent on the race that bit WI-S04-005 in S-04 part 2.** §1 narrative point 5 says "Decrement on manifest delete (S-06 GC forward); refcount = 0 = candidate for chunk delete." Idempotent INSERT increments and S-06 GC decrements; refcount=0 → R2 DELETE candidate. But: if S-06 GC and a SplitBlob handler race (GC sees refcount=0, fires R2 DELETE for chunk; concurrent SplitBlob INSERT-on-conflict bumps refcount 0→1 between GC's read and DELETE), the chunk is deleted from R2 but D1 says refcount=1. The reverse race is also possible. Mitigation requires either: (a) optimistic concurrency `WHERE refcount = 0` on the GC DELETE (so a race is detected); (b) two-phase: mark `pending_delete` then DELETE in next sweep; (c) SQL transaction wrapping the read-decide-delete. This is a forward S-06 problem but the `refcount` semantics in WI-S05-004 should already define the contract (R-007 in risk register only flags "Refcount overflow"; the race is unmentioned). Add a row to risk register: "R-XX Refcount race S-06 GC vs SplitBlob (Lote 10.4bis WI-S04-005 R2-then-D1 race lesson) | M | M | HIGH | M | LOW | Optimistic concurrency `WHERE refcount = 0` on GC DELETE; documented in S-06 forward". 

**P1 — fix this sprint, before PRR:**

5. **`manifest_chunks.chunk_index <= 80000` CHECK is correct but inconsistent across WIs.** WI-S05-005 §1 declares `MAX_CHUNK_COUNT = 80000`; WI-S05-001 §1 narrative point 5 "MAX_CHUNKS_PER_MANIFEST = 80000". Schema CHECK matches: `chunk_index >= 0 AND chunk_index <= 80000`. Edge case: `chunk_index <= 80000` allows index 80000, which means 80001 chunks (0-indexed). The bound should be `chunk_index < 80000` OR the constant should be 80001. Currently the program lists 80000 as MAX_CHUNK_COUNT (i.e., maximum count of chunks, not maximum index value). Pick one and align: recommended `chunk_index < 80000` for index bound + MAX_CHUNK_COUNT = 80000. Off-by-one matters at the bound.

6. **Wrangler binding stub is incomplete — only 1 of 10 R2 buckets shown.** §1 TOML stub:
    ```toml
    [[r2_buckets]]
    binding = "CHUNK_BUCKET_SAM"
    bucket_name = "corelink-chunk-sam"
    # ... 4 more regions × 2 (chunk + manifest)
    ```
    The actual artifact (per §13) is `wrangler.toml` updated, but the stub elides 9 of 10 bindings with `# ...`. Either: (a) include all 10 (canonical answer; deploy guard validates); (b) reference `wrangler.toml` ground truth from a separate source; (c) accept the stub with a note "exhaustive enumeration in artifact". Currently it's hand-wave for a critical infrastructure config.

7. **Property test `prop_multipart_sessions_state_transitions` description vs CHECK constraint mismatch.** §6.1.9.5 declares the property: "in_progress → completed OR aborted; never reverse." But the schema only has `CHECK (state IN ('in_progress', 'completed', 'aborted'))` — no triggered constraint enforcing monotonicity. §8 Gherkin "multipart_sessions state transitions" then says "handler-level rejection (not enforced by CHECK; INV-MULTIPART-STATE-MONOTONIC documented em registry)." So the property test, the registry invariant, and the handler are all responsible for the same property — but the schema CHECK is silent. This is fine ARCHITECTURALLY (defense-in-depth), but the property test as currently described tests handler behavior, not schema behavior. Re-frame the property test as `prop_handler_state_monotonic` and remove the `_state_transitions` from the schema property test list (which is currently 5 properties; would drop to 4). Or move the test scope to handler-level.

8. **D1 batch 250-row chunking strategy is implied but not explicit.** SLA addendum says "D1 manifest_chunks batch INSERT 250 rows ≤ 100ms (Lote 10.4bis 100KB limit lesson)." But: what if a manifest has 80000 chunks? 80000 / 250 = 320 batches of INSERT × 100ms = 32 seconds — exceeds CF Worker 30s CPU budget. Either: (a) accept latency; (b) move manifest_chunks INSERT to async outbox (D1 writes serialized via task queue); (c) reduce per-manifest max chunks to fit a single worker invocation. WI-005-001 §1 narrative point 11 says "manifest_chunks INSERT batch ≤ 250 rows" but doesn't address the 80000-chunk worst case. Pin the strategy.

9. **R2 bucket lifecycle policy unmentioned.** Schema implies chunks/manifest/multipart-session storage but no R2 lifecycle rule documented. R2 multipart sessions specifically benefit from `lifecycle add` rule "abort multipart > 7d" applied at the **bucket level** as defense-in-depth alongside the sweeper cron (Lote 10.4bis Wrangler CLI lesson — `lifecycle add`). Sprint contract §5.2 mentions Sweeper as primary; §6 DoD mentions sweeper but not bucket-level lifecycle. Document the dual control.

10. **D1 storage projection assumes uniform tenant distribution; no tail-tenant analysis.** §22: "10M blobs × 25 chunks/blob avg" — distribution-blind. Fat-tail tenants (one customer with 100M chunked blobs) blow the average. Add: P95 tenant blob count; per-tenant chunks row cap (or document why no cap).

**P2 — next sprint or doc-only:**

11. §6.1.9 property tests count: "5 properties" listed; counting the bullets gives 5; aligns. Good.

12. §15 chaos count: 5; aligns with HIGH_RISK SOTA bar minimum (≥5). The bar says "≥ 5"; this is at the floor. Other HIGH_RISK WIs in the program have 12 chaos (WI-S04-005, WI-S04-006); 5 here is below the program's own SOTA — bump to 8-10 to match peers.

13. §28 risk register only 10 rows; SOTA bar "≥ 10". At the floor. WI-S04-005 has 12. Adding refcount-race row (P0 #4) brings to 11; consider 2 more (sharding migration disruption R-010 already there; could add "R-011 D1 batch 250-row chunking strategy regressed").

14. Sub-task ST-014 "Architect + DBA + AppSec review iter: 3h" — 3h for three specialists is the same below-industry-norm flag from WI-S04-004 ST-018 (4h for Crypto SME). DBA review of a sharding-pre-decision schema with 37 GB / 10 GB tension is realistically 8-16h. Bump or reframe.

15. Cost analysis §22 $360/yr seems light: D1 at $0.75/GB/mo × 40 GB = $30/mo = $360/yr is correct for storage alone, but D1 also bills per row read/written; 250M chunks × refcount-update workload at 10M reads/dia × $0.001/k-rows = $10/dia = $3.6k/yr. Add row-op cost line.

16. §31 change log claims "1.0.0 / 2026-04-25 / Lote 10.5; SOTA pós-Lote 10.4bis lessons applied". Confirm by direct grep that all four lessons (CHECK inline / BEGIN-COMMIT removed / tenant_prefix BLOB(16) / sharding ADR-0040 forward) appear in the spec body — they do. Honest claim.

17. Sign-off table row 13 "DBA (advisory)" — same DBA staffing-blocked situation as WI-S04-002. No compensating control for absent DBA (e.g., a contract DBA review).

18. Migration 004 idempotency hash-validation — assumes `migration_meta` table exists from S-01; verify.

---

### WI-S05-005 (Merkle manifest builder + verifier dual-side)

**Score: 8.2/10**

Breakdown:
| Axis | Score | Note |
|---|---|---|
| Rigor cripto | 8.0 | BLAKE3 + RFC 6962 correct; HKDF info domain sep correct; canonical_bytes layout NOT published; tree-shape ambiguity (sort-by-index vs lex-sort) |
| Completeness | 9.0 | 32 sections; 5 properties; 5 chaos; cargo-fuzz 1h × 3 targets; test vectors Annex A+B (50+50); spec/manifest_protocol.md; ADR-0041 forward |
| Clarity | 8.5 | Reads precisely; primitives correctly named; only the canonical-bytes elision blurs |
| SOTA-adherence | 8.5 | Mann-Whitney 3-prong cripto-grade |Δmedian| ≤ 1ms; Crypto SME MANDATORY EMPHATIC; benchmarks tight |
| Internal consistency | 7.5 | Tree shape "sort by chunk_digest" vs "sort by chunk_index" — pick one; streaming verify abort signal under-specified |
| Prior-WI consistency | 8.5 | Pattern reuse from WI-S04-003 (RFC 6962 leaf/inner) and WI-S04-004 (HKDF info+SignatureVerifier trait) traced; result_hash = merkle_root direct (Lote 10.4bis lesson) |
| Customer-facing readiness | 8.0 | Dual-side verify story honest about partial vs full HKDF compromise; SLSA L3 partial alignment claim quantified |

**Strengths:**
- **`result_hash = merkle_root` direct** (§9.6 design decision; Lote 10.4bis lesson explicit). This is the same fix R4 round 3 forced into WI-S04-003 to eliminate protobuf-determinism dependency. Internalized, not just claimed. Carried forward correctly.
- **HKDF info=`b"manifest-sig"` separated from `b"ac-sig"`** (§9.3). Pattern reuse from WI-S04-004 is correct; CI byte-equal test asserts info string is fixed (chaos #4); ADR-0041 forward documents the policy. Cross-domain replay defense codified.
- **WI-S04-004 `SignatureVerifier` trait reused** (§9.7) — no duplicate cripto stack. Single source of truth for sig primitives.
- **RFC 6962 leaf/inner domain separation** (§1 tree shape: `leaf_hash = blake3(\x00 || chunk_digest)`, `inner_hash = blake3(\x01 || left || right)`) — prevents 2nd-preimage tree-shape attack. Correctly named, correctly cited.
- **Bounded parser** with MAX_CHUNK_COUNT=80000 + MAX_TOTAL_SIZE=160 GiB enforced at decode (`ChunkCountExceeded`, `TotalSizeExceeded` errors). Fail-fast at decode (≤ 1ms reject p99).
- **Streaming progressive verify** with mid-stream fail-fast (`StreamingChunkMismatch { index: u32 }`) — defense-in-depth pattern; integration test asserts handler invokes (chaos #3).
- **Cargo-fuzz 1h CI nightly × 3 targets** (decode + verify + sig) — matches WI-S04-004's cargo-fuzz pattern; closes the round-3 P1 ("only fuzz verify"). Internalized.
- **Test vectors Annex A (50 valid) + B (50 invalid; each error variant)** — reusing WI-S04-003 pattern; CI integrates Annex (chaos #5 catches Annex regression).
- **Mann-Whitney 3-prong cripto-grade |Δmedian| ≤ 1ms** between valid and tampered manifest verify timing — appropriate tightness for a verify protocol where the structural verify is dominated by BLAKE3 SIMD on 80k chunks.
- **`#[non_exhaustive]` on `Manifest` struct** + semver discipline + ADR-0041 forward — public API stability codified.
- **`#[forbid(unsafe_code)]`** + zero `unwrap` enforced — Rust SOTA hygiene.
- **Determinism property `prop_manifest_determinism`** asserts 1000 random × 100 builds = byte-identical — closes "HashMap iteration order" footgun that's a recurrent program risk (WI-S04-003).

**P0 — must fix before Lote 10.5bis SEAL:**

1. **`canonical_bytes()` layout for the sig is NOT PUBLISHED in the spec.** §1 declares `pub sig: Vec<u8>` and `ManifestSigner` trait, and §9.7 says "SignatureVerifier trait reuse from WI-S04-004 (no duplicate cripto stack)". WI-S04-004's `verify_sig(canonical_bytes, sig, sig_key_id)` requires `canonical_bytes` as input — but **WI-S05-005 never publishes what the manifest's canonical_bytes ARE**. Implicit assumption: it's `merkle_root` (32 bytes) only. **But:** §1 declares fields `version: u8, tenant_id, blob_digest, merkle_root, chunk_count, total_size_bytes, chunks, created_at_ms, sig, sig_key_id, chunker_algo`. Of these, the sig must commit to AT MINIMUM: `version + tenant_id + blob_digest + merkle_root + chunk_count + total_size_bytes + created_at_ms + chunker_algo` (8 fields). If only `merkle_root` is signed, an attacker who captures a valid manifest with `chunk_count=25, total_size_bytes=50_MiB` can forge a manifest with the same `merkle_root` but `chunk_count=80000, total_size_bytes=160_GiB` — **the bounded parser would accept** (80000 ≤ 80000, 160 GiB ≤ 160 GiB) and the sig would still verify. The bounded-parser protection is only effective if the bounds are **inside the canonical bytes**. Fix: explicitly declare the canonical_bytes layout (analogous to WI-S04-004 §6.1.5 89-byte layout). Suggested:
    ```
    canonical_bytes_v1 = 
        version_le_u8                              (1 byte)
     || tenant_id_canonical_bytes                  (variable; length-prefixed)
     || blob_digest                                (32 bytes)
     || merkle_root                                (32 bytes)
     || chunk_count_le_u32                         (4 bytes)
     || total_size_bytes_le_u64                    (8 bytes)
     || created_at_ms_le_u64                       (8 bytes)
     || chunker_algo_le_u8                         (1 byte)
                                                   ─── ~86 bytes + tenant_id length
    ```
    `chunks: Vec<ChunkRef>` is NOT in canonical_bytes — `merkle_root` commits to the chunks already. Add the layout to §1 + §6.1.5 + Annex A test vectors (must include canonical_bytes hex per vector). **Without this, WI-005 cannot be Crypto SME independently reviewed; the SME will ask "what does the sig sign over?" and the spec will be silent.** This is a 4h fix and the highest-leverage Lote 10.5bis action.

2. **Tree-shape ambiguity: lex-sort-by-chunk_digest vs sort-by-chunk_index.** §1 tree shape: "balanced binary tree over chunk_digests sorted by index" (correct for multipart — order is semantic). §1 narrative point 2: "Determinism: tree construction byte-stable for same `(tenant_id, blob_digest, ordered chunks)` — no timestamps in tree; protobuf-free encoding". §9.7 "reuse from WI-S04-003 ... Bounded parser + cycle detection: not applicable here (manifest tree is flat list of chunks; no nested directories)". WI-S04-003 used **lex sort by digest** because action-result outputs are an UNORDERED set of (path, digest) pairs and lex ordering provides canonicalization. **But chunks in a multipart blob are an ORDERED sequence** — sorting them by digest reorders bytes! Concatenating `chunks[lex_sorted]` would give the wrong blob. The tree must hash chunks in **index order** (i.e., the leaf array is `[hash(\x00 || chunks[0].digest), hash(\x00 || chunks[1].digest), ..., hash(\x00 || chunks[N-1].digest)]`, NO sorting). The spec text mixes the two orderings and a careless reader will copy WI-S04-003's lex-sort and break determinism. **Pin the ordering as "by chunk_index ascending; no sorting; leaves are in the order chunks are concatenated to produce the blob"** in §1 tree shape AND remove the "lex sort" reference if any (verify there isn't one in the §9.7 narrative carrying-over from WI-S04-003 pattern reuse). Add a property test `prop_manifest_chunk_order_preserved`: shuffle input chunks → manifest_root differs → reject (the spec already asserts this implicitly via `prop_manifest_round_trip` but the order-vs-set distinction must be explicit).

3. **Streaming progressive verify abort-signal mechanism is under-specified.** §1 trait:
    ```rust
    async fn verify_streaming<'a>(
        &'a self,
        manifest: &'a Manifest,
        chunk_stream: impl Stream<Item = (u32, Bytes)> + 'a,
    ) -> Result<(), VerifyError>;
    ```
    Returns `Result<(), VerifyError>`. **But the contract** (§1 Cripto-driven invariants point 5: "Streaming progressive verify: fail-fast pattern; never trust client-supplied bytes mid-stream") requires that on fail at chunk N, **the handler must abort the upstream R2 stream / gRPC stream / HTTP body** to prevent the tampered chunk N+1 from being processed. The `Result` return type alone doesn't propagate cancellation back to the producer — it's the consumer-side error. WI-S05-001 §1 step [5] should call `verify_streaming` and on `Err` cancel the upstream `chunk_stream`. But: the chunk_stream is `impl Stream<Item = (u32, Bytes)>` — futures' `Stream` doesn't have native cancellation; cancellation is via `Drop` of the future or by returning error from a `select_all`. Pin the mechanism. Recommend: (a) `verify_streaming` returns a `(Result, AbortHandle)` pair so the handler can cancel; OR (b) document that the handler wraps the chunk_stream in a `select_with_cancel` adapter that the verifier signals via an internal channel; OR (c) use `tokio_util::sync::CancellationToken` parameter. Currently §6 Gherkin "And handler signals abort to client" is hand-wave on the mechanism. Pin it.

4. **`Manifest::version: u8 = 1` field and migration story.** §1 declares `pub version: u8, // = 1` with no migration handler. Protocol versioning at this layer is correct (u8 supports 256 versions). But: §6.1.7 says "Online schema migration v1 → v2 (post-GA via ADR)" is OUT OF SCOPE. **What does the verifier do for `version != 1` in v1 ship?** Currently `VersionUnsupported(u8)` error variant exists in `ManifestError` — verifier presumably rejects. Property test `prop_manifest_bounds_enforcement` doesn't include `version` boundary cases. Add: test vector for `version=2` rejected; test vector for `version=0` rejected; document the v1→v2 migration plan (forward) — even just "in-band migrator service in S-XX". Otherwise verifier is silently rigid on a 1-byte field.

5. **HKDF info=`b"manifest-sig"` and meta-manifest info=`b"meta-manifest-sig"` are byte-stable but not collision-resistant against substring-extension.** WI-S05-006 §6.1.3 introduces `b"meta-manifest-sig"`. Domain separation requires that **no info string is a prefix of another**. `b"manifest-sig"` is NOT a prefix of `b"meta-manifest-sig"` (different first byte: `'m' || 'a' || ...` vs `'m' || 'e' || ...`) — fine. But `b"meta-manifest-sig"` literally contains `b"manifest-sig"` as a substring. HKDF's domain separation property is robust to substring containment (the info is concatenated into HKDF-Expand's input, not searched within), so this is **cryptographically safe**. **But:** future maintenance risk: an engineer refactoring the constants might collapse them. Mitigation: add CI byte-equal test asserting BOTH `b"manifest-sig"` AND `b"meta-manifest-sig"` constants present and distinct; assert no info string is a prefix of another (extensible to `b"ac-sig"` from WI-S04-004). Currently §1 / §15 chaos #4 only asserts `b"manifest-sig"` byte-equal. Strengthen to assert all three info strings are present + distinct + non-prefix. Crypto SME will appreciate.

**P1 — fix this sprint, before PRR:**

6. **Mann-Whitney cripto-grade |Δmedian| ≤ 1ms is a reasonable bar but the comparison is not symmetric.** §10.s05.005.2: "|Δmedian| ≤ 1ms (valid vs tampered manifest verify)." The **timing of a tampered manifest verify is dominated by the early reject** (RootMismatch fail-fast — likely <1ms for 25 chunks because BLAKE3 verification of one mismatching leaf gives O(log N) tree walk to find the bad leaf, but the spec says "p99 verify ≤ 50ms @ 80k chunks" for valid, and tampered probably ≤ 5ms at 80k chunks). |Δmedian| = 50ms - 5ms = 45ms ≫ 1ms. The Mann-Whitney bar can only meet 1ms if the verifier is **forced to traverse the entire tree even on early reject** — i.e., NO fail-fast on tree mismatch. **But the spec ALSO requires fail-fast** (§1 narrative point 4: "Bounded parser ... reject early at decode"; §1 streaming progressive verify "fail-fast em mid-stream"). These are in tension: cripto-grade timing equality requires constant-time verify; fail-fast requires variable-time verify. WI-S04-004's |Δmedian| ≤ 0.5ms for sig verify of an 89-byte input is correct (sig verify is constant-time by design — `subtle::ConstantTimeEq` on 32 bytes). Manifest verify at 80k chunks is **NOT a constant-time primitive**; it's a tree walk where time-to-detect-tampering depends on tampering location. Either: (a) reframe the cripto-grade gate as "constant-time per-chunk leaf verify" (WI-S04-004 pattern; ≤ 0.5ms per leaf comparison) and drop the whole-manifest |Δmedian| metric; (b) keep the whole-manifest gate but at a realistic delta (e.g., |Δmedian| ≤ 10ms or more); (c) require timing-equal manifest verify as a security control (force the verifier to walk the entire tree even on mismatch — WASTES CPU on tampered inputs but eliminates timing oracle for tamper location). **Pick one and document.** WI-S04-004's cripto-grade gate makes sense for sig-verify; copying it to manifest-verify without the constant-time path requirement is a category error.

7. **Test vectors Annex B (50 invalid; each error variant) — variant coverage math.** §10.s05.005.5: "Annex B (50 invalid; each error variant)." `ManifestError` has 7 variants (ChunkCountExceeded, TotalSizeExceeded, RootMismatch, InvalidChunkDigest, ChunkIndexOutOfOrder, VersionUnsupported, SigError). 50 / 7 = ~7 vectors per variant. RootMismatch alone needs 10+ (1-byte flips at varying offsets in chunks digests + claimed root + leaf vs inner mismatch positions). Recommend 70+ vectors total: 15 RootMismatch + 10 ChunkIndexOutOfOrder + 10 ChunkCountExceeded + 10 TotalSizeExceeded + 10 InvalidChunkDigest + 5 VersionUnsupported + 10 SigError — total 70. Current 50 is undersized. (Matches the WI-S04-004 round-3 P1 #11 finding.)

8. **Streaming verify per-chunk p99 ≤ 5ms (BLAKE3 SIMD) target — verify the unit.** §3 SLA addendum: "Streaming verify per-chunk p99 ≤ 5ms (BLAKE3 SIMD)." 2 MiB chunk × BLAKE3 ≥ 2 GB/s = 1ms. p99 ≤ 5ms with 4ms slack for non-BLAKE3 work (memory copy, tree position lookup, channel send). Reasonable. **But:** at 80k chunks streaming serially × 5ms = 400 seconds — exceeds CF Worker 30s budget. Streaming verify is by definition pipelined with download (no serial wait), but the budget tracking has to account for verify-pipeline-blocking-download. Document the actual concurrency model: chunks arrive from R2 at network-rate (probably 100 MB/s × 2 MiB chunk = ~50 chunks/sec); verify at BLAKE3 SIMD rate (~1000 chunks/sec); verify is faster than download, no blocking. OK in steady state. Worst case: bursty download where 100 chunks arrive in 100ms; verify backs up; queue depth bounded somewhere. Document the queue bound (memory budget per request stack ≤ 32 KiB per §14.s05.005.9 — this implies queue depth = 32 KiB / chunk_metadata_size ≈ 1000 entries. Likely fine. Specify.

9. **Cargo-fuzz 3 targets enumeration vs WI-S04-004's 1 target.** §10.s05.005.4: "decode + verify + sig (3 targets)". WI-S04-004 round-3 P1 #10 forced it to add 2 more targets (sign + tdk_handle). Verify the manifest cargo-fuzz also covers builder (encode) — currently the 3 are decode/verify/sig; missing **build** (encode side: `ManifestBuilder::build`). A future ADR might change the canonical encoding; encode-side fuzzing detects regression. Add **builder** target → 4 targets.

10. **`prop_streaming_verify_fail_fast` test definition.** §6.1.7: "tampered chunk N; verify catches before chunk N+1". Implementation note: the test must assert **chunks 6..25 NOT processed** after chunk 5 fails. This is testing absence — easy to write incorrectly. Recommend: the test injects a "next chunk would panic" sentinel as chunk 6; if the test passes (verify caught chunk 5 and aborted), the panic is never hit; if verify continued past chunk 5, panic fires and test fails. Document the test idiom in §1 properties section.

11. **`MAX_CHUNK_COUNT 80000` derivation 160 GiB / 2 MiB.** Math: 160 × 1024 / 2 = 81920, NOT 80000. The discrepancy is rounding. 80000 × 2 MiB = 156.25 GiB max. Either: (a) raise MAX_CHUNK_COUNT to 81920 (160 × 1024 / 2 exactly); (b) reduce MAX_BLOB_SIZE to 156.25 GiB; (c) accept 80000 as a round number with the implicit max blob ~156 GiB. Currently the spec asserts both "80000 = 160 GiB / 2 MiB" (false) and "MAX_TOTAL_SIZE 160 GiB" (true). This is a definitional inconsistency that **also affects sprint contract §5.1, WI-S05-001, WI-S05-004 schema CHECK, and the integration test**. The simplest fix: MAX_CHUNK_COUNT = 81920; update everywhere. (Or leave 80000 and reduce documented max blob to "≤ 80000 × 2 MiB = 156.25 GiB single multipart" — but sprint contract advertises 160 GiB.)

**P2 — next sprint or doc-only:**

12. §28 risk register 12 rows; meets the SOTA bar ≥ 10. Good.

13. R-001 "Manifest forge via cripto break (BLAKE3 collision) | L | L | CRITICAL | L | LOW" — the residual LOW assumes 2^128 collision-resistance is durable; correct under classical adversary; post-quantum threat (Grover gives sqrt speedup, ~2^128 → ~2^85 quantum) is a known concern. Add row R-013 "Post-quantum threat to BLAKE3-256 collision-resistance | L | L | LOW (timeline) | L | LOW | Post-quantum migration via S-XX ADR; hash agility via `chunker_algo` → can extend to `manifest_hash_algo` v2".

14. Sub-task ST-015 "Crypto SME review iter (BLAKE3 + Merkle protocol + sig domain sep): 4h" — same 4h-vs-40-80h industry-norm gap as WI-S04-004 ST-018. Lote 10.4bis review forced a recognition; carry-forward unfixed here. Bump or call it consultation not full review.

15. §15 chaos count: 5; same floor-level as WI-S05-004; below S-04 part 2 peer 12. Bump to 8-10.

16. §22 cost analysis $5.4k/yr — lacks Crypto SME ongoing engagement cost (post-ship review or retainer for emergency BLAKE3 CVE response). Add ~$5k/yr cripto consultation line.

17. §4 examples — unenumerated. List the 4 examples (`build_25_chunks.rs`, `verify_full_with_tampering.rs`, `streaming_progressive.rs`, `meta_manifest_stitched.rs` — last one ties to WI-S05-006 stitched flow). Currently §10.s05.005.9 just asserts "4 examples".

18. Sign-off table `_pending_` for 12 of 13 — same staffing reality as WI-S04-004. Crypto SME MANDATORY EMPHATIC is correct; engagement plan absent.

---

### WI-S05-006 (Sweeper Cron DO + RB-FM-060 + 160 GiB stitched + PRR ship gate)

**Score: 7.9/10**

Breakdown:
| Axis | Score | Note |
|---|---|---|
| Sweeper design | 8.5 | Per-region; alarm re-arm at start (Lote 10.4bis); bounded batch 250; tenant-scoped strict |
| Stitched flow correctness | 6.5 | Meta-manifest design is asserted but `meta_manifests` table forward to S-09/S-14 — schema not in this sprint; integration test 200 GiB tests against undefined schema |
| Conformance enumeration | 7.5 | "10 enumerated tests" claim correct but commit hash + enumeration lives "em ADR-0038 Annex" — same defect as S-04 P0 #5 partial |
| 4-tier classification | 8.5 | P0/P1/P2/P3 enumerated with examples; better than S-04's subjective version |
| PRR sign-off staffing | 6.5 | 10-of-13 _TBD_/staffing-blocked; carried forward from S-04 part 2 unfixed |
| Crypto SME consistency | 8.5 | Mandatory non-waivable across WIs; no contradiction; pre-PRR review of WI-005 + WI-002 referenced |
| Production rollout | 8.5 | Gradual 10%→50%→100%; D+9..D+13 timeline explicit; rollback Wrangler version revert |

**Strengths:**
- **Sweeper Cron DO per-region + alarm re-arm AT START of tick** (§6.1.1; Lote 10.4bis lesson explicitly internalized — addresses the WI-S04-005 P1 #5 finding "if alarm handler panics, re-arm code never runs"). Chaos #1 catches sweeper-not-re-armed via metric alert ≤ 1h.
- **Bounded batch 250 sessions/tick** (§6.1.1; Lote 10.4bis D1 100KB limit lesson). Internalized; matches the corrected batch size from S-04 part 2 P0 #4.
- **Tenant-scoped DELETE/UPDATE strict** (§6.1.1; Lote 10.4bis WI-S04-005 lesson). Carried forward.
- **Stagger alarms across regions** (§6.1.1; Lote 10.4bis WI-S04-005 P1 #8 lesson). Internalized — addresses the D1 lock contention spike from 5 regions firing concurrently.
- **4-tier P0/P1/P2/P3 incident classification with concrete examples** (§1 narrative point 7): "P0: cross-tenant chunk leak; manifest forge; sweeper cron stale > 1h. Resets clock; SEV-0. P1: throughput < 50 MB/s sustained; SLO breach. Resets clock. P2: tampering counter increment; orphan rate > 1% sustained. Resets clock. P3: dedup ratio < 1.2× sustained; metric drift. Does NOT reset." This is materially better than S-04 part 2 (which left it subjective). **But still missing**: who classifies P1 vs P2 vs P3 in real time? See P0 #4 below.
- **Crypto SME mandatory non-waivable** (§9.5; explicitly addresses the S-04 part 2 P0 #2 contradiction WI-004 vs WI-006). The "WI-006 PRR ceremony references WI-002 + WI-005 SME sign-offs" framing is correct: SME signs the cripto WIs pre-PRR; PRR is integration validation. Internal consistency restored.
- **`validate_inv_promotion.py` CI gate** (§9.7; Lote 10.4bis introduced gate). Explicit. Addresses the S-04 part 2 P1 #13 persistent program gap ("CI gate WI INV declarations exist in registry §3.15"). Internalized.
- **Gradual production rollout 10%→50%→100%** (§9.8; Lote 10.4bis lesson). D+9 = 10%, D+11 = 50%, D+13 = 100%, D+20 post-ship review.
- **DASH-MULTIPART 10 panels enumerated** with PagerDuty + Slack alerts (§6.1.5). Better than S-04 part 2 DASH-AC 8 panels — adds REAPI conformance status panel + manifest sig invalid counter + sweeper tick rate panel. Operationally specific.
- **RB-FM-060 dry-run with concrete time-track targets** (§6.1.2): detection ≤ 5min, remediation ≤ 30min, customer comm ≤ 1h. Same operational rigor as WI-S04-006 RB-FM-303.
- **5 ADRs ratificação enumeration** (§6.1.9): ADR-0022 (chunk vs part decoupling), 0038 (handler invariants), 0039 (chunker public API), 0040 (multipart D1 sharding), 0041 (manifest API). **But:** see P0 #3 below for the rubber-stamp concern carried over from prior sprints.

**P0 — must fix before Lote 10.5bis SEAL:**

1. **160 GiB stitched flow tests against a `meta_manifests` schema that doesn't exist in this sprint.** §6.1.3: "D1 schema additions (forward S-14 OR S-09): meta_manifests table com (tenant_id, blob_digest, child_manifest_digests[])". WI-S05-004 §6.2 lists out-of-scope: NOT in WI-S05-004. Sprint contract S-05 §4 CAP table doesn't list meta-manifest as a CAP. **Yet WI-006 §1 + §6.1.3 declares the integration test (200 GiB synthetic blob, stitched flow) as a DoD gate for ship.** The integration test would have to either: (a) ship the meta_manifests schema AS PART OF S-05 (currently un-committed); (b) hard-code the meta-manifest as an in-memory data structure for the test and not persist to D1 (test passes; production has no persistence layer; stitched flow is undeployable post-S-05); (c) defer the integration test to the sprint that ships meta_manifests (S-09 or S-14). Currently the spec is **simultaneously** asserting the test runs in S-05 AND that the schema lands in a future sprint. Pick one. Recommended: ship a minimal `meta_manifests` schema as an addendum to migration 004 or a new migration 005 in WI-S05-004 (cost: ~2-4h), AND register `meta-manifest-sig` HKDF info domain in WI-S05-005 sig module. Without this, the 160 GiB stitched flow gate is **untestable in production** — the integration test can pass in CI with mocked persistence, ship would silently regress.

2. **REAPI conformance "10 enumerated tests covering AC subset" — wait, AC subset?** §6.1.4: "Subset enumerated: 10 conformance tests covering AC subset (lesson Lote 10.4bis WI-S04-006 pattern)." **AC = Action Cache**. But this is the multipart sprint (CAS / SplitBlob / SpliceBlob). Conformance subset should be the **CAS multipart subset**, not AC subset. Either: (a) typo (recommended fix; should read "CAS subset"); (b) intentional reuse of S-04 AC conformance suite which is unrelated to multipart functionality. The §8 Gherkin scenario "REAPI conformance 100% green" says "10 enumerated SplitBlob/SpliceBlob conformance tests (ADR-0038 Annex)" — this is correct (CAS subset); the §6.1.4 narrative says "AC subset" — this is wrong. Pin and fix. **Crypto SME / Architect will catch this on first read** and ask which surface is being conformance-tested.

3. **ADR-0040 (multipart D1 sharding) "ratificada in this sprint" is the same procedural-rubber-stamp risk as WI-S04-005 ADR-0019.** §6.1.9: "ADR-0040 (multipart D1 sharding): ratificada em WI-S05-004." But WI-S05-004 §13 lists ADR-0040 as "forward" (not designed); §22 lists 250M / 37 GB scale with "sharding mandatory; ADR-0040 forward para multipart sharding". WI-006 §6.1.9 then claims ADR-0040 is RATIFICADA at ship gate. **What is being ratified if the sharding strategy isn't designed yet?** Same defect class as the WI-S04-006 P0 #3 ("Crypto SME mandatory vs advisory contradiction") and the procedural rubber-stamp prevention referenced as a lesson. Either: (a) design ADR-0040 fully in S-05 (sharding key chosen: per-tenant_tier OR per-region; with rationale + storage projections + migration plan to first shard split), with WI-S05-004 producing the ADR text and WI-S05-006 confirming ratification; (b) drop ADR-0040 from the 5-ADR list and treat it as forward; ship S-05 with documented D1 fragility. Currently the spec text is "ratificação confirmation" without the design content. **The Lote 10.4bis "PRR sign-off rubber-stamp prevention" lesson is being applied to sign-offs but NOT to ADR ratificações — same anti-pattern.**

4. **4-tier P0/P1/P2/P3 classification: who classifies in real time?** §1 narrative point 7 enumerates with examples but doesn't pin the classifier role. S-04 part 2 P0 #4 finding ("a single-engineer Owner can self-classify any incident as P3 to keep the clock running — defeats the gate") repeats here. Mitigation propose:
    - On-call engineer makes initial classification (≤ 5 min response).
    - SRE Lead OR Security Lead confirms within 30 min (joint required for P0/P1).
    - Auto-classification rules: any DASH-MULTIPART alert sustained > 5min → automatic P2 (resets clock); any cross-tenant alert (any duration) → automatic P0 (resets clock); manifest sig invalid counter > 0 sustained → automatic P1 (resets clock).
    
    Currently the spec lists the four tiers but the classification process is implicit. Add explicit classification protocol to §6.1.6 OR to the runbook RB-FM-060 (currently RB-FM-060 §2 dry-run focus, not classification protocol). **Without classification protocol, a Gustavo-only sprint can self-classify any incident as P3 indefinitely.**

5. **PRR sign-off staffing reality: 10-of-13 unfilled — same gap as S-04 part 2, carried forward unfixed.** §30 sign-off table:
    - Owner / Final Approver (Gustavo) — 1-2 = same person
    - SRE Lead — `_staffing-blocked; ADR-0034 waiver via Architect compensation`
    - Security Lead — `_TBD; mandatory_`
    - Engineer × 2 — `_TBD; mandatory_`
    - QA — `_TBD; mandatory_`
    - Product (Gustavo) — same as Owner
    - Compliance — `_TBD; mandatory — SLSA L3_`
    - Privacy — `_TBD; mandatory — PII redaction_`
    - Architect — `_TBD; mandatory — ADR ratificações + handler trait + ADR-0040_`
    - AppSec — `_TBD; mandatory emphatic — tampering detection + 5-layer + bucket ACL_`
    - Crypto SME — `_MANDATORY (não advisory)_`
    
    That's 1 person (Gustavo) covering 4 roles + 9 unstaffed roles. R-008 mitigation "2-week advance booking; lesson Lote 10.4bis 40-80h" is paper. **Sprint cannot ship without identified humans.** R4 round 3 flagged this exact pattern; Lote 10.4bis was supposed to address with ADR-0034 staffing waiver — but ADR-0034 only WAIVES the SRE Lead role; it doesn't fill 9 other unfilled mandatory roles. Either: (a) explicit retention plan with names/firms (contractor pool) by D-0 sprint kickoff; (b) sprint contract amendment reducing mandatory count; (c) accept staffing-blocked SEAL slip (likely correct outcome given solo-engineer reality). **Cannot ship without resolving.** This issue is **identical** to S-04 part 2 P0 #3 — the program needs a system-level staffing fix, not per-WI patches.

**P1 — fix this sprint, before PRR:**

6. **Cumulative INV §3.16 promotion: "10 INVs" in §6.1.8 vs 13 INVs in §12 inventory.** §6.1.8 numerical claim "(10 INVs)" but counts 3+3+2+2+3 = **13** distinct INV-MULTIPART invariants. §12 lists 13 NEW (matching) plus 3 mantidos (CAS-INTEGRITY, CAS-IDEMPOTENCY, TENANT-ISOLATION). Header field says "10 INVs". Title field says "10 INVs". §10.s05.006.8 says "10 INVs §3.16 promovidas". §11 DoD says "10 INVs §3.16 promovidas + CI gate green". **The number 10 is wrong consistently across the spec.** Fix to 13.  validate_inv_promotion.py CI gate would catch the count drift but only if the gate input matches the registry; the spec self-inconsistency remains.

7. **Production rollout 10%→50%→100% with 24h interval lacks SLO-budget advancement criteria.** §29 review checkpoints: D+9 = 10%, D+11 = 50%, D+13 = 100%. **What signal advances 10%→50%?** Same defect as S-04 part 2 P1 #6 (carried forward). Recommend: (a) error budget < 50% consumed during 24h; (b) cache hit ratio ≥ 70% at the % under load; (c) zero cross-tenant alerts; (d) cost per-op within +10% of baseline; (e) zero sweeper-stale alerts. Currently advance criteria are calendar-based.

8. **Synthetic Bazel workload calibration: ST-008 = 3h.** §17 sub-task: "72h SLO continuous staging setup: 3h". This is the same WI-S04-006 ST-009 = 4h finding (carried forward, slightly worse). Calibrating a synthetic workload that produces ≥ 70% cache hit ratio + ≥ 1.5× dedup ratio against real Docker layer / ML model file workloads is realistically 20-40h. Block fix: pre-PRR Product+Architect review of the workload definition.

9. **REAPI conformance test set "10 enumerated" but ADR-0038 Annex content unverified.** §6.1.4 + Gherkin: "10 enumerated tests covering SplitBlob (5 variants) + SpliceBlob (5 variants); pinned commit em ADR-0038 Annex". S-04 part 2 P0 #5 forced pinning the test set list with commit hash. WI-S05-006 inherits the same defect: the actual 10 test names are NOT in the WI; they're "in ADR-0038 Annex" (ADR-0038 is RATIFICADA in WI-005-006 §6.1.9, but the Annex content is undocumented in this WI). Pin the 10 test names + bazelbuild/remote-apis commit hash in WI-S05-006 §6.1.4 directly OR cross-reference ADR-0038 Annex with explicit assurance the Annex contains the list. Currently unverifiable.

10. **DASH-MULTIPART alert thresholds derived from synthetic workload baseline** (per S-04 part 2 P1 #9 finding). Currently §6.1.5 lists 10 panels with alert assertions ("Orphan rate per region (alert if > 1% sustained)") but the 1% threshold is not justified by chaos-test calibration. Pre-PRR, define alert thresholds via 7-day synthetic baseline.

11. **PRR meeting 2h structured for HIGH_RISK 13 sign-off** (S-04 part 2 P2 #19 finding carried forward). 2h ÷ 13 sign-offs ÷ 5 prior WIs = 1.8 minutes per sign-off per WI. Realistic minimum 4h. §17 ST-010 = 2h.

12. **Cost regression gate aggregated across all 6 WIs** (§6.1.10 + §10.s05.006.10). Per-Split ≤ $0.000040; per-Splice ≤ $0.000010; per-chunker ≤ $0.000001; per-multipart-op ≤ $0.0000045; per-D1 op ≤ $0.000001; per-manifest verify ≤ $0.000002; per-build ≤ $0.000005. **All independent gates** but the cost panel (§6.1.5 panel 9) shows "cost per-op tracker" — singular. Define the aggregated cost-per-multipart-Split = sum-of-component-costs and gate THAT. Currently 7 independent gates without aggregate; one component regressing 50% while another regresses -50% would pass each gate but customer-facing per-op cost regressed.

13. **Customer comm "ready" gate: SLA addendum + release notes + Bazel onboarding doc** (§6.1.11). Reviewed by Product (Gustavo) + Compliance (TBD) + Privacy (TBD). With Compliance and Privacy unstaffed, the "review" reduces to Owner-only. Same gap as S-04 part 2 P1 #11. Document the actual review chain.

14. **Sweeper cron tick rate alert "if < 1/h sustained"** (§6.1.1 metric). 60-min detection lag for cron silent death is the same lag as S-04 part 2 P1 #5 (60-min). For HIGH_RISK 1h is too long. Add: external watchdog cron (CF Cron Trigger global, every 10min) checks per-region last_tick metric and forces manual alarm if stale. Reduces detection lag to ~10-15 min.

**P2 — next sprint or doc-only:**

15. §15 chaos count: 12 — meets SOTA bar; matches WI-S04-006. Good.

16. §17 sub-task estimates total ~72h Optimistic; PERT 78h. Matches sprint contract §12 PERT WI-S05-006 = 19h... wait, **mismatch**: sprint contract says WI-S05-006 PERT = 19h; WI-006 spec §19 says 78h. 4× discrepancy. Either sprint contract is stale or spec is over-budgeted. For a PRR ship gate covering sweeper + 160 GiB stitched + REAPI conformance + DASH + 72h SLO + PRR ceremony + 5 ADR ratificações + 10 INVs + customer comm + production rollout, 19h is wildly optimistic. 78h is probably correct. Reconcile sprint contract §12 with WI spec §19 — same defect class as S-04 (sub-task estimate inconsistency).

17. §28 R-002 "PRR sign-off rubber-stamp | M | M | HIGH | M | LOW" — the residual LOW assumes "Per-role checklist evidence; lesson Lote 10.4bis" mitigation is effective; but the actual program has not yet validated this on a real PRR (S-04 hasn't shipped yet). Lower confidence on the LOW residual; track empirically.

18. §28 R-008 "Crypto SME unavailable on ship date | M | L | MEDIUM | L | LOW" — Probability M, but the actual SME engagement hasn't been booked yet (S-04 part 2 P2 #18 finding — 4h ST-018 budget vs 40-80h industry-norm). Carry forward; risk Probability is realistically H not M.

19. §22 cost analysis $9k Y1 + $1k/yr — excludes Crypto SME post-ship review/retainer (~$5-10k/yr) + chaos engineering ongoing ($2-5k/yr). Reconcile to ~$15-20k Y1.

20. ADR-0034 staffing waiver path is referenced (R-008 mitigation, sign-off table SRE Lead row) but its actual content/scope hasn't been verified in this review; ADR-0034 should explicitly enumerate which roles can be waived and under what compensating control.

21. §27 Knowledge Transfer: Onboarding test 10 questions — listed but not authored. List the 10 questions. (Same finding as S-04 part 2 §27 Onboarding test).

---

## Cross-WI Consistency Check (S-05 cumulative + handoffs WI-001/002/003)

**Dependency graph correctness:**
- WI-006 declares hard-blockers on WI-001..005 SEALED — declared correctly (§18).
- WI-005 declares hard-blocker on `blake3, subtle, hkdf` crates + WI-S04-004 SignatureVerifier trait reuse + WI-S05-002 chunker output — declared correctly (§18).
- WI-004 declares hard-blocker on D1 framework + R2 access + wrangler 4.x + soft on WI-S04-002 schema pattern — declared correctly (§18).
- WI-005 declares **soft** blocker on WI-005-001 handler consumption + WI-005-006 conformance — should be hard for handler (handler verify path is the security control); minor.

**ADR whitelisting:**
- ADR-0022 (WI-002 chunker chunk vs part decoupling)
- ADR-0038 (WI-001 handler invariants)
- ADR-0039 (WI-002 chunker public API stability)
- ADR-0040 (WI-004 multipart D1 sharding)
- ADR-0041 (WI-005 manifest API stability)
- 5 ADRs total, RATIFICADAS at ship gate (§6.1.9 of WI-006). Plus referenced ADR-0034 (staffing waiver). 6 ADRs touched. WI-006 §6.1.9 says "all whitelisted em validate_references.py". Verify against actual `scripts/validate_references.py` — same audit defect class as S-04 part 2 (count drift).
- **Important: ADR-0040 ratificação is procedurally hollow** (P0 #3 above) — sharding strategy not designed.

**Sign-off harmonization:**
- WI-S05-004: Architect + DBA + AppSec mandatory (§30 row 11-13). DBA advisory.
- WI-S05-005: Architect + AppSec + Crypto SME mandatory emphatic (§30 row 11-13). Security Lead mandatory.
- WI-S05-006: 12 mandatory + Crypto SME mandatory non-waivable = 13 (§30). Crypto SME consistency restored vs S-04 contradiction. Good.
- WI-S05-001/002/003 (read for cross-consistency): all consistent with mandatory Crypto SME for cripto WIs.

**Shared technical primitives:**
- `tenant_prefix BLOB(16) materialized` — WI-005-004 schema; consumed by WI-S05-006 sweeper; WI-S05-001 / WI-S05-003 R2 path layer 4. Consistent. Closes the WI-S04-005 P0 #1 gap (carried forward correctly).
- `MAX_CHUNK_COUNT = 80000` (WI-005-005 manifest) vs `chunk_index <= 80000` (WI-005-004 CHECK). Off-by-one issue noted (P1 #5 in WI-005-004; P1 #11 in WI-005-005). Inconsistent; align all.
- `MAX_TOTAL_SIZE = 160 GiB` (WI-005-005 bound) vs sprint contract §5.1 "160 GiB max single multipart". Consistent.
- HKDF info=`b"manifest-sig"` (WI-005-005) ≠ `b"ac-sig"` (WI-S04-004) ≠ `b"meta-manifest-sig"` (WI-005-006). Domain separation correct; CI byte-equal test in WI-005 chaos #4. Add prefix-distinctness test (P0 #5 in WI-005-005).
- `result_hash = merkle_root` direct (WI-005-005 §9.6; Lote 10.4bis lesson). Consistent with WI-S04-003 fix. Carried forward correctly.
- 5-region sharding (WI-S05-006 sweeper; WI-S05-004 R2 path region IN ('sam', 'iad', 'lhr', 'nrt', 'syd')). Consistent.
- Bounded batch 250 sessions/tick (WI-S05-006 sweeper; WI-S05-004 SLA addendum). Consistent.

**Forward INVs declared (§3.16 promotion via WI-006):**
- 13 INVs listed in §12 of WI-006 (matching the 3+3+2+2+3 distribution). Header field says 10. **Self-inconsistency** (P1 #6).
- Cross-WI source attribution checks out.
- `validate_inv_promotion.py` CI gate referenced — addresses the S-04 part 2 P1 #13 persistent program gap. **This is the highest-leverage program improvement; verify the gate actually exists in `scripts/`.**

**Schema cross-coupling:**
- `meta_manifests` table referenced in WI-S05-006 §6.1.3 as "forward S-14 OR S-09" — schema NOT in WI-005-004. **Integration test 200 GiB stitched depends on it**. P0 #1 in WI-005-006.
- `chunks.refcount` semantics WI-005-004 §1 narrative point 5 — S-06 GC race not addressed. P0 #4 in WI-005-004.

**TLA+ alignment:**
- WI-005 §12 references "cas_integrity.tla chunked variant (forward S-09 TLA+ work)". Same forward-deferred status as S-04. The "TLA+ verde" DoD item (none in S-05 explicitly; sprint contract §6 doesn't list TLA+ as DoD) is OK by absence. No regression here vs S-04.

**Staging environment ownership:**
- All three WIs assume `staging.corelink.humangr.com` exists with full S-04 + S-05 stack deployed. No sub-task provisions/maintains staging. Same sprint-level dependency unsurfaced as S-04 part 2 (carried forward).

---

## Comparison vs Canonical SOTA Bar (WI-S04-003 best-in-class 8.6)

WI-S04-003 (Merkle envelope dual-side verify) earned 8.6/10 by:
- Strongest cripto rigor in the program (RFC 6962 leaf/inner; bounded parser; cycle detection; Annex A+B test vectors; cargo-fuzz).
- result_hash = merkle_root direct (eliminating protobuf-determinism dependency; this was the round-3 P0 fix).
- Streaming progressive verify with mid-stream fail-fast.
- HKDF info domain separation `b"ac-sig"` codified in CI byte-equal test.
- Zero allocation hot path; `#[forbid(unsafe_code)]`; semver discipline.

**WI-S05-005 vs S04-003 bar:**
- Strengths matching 8.6: BLAKE3 + RFC 6962 + bounded parser + streaming progressive + cargo-fuzz × 3 (now 4 with builder addition recommended) + Annex A+B + ADR forward + sig domain separation. Internalizes Lote 10.4bis lessons.
- Below 8.6: **`canonical_bytes()` layout NOT published** (P0 #1 — same defect class as WI-S04-004 round 3); tree-shape ambiguity sort-by-index vs lex-sort (P0 #2); streaming verify abort signal mechanism unspecified (P0 #3); Mann-Whitney cripto-grade |Δmedian| ≤ 1ms tension with fail-fast (P1 #6); MAX_CHUNK_COUNT = 80000 off-by-one with 160 GiB (P1 #11).
- **Concrete improvements to reach 9.0**: (a) publish canonical_bytes layout in §1 (highest leverage; 4h fix); (b) pin tree shape "sort by chunk_index ascending; no sorting; leaves are in concatenation order" with property test; (c) pin streaming verify abort mechanism (CancellationToken or AbortHandle pair); (d) reframe Mann-Whitney to per-chunk leaf-verify timing equality (constant-time primitive) instead of whole-manifest timing; (e) align MAX_CHUNK_COUNT = 81920 (or accept 80000 with documented 156.25 GiB max blob).

**WI-S05-004 vs S04-003 bar (schema vs cripto — different domains; calibrate to WI-S04-002 schema pattern instead):**
- Strengths matching/exceeding S04-002 baseline (which was 8.0): CHECK inline correctly applied; PK composite tenant-first; tenant_prefix BLOB(16) materialized (CLOSES the WI-S04-005 P0 #1 gap); ALTER TABLE ADD COLUMN justified; storage projection numeric; ADR-0040 forward (but rubber-stamp risk — see P0 #3 in WI-006).
- Below 8.5: size_bytes 4 MiB upper bound CHECK with chunker-mode mismatch (P0 #1); UNIQUE multipart_sessions semantic ambiguity 409-vs-retry (P0 #2); ADR-0040 procedural rubber-stamp (P0 #3 carried into WI-006); refcount race S-06 GC not addressed (P0 #4); Wrangler binding stub incomplete (P1 #6); chaos count at floor (5 vs program peer 12).
- **Concrete improvements to reach 8.7**: (a) fix the 4 P0s; (b) bump chaos to 8-10; (c) bump risk register to 12 with refcount-race; (d) DBA review iter realistic 8-16h.

**WI-S05-006 vs S04-006 bar (PRR ship gate):**
- Strengths matching/exceeding 7.8 baseline: 4-tier P0/P1/P2/P3 enumerated with examples (S-04 was subjective); Crypto SME mandatory non-waivable consistent across WIs (S-04 had contradiction); validate_inv_promotion.py CI gate (closes S-04 P1 #13 persistent gap); alarm re-arm AT START of tick (closes WI-S04-005 P1 #5); bounded batch 250 (closes S-04 P0 #4); stagger alarms (closes WI-S04-005 P1 #8).
- Below 8.5: meta_manifests schema NOT in this sprint but integration test depends on it (P0 #1 — new defect, not from S-04); REAPI conformance "AC subset" typo in §6.1.4 (P0 #2); ADR-0040 procedural rubber-stamp (P0 #3); 4-tier classification protocol unpinned (P0 #4); 10-of-13 sign-off staffing (P0 #5 — carried forward unfixed); 10 INVs vs 13 INVs self-inconsistency (P1 #6); production rollout temporal not telemetric (P1 #7 — same as S-04 P1 #6 carried forward); REAPI 10 enumerated tests not in WI (P1 #9 — same as S-04 P0 #5 carried forward partial); cost gate aggregation missing (P1 #12); customer comm review chain unstaffed (P1 #13).
- **Concrete improvements to reach 8.7**: (a) ship `meta_manifests` schema in WI-005-004 OR defer 200 GiB integration test; (b) fix REAPI "AC subset" → "CAS subset" typo; (c) design ADR-0040 fully OR drop from ratificação list; (d) pin 4-tier classification protocol with auto-rules; (e) staffing reality plan with names by D-0; (f) fix "10 INVs" → "13 INVs" everywhere; (g) telemetric advancement criteria for rollout; (h) pin REAPI 10 test names + commit hash directly.

---

## Verdict per WI + Aggregate

| WI | Score | Verdict | Highest-Risk Item |
|---|---|---|---|
| WI-S05-004 | 8.0 | **GO-WITH-FIXES**; tightest schema spec the program has produced; ADR-0040 must be designed-not-rubber-stamped | UNIQUE multipart_sessions 409-vs-retry semantics ambiguity (P0 #2); ADR-0040 procedural rubber-stamp (P0 #3) |
| WI-S05-005 | 8.2 | **GO-WITH-FIXES**; cripto rigor real; canonical_bytes layout MUST be published before Crypto SME review | `canonical_bytes()` layout NOT published (P0 #1); tree-shape sort-by-index vs lex-sort ambiguity (P0 #2); streaming verify abort mechanism unspecified (P0 #3) |
| WI-S05-006 | 7.9 | **GO-WITH-FIXES**; ship gate more realistic than S-04's; staffing reality + meta_manifests schema gap unresolved | meta_manifests schema NOT in S-05 but integration test depends on it (P0 #1); 10-of-13 sign-off staffing carried forward (P0 #5) |

**Aggregate part 2 score: 8.05/10** = (8.0 + 8.2 + 7.9) / 3.

Comparison points:
- S-03 part 1 average: 7.6/10
- S-03 part 2 average: 7.95/10 (WI-S03-007 best 8.5)
- S-04 part 1 average: 8.05/10 (WI-S04-003 best 8.6)
- S-04 part 2 average: 7.83/10
- **S-05 part 2 average: 8.05/10** (no WI reaches 8.5; WI-005 closest at 8.2)
- User's SOTA target: 9-10. **Gap to target: ~1.0 points; closable with Lote 10.5bis P0 fixes.**

The trio shows real maturation over S-04 part 2 (+0.22 points) — Lote 10.4bis lessons internalized at the *spec language level*, not just claimed in headers. But none reaches 8.5 (the program's WI-S04-003 best-in-class), and three of the program's persistent defects carry forward unfixed: (a) ADR ratificação procedural rubber-stamp (now in ADR-0040 and previously in ADR-0019), (b) PRR sign-off staffing reality (now 10-of-13 unfilled, was 9-of-13 in S-04), (c) integration tests depending on schemas not yet shipped (meta_manifests, new this sprint). These need a system-level fix.

---

## Lote 10.5bis P0 Fix Plan

### Day 0 (immediate; sprint-level resolution before fix-cycle)

1. **Resolve `meta_manifests` schema decision.** Either: (a) ship minimal `meta_manifests` schema as addendum to migration 004 in WI-S05-004 (cost: ~2-4h; adds (tenant_id, blob_digest, child_manifest_digests[]) table + sig domain `b"meta-manifest-sig"` registration in WI-S05-005); (b) defer 200 GiB stitched integration test to S-09 / S-14 sprint that ships `meta_manifests`; document S-05 ships with stitched flow as design-only. **Recommended (a)** — small schema addition unblocks the GA promise of "any blob ≤ 5 TiB" advertised in customer SLA.

2. **Resolve PRR sign-off staffing reality.** Sprint-level decision: (a) explicit retention plan with names/firms (contractor pool + Crypto SME 40-80h booking confirmed) by D-0; (b) amend sprint contract §19 to reduce mandatory sign-off count given solo-engineer reality; (c) accept staffing-blocked SEAL slip past sprint kickoff. **Cannot ship without.** This is identical to S-04 part 2 P0 #3 — same defect class.

3. **Resolve ADR-0040 procedural rubber-stamp.** Either: (a) design ADR-0040 fully in WI-S05-004 (sharding key chosen with rationale + projected first-shard-split timeline + migration plan); (b) drop ADR-0040 from §6.1.9 5-ADR ratificação list and document S-05 ships with known-fragile D1 backend (37 GB target / 10 GB hard limit / 80% alert). **Recommended (a)** — sharding decision deferred is a foot-gun for S-06 / S-07.

### Day 1 (WI-S05-005 cripto correctness fixes)

4. **Publish `canonical_bytes()` layout in §1** — explicit byte layout (~86 bytes + tenant_id length); add to Annex A test vectors as canonical_bytes hex per vector; align with WI-S04-004 §6.1.5 layout style. **4h fix; highest-leverage Lote 10.5bis action.**

5. **Pin tree-shape ordering.** Rewrite §1 tree shape: "balanced binary tree over chunks in chunk_index ascending order; NO sorting; leaves are `hash(\x00 || chunks[i].digest)` for i in 0..N." Remove any "lex sort" reference inherited from WI-S04-003 pattern reuse. Add property test `prop_manifest_chunk_order_preserved`.

6. **Pin streaming verify abort mechanism.** Recommend `CancellationToken` parameter on `verify_streaming` OR `(Result, AbortHandle)` return. Update §1 trait + §6.1.7 property test + §1 narrative point 7.

7. **Fix MAX_CHUNK_COUNT off-by-one.** Either MAX_CHUNK_COUNT = 81920 (160 × 1024 / 2 exactly) OR document max blob = 156.25 GiB single multipart. Update sprint contract §5.1 + WI-S05-004 schema CHECK + WI-S05-001 narrative + WI-S05-005 bounds + WI-S05-006 stitched threshold. **Cross-WI change; coordinate.**

8. **Reframe Mann-Whitney cripto-grade gate.** Either: (a) per-chunk leaf-verify timing equality |Δmedian| ≤ 0.5ms (constant-time primitive); (b) keep whole-manifest gate at realistic |Δmedian| ≤ 10ms; (c) require timing-equal manifest verify (force full tree walk on tampered inputs). Pick one and document.

### Day 2 (WI-S05-004 schema fixes)

9. **Resolve UNIQUE multipart_sessions 409-vs-retry semantics.** Pick retry-on-existing (return existing upload_id) — matches S3 / R2 multipart idempotency contract. Update §8 Gherkin + WI-S05-001 handler narrative.

10. **Add refcount-race row to risk register.** S-06 GC vs SplitBlob concurrent INSERT — optimistic concurrency `WHERE refcount = 0` on GC DELETE. Carry-forward to S-06 ADR.

11. **Fix size_bytes CHECK comment.** Comment should read "size 1 byte to 4 MiB max" or rationalize to fixed-2-MiB-with-final-partial bounds.

12. **Bump chaos count to 8-10** (currently 5; below program peer).

### Day 3 (WI-S05-006 ship gate fixes)

13. **Fix REAPI conformance "AC subset" → "CAS subset" typo** in §6.1.4. Likely a one-line copy-paste from WI-S04-006.

14. **Pin REAPI 10 test names + commit hash** directly in WI-S05-006 §6.1.4 (not "in ADR-0038 Annex"). Same fix class as S-04 part 2 P0 #5 — carry-forward gap; close definitively.

15. **Pin 4-tier classification protocol.** Add explicit rules: on-call engineer initial classification ≤ 5 min; SRE Lead OR Security Lead confirms within 30 min; auto-rules on DASH-MULTIPART alerts (sustained > 5min → P2; cross-tenant alert → P0; sig invalid sustained → P1).

16. **Replace temporal advancement with telemetric** for rollout: error budget < 50%, cache hit ratio ≥ 70%, zero cross-tenant alerts, cost +10% of baseline.

17. **Fix "10 INVs" → "13 INVs"** consistently across §1 title, §6.1.8, §10.s05.006.8, §11. Verify validate_inv_promotion.py count matches.

### Day 4 (cross-WI hygiene)

18. **Add CI byte-equal test for HKDF info string distinctness** across `b"ac-sig"`, `b"manifest-sig"`, `b"meta-manifest-sig"`. Assert all three present + distinct + non-prefix.

19. **Add cargo-fuzz target #4 (builder/encode)** in WI-S05-005.

20. **Reconcile sprint contract §12 PERT** WI-S05-006 = 19h vs spec §19 = 78h. Update sprint contract.

21. **Verify validate_inv_promotion.py CI gate exists** in `scripts/`. If not, ship the gate as part of Lote 10.5bis (this closes a 4-sprint persistent program gap).

22. **External watchdog cron** for sweeper liveness — CF Cron Trigger global, every 10 min, checks per-region last_tick metric. Reduces detection lag from 60 min to 10-15 min.

### Day 5 (re-review by R4)

Re-run targeted re-review on the touched sections; aim for WI-S05-004 → 8.5+, WI-S05-005 → 8.7+ (canonical_bytes + tree-shape are highest-leverage), WI-S05-006 → 8.5+ (assuming meta_manifests + ADR-0040 + staffing all resolved). Aggregate target: **8.5+/10**. Closes most of the gap to the 9-10 SOTA target; remaining gap held by program-level structural items (registry actually updated post-SEAL; actual staffed sign-offs; actual TLA+ specs that exist).

---

## Final Verdict

**GO-WITH-FIXES** for all three WIs; **do not SEAL Lote 10.5 without addressing the 12 P0 items** (3 sprint-level + 4 WI-005 + 4 WI-004 + 5 WI-006, with overlap). 

The trio reflects **demonstrated maturity gain** over S-04 part 2: Lote 10.4bis lessons are internalized at the spec-language level (CHECK inline applied; tenant_prefix BLOB(16) materialized closing WI-S04-005 P0 #1; alarm re-arm AT START closing WI-S04-005 P1 #5; bounded batch 250 closing S-04 P0 #4; stagger alarms closing WI-S04-005 P1 #8; Crypto SME mandatory non-waivable consistent across WIs closing S-04 P0 #2; 4-tier classification with examples closing S-04 P0 #4; validate_inv_promotion.py CI gate closing S-04 P1 #13; gradual rollout 10%→50%→100% codified). The program is learning sprint-over-sprint.

But three classes of defect persist:
1. **ADR ratificação procedural rubber-stamp** — now ADR-0040 (S-05), previously ADR-0019 (S-04). The program needs a system-level rule "no ADR ratifies at PRR without designed content" — analogous to the sign-off rubber-stamp prevention but for ADRs.
2. **PRR sign-off staffing reality** — 10-of-13 unfilled in S-05, was 9-of-13 in S-04. Solo-engineer reality demands an explicit retention plan with names by D-0 sprint kickoff OR a sprint contract amendment OR accepted SEAL slip. Per-sprint paper mitigations are insufficient.
3. **Integration tests depending on schemas not yet shipped** — `meta_manifests` is the new instance this sprint; the cumulative pattern (specs assert tests against future schemas) is a recurring program gap that needs a CI rule.

The single highest-leverage Lote 10.5bis action: **publish the explicit `Manifest::canonical_bytes()` layout in WI-S05-005 §1**. This is a 4h fix that closes the same defect class R4 round 3 forced into WI-S04-004 — and without it, Crypto SME independent review of WI-S05-005 cannot start. The second-highest: **ship minimal `meta_manifests` schema in WI-S05-004 OR defer 200 GiB integration test** — currently the spec asserts both, which is incoherent. The third: **resolve PRR sign-off staffing reality** at the sprint level — paper mitigations have failed across two sprints; system-level fix needed.

If Lote 10.5bis lands the 12 P0s + the 3 sprint-level resolutions, the trio reaches **8.5-8.7/10 average**, comparable to WI-S04-003's best-in-class. The remaining gap to 9-10 is held by (a) the 13 INVs actually being in registry post-SEAL (validate_inv_promotion.py gate must actually exist and run in CI; verify), (b) actual staffed sign-offs (not _TBD_; not paper retention plans), (c) the meta_manifests schema decision actually ship-coherent. Those are sprint-cycle structural items, not per-WI items.

The program is at **8.05 average for S-05 part 2** — the third-highest part-score in the program (behind S-04 part 1 at 8.05 — exact tie — and ahead of S-04 part 2 at 7.83 / S-03 part 2 at 7.95 / S-03 part 1 at 7.6). Trajectory is up. SOTA target 9-10 remains out of reach until the system-level fixes (ADR-design-not-rubber-stamp; staffing reality; INV registry CI gate proven in CI) are landed.

---

**Reviewer**: Agent R4 (Claude Opus 4.7, 1M context)
**File**: `/Users/gustavoschneiter/Documents/HuGR/corelink-server/specs/_audits/sealed/2026-04-25-agent-r4-s05-part2-wi-review.md`
