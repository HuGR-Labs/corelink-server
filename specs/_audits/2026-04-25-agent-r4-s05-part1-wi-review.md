---
id: "AUDIT-2026-04-25-AGENT-R4-S05-PART1"
type: "audit"
doc_status: "DRAFT"
audit_status: "ACTIVE"
created: "2026-04-25"
reviewer: "Agent R4 (Claude Opus 4.7, 1M context, independent reviewer — round 4)"
scope: "Lote 10.5 — Sprint S-05 Part 1 (WI-S05-001 .. WI-S05-003)"
sprint_contract: "specs/04_sprints/S05/_spec_contract.md v1.1.0"
calibration_baselines:
  - "specs/_audits/2026-04-25-agent-r4-s04-part1-wi-review.md (S-04 part1 8.05/10; WI-S04-003 = 8.6 best-in-class)"
  - "specs/_audits/2026-04-25-agent-r4-s04-part2-wi-review.md (S-04 part2 7.83/10)"
files_reviewed:
  - "specs/04_sprints/S05/work_items/WI-S05-001-reapi-splitblob-spliceblob-handlers.md (~887 lines)"
  - "specs/04_sprints/S05/work_items/WI-S05-002-corelink-chunker-fastcdc-adr-0022.md (~835 lines)"
  - "specs/04_sprints/S05/work_items/WI-S05-003-r2-multipart-adapter.md (~511 lines)"
cross_references:
  - "specs/04_sprints/S05/_spec_contract.md (v1.1.0; PRR HIGH_RISK 13)"
  - "specs/03_architecture/invariant_registry.md §3.16 (INV-MULTIPART-* promotions)"
  - "specs/03_architecture/error_taxonomy.md §3.9 (Multipart errors)"
  - "specs/03_architecture/remote_cache_product_profile.md (REAPI surface)"
  - "specs/03_architecture/adrs/ (ADR canonical path)"
---

# Agent R4 — Lote 10.5 S-05 Part 1 (WIs 001–003) WI Review

> **Reviewer**: Agent R4 (independent SOTA reviewer; ruthless, technical, no diplomacy).
> **Calibration target**: User directive "average não serve. SOTA puro 9-10 é o target." WI-S04-003 sat at **8.6/10 best-in-class**; S-04 part 1 average was **8.05/10**, S-04 part 2 was **7.83/10**.

---

## Veredito Geral

The S-05 Part 1 trio is the **first multipart triple in the program** and the most architecturally interesting work item set since WI-S04-003. The package shows clear maturity gains over S-04: the **Lote 10.4bis lessons have been internalized preemptively** (TenantCtx-only enforcement is consistent across all three WIs; `with_tenant_ctx!` is correctly absent and explicitly anti-patterned in WI-S05-001 §7; `audit_outbox` is correctly cited as WI-S01-004 throughout; D1 batch 100KB limit is correctly applied to manifest_chunks INSERT capped at 250 rows; tenant_prefix materialized column is referenced as the canonical Layer 4 mechanism; Wrangler `lifecycle add` is the correct CLI; CORS via REST API curl is documented; INV §3.16 is preemptively populated). The **invariant registry is fully aligned** with the WIs: every INV-MULTIPART-* claim in WI-001/002/003 is present and consistent in `invariant_registry.md §3.16`. The **ADR ratificação plan** for ADR-0022 is correct and supported by the sprint contract §4 (CAP-CAS-013). **WI-S05-002 in particular is the strongest of the three** — the determinism story is concrete, mask seeds versioning is explicit, the Iterator lifetime contract is honest, the cargo-fuzz harness is 1h CI nightly with two targets (Fixed + FastCDC), the test vectors Annex is sized for SLSA L3 reviewer reproducibility, and the BLAKE3 throughput claim is backed by a criterion bench gate. The Crypto SME mandatory-emphatic annotation is correctly emphasized. WI-S05-001 is comprehensive (gRPC + REST single trait, 12 chaos experiments, 14-row risk register, REAPI conformance harness, dual-side Merkle integration delegated correctly to WI-S05-005). WI-S05-003 is leaner (compact §9-32 form) but coherent — the orphan detection mechanism, ETag tracking discipline, cross-tenant upload_id binding, and Wrangler CLI corrections are all in place.

But four classes of defects keep the trio short of the **9-10 SOTA bar** and below the WI-S04-003 ceiling. **First (P0): error taxonomy drift, same defect class as WI-S04-001 not fixed in S-05.** `error_taxonomy.md §3.9` defines exactly **3 multipart codes** (`COR_MULTIPART_PART_MISSING`, `COR_MULTIPART_MERKLE_INVALID`, `COR_MULTIPART_TIMEOUT`); WI-S05-001 invents and uses **8 new** (`COR_MULTIPART_BLOB_TOO_LARGE`, `COR_MULTIPART_BACKEND_UNAVAILABLE`, `COR_MULTIPART_CONCURRENCY_LIMITED`, `COR_MULTIPART_ALGO_UNSUPPORTED`, `COR_MULTIPART_SIG_INVALID`, `COR_MULTIPART_CHUNK_MISSING`, `COR_MULTIPART_MANIFEST_NOT_FOUND`, plus `COR_AUTH_SCOPE_INSUFFICIENT` which is at least plausibly an auth-domain reuse) without amending the taxonomy; WI-S05-003 references `COR_MULTIPART_TIMEOUT` correctly but also `COR_MULTIPART_BLOB_TOO_LARGE`, `COR_MULTIPART_CONCURRENCY_LIMITED`, `COR_MULTIPART_BACKEND_UNAVAILABLE`, `COR_MULTIPART_PART_MISSING`. Lote 10.4bis flagged exactly this defect for WI-S04-001 and the WI authors carried the lesson forward in spirit (preemptive INV §3.16 population) but **failed to extend `error_taxonomy.md §3.9`** the same way. **Second (P0): the ADR storage path is wrong across the entire program.** All three WIs cite `specs/02_governance/decisions/ADR-XXXX.md`; the actual on-disk canonical ADR directory is `specs/03_architecture/adrs/` (verified: ADR-0012 .. ADR-0020 live there). This is not an S-05 defect specifically — S-04 has it too — but it is **inherited unchanged into S-05**, the WI-S05-002 ratificação plan literally writes "Update `specs/02_governance/decisions/ADR-0022-...md` from DRAFT to ACCEPTED" pointing to a file that does not exist at that path; the correct path would put ADR-0022 next to ADR-0019/0020. Either the program needs to migrate ADRs into `02_governance/decisions/`, or the WIs need to use `03_architecture/adrs/`. **Third (P0): the SpliceBlob streaming-verify boundary is described inconsistently across §1, §6.1.4, and §15 chaos #5.** §1 step [5] says "verify each chunk's hash matches manifest_chunks[i].chunk_digest (delegate WI-S05-005 verifier API)"; §6.1.4 step [5] says "Per-chunk hash verify in stream (BLAKE3 incremental) — fail-fast if mismatch"; §6.2 (out-of-scope) says "Streaming progressive verify (verify chunk hash mid-stream): partial in this WI; full em WI-S05-005." This is a **load-bearing security control** (the dual-side verify story for SpliceBlob; INV-MULTIPART-DUAL-SIDE-VERIFY + INV-MULTIPART-STREAMING-VERIFY-FAIL-FAST). "Partial in this WI; full em WI-S05-005" + "fail-fast if mismatch" are not compatible. Decide: either (a) WI-S05-001 ships streaming-verify-fail-fast and the chaos test #6 ("chunk missing mid-stream") covers it, or (b) WI-S05-001 ships only post-verify (verify all chunks before streaming any to client; this changes the latency profile dramatically for a 5 GiB blob), or (c) WI-S05-005 owns the verifier and WI-S05-001 only invokes it. The current spec has all three positions in different paragraphs. **Fourth (P0): bounded-concurrency rationale is shallow and the defaults are inconsistent across the trio.** WI-S05-001 §6.1.5 sets per-tenant SplitBlob semaphore to **4 default**; WI-S05-003 §6.1.4 sets per-tenant UploadPart semaphore to **8 default**. Both are stated as "default; tunable per-tier S-13 forward." WI-S05-001 §9.4 justifies 4 with "free=2, business=8, enterprise=16"; WI-S05-003 has no equivalent justification (just "8 default"). Are SplitBlob calls (which each spawn UploadPart sub-ops) supposed to compose with UploadPart concurrency? If a tenant's 4 SplitBlob calls each fan out 8 UploadParts, that's 32 R2 PUTs/sec — the underlying R2 rate-limit reasoning is missing. The trio reads as if WI-S05-001 and WI-S05-003 each picked their own semaphore size in isolation.

Beyond those four classes, **WI-S05-002 has a mathematical bug in the determinism property test claim** ("1000 random blobs × 100 chunkings = 100% byte-identical" — this is not an effective adversarial test for FastCDC; deterministic algorithms are byte-identical by construction; the property test should be **cross-version** or **cross-platform**, not "run the same code 100 times"); **WI-S05-002 §1's `feed()` signature is internally inconsistent** (returns `Box<dyn Iterator<Item = Chunk<'a>> + 'a>` but section §6.1.4 / SLA addendum / §22 cost analysis claim "zero-allocation Iterator" — `Box::new` is an allocation; either the signature is wrong or the claim is wrong); **WI-S05-002 cargo-fuzz throughput claim is unanchored** ("≥ 2 GB/s single core" is the BLAKE3 SIMD ceiling on x86-64 with AVX-512 — CF Workers do not expose AVX-512 to V8 isolates, and Rust→WASM throughput for BLAKE3 is closer to 500-800 MB/s — the bench may pass on the developer's M3 laptop and silently regress in production); **WI-S05-003 has a quietly-broken claim in §1 invariant 2** ("`UNIQUE (tenant_id, blob_digest)` em D1 multipart_sessions table prevents double-record" — but multipart_sessions stores `upload_id` not `blob_digest` for in-flight sessions; the natural UNIQUE is `(tenant_id, blob_digest, state='in_progress')` per `invariant_registry.md §3.16` row INV-MULTIPART-IDEMPOTENT, and a partial unique index in D1/SQLite requires `WHERE state='in_progress'` syntax — this needs to be specified in WI-S05-004 schema); and **WI-S05-001 §6.1.4 step [4] says "manifest::verify_signature (delegate WI-S04-004 SignatureVerifier trait pattern)" but WI-S04-004 is per-AC-envelope sig with `info=b"ac-sig"`** — a domain-separated `info=b"manifest-sig"` requires the SignatureVerifier trait either be (i) parameterized over `info`, (ii) duplicated for manifest, or (iii) have a `for_manifest()` constructor — none of which is documented in the WI-S05-001 inheritance chain.

The trio earns a **Part 1 average of 8.10/10** — slightly above the S-04 part 1 average (8.05) and confirming that the lessons-learned channel is working, but no individual WI clears WI-S04-003's 8.6 ceiling. WI-S05-002 lands at **8.4** (strongest; the chunker spec is clear, determinism rationale is concrete, the test vectors Annex + cargo-fuzz dual-target + ADR ratificação plan are exemplary; held back by the Iterator allocation contradiction, the BLAKE3 throughput portability hand-wave, and the property-test scope criticism). WI-S05-001 lands at **8.0** (comprehensive but the SpliceBlob streaming-verify boundary is muddled, the manifest-sig delegation API gap is unaddressed, the error taxonomy drift is the same defect class as WI-S04-001 unfixed). WI-S05-003 lands at **7.9** (cleanest sequence flow but the §9-32 compact form sacrifices several SOTA discipline elements — the chaos suite is 5 instead of ≥10, the risk register is 10 rows minimum, the §1 invariant 2 claim about multipart_sessions UNIQUE is wrong, the Crypto SME is "advisory" rather than mandatory which is defensible but not aligned with WI-S05-001's classification of ETag tracking as security boundary). All three are **GO-WITH-FIXES** for Lote 10.5bis; none is REJECT; **WI-001 must absorb a Lote 10.5bis P0 patch** (taxonomy + verify boundary + sig API delegation); **WI-002 should land an Iterator allocation correction + bench portability disclaimer**; **WI-003 needs the schema-side UNIQUE clarification + chaos suite expansion to ≥10**.

**Aggregate part 1 score: 8.10/10.**

---

## Per-WI Numerical Score

| WI | Score | Cripto | Complete | Clarity | SOTA | Internal | Prior-WI | Customer | Verdict |
|---|---|---|---|---|---|---|---|---|---|
| WI-S05-001 | **8.0** | 7.5 | 8.5 | 8.0 | 8.5 | 7.0 | 8.0 | 9.0 | pass-with-fixes (P0) |
| WI-S05-002 | **8.4** | 8.5 | 9.0 | 8.5 | 9.0 | 8.0 | 8.5 | 8.5 | pass-with-fixes-light |
| WI-S05-003 | **7.9** | 7.5 | 7.5 | 8.5 | 7.5 | 8.0 | 8.5 | 8.5 | pass-with-fixes |

**Average: 8.10/10.**

Breakdown axes (0-10):
- **Cripto rigor**: BLAKE3-keyed-hash inline + FastCDC determinism + bounded parser justifications (002 leads); manifest sig domain separation (`b"manifest-sig"`) is correct in 001 but the API delegation gap drops the score; ETag content-hash framing in 003 is light but defensible (R2 ETags are infrastructure-level integrity, not cripto control).
- **Completeness**: 32 sections explicitly numbered in 001/002; 003 collapses §9-32 into a compact form that reads cleanly but loses the chaos ≥10 + risk ≥10 SOTA discipline. 001 reaches 12 chaos / 14 risk; 002 reaches 11 chaos / 12 risk; 003 reaches 5 chaos / 10 risk.
- **Clarity**: 002 has the clearest sequence (chunker is a self-contained primitive); 001's Split/Splice flows are dense but readable; 003's §9-32 compact form is fine for a pure adapter but loses some specificity.
- **SOTA-adherence**: 13-row sign-off correctly itemized in all three (Crypto SME mandatory emphatic in 002, mandatory in 001, advisory in 003); Mann-Whitney 3-prong middleware-grade `|Δmedian| ≤ 5ms` in 001/002 (003 omits it which is reasonable for an SDK adapter); cost regression gate per-op in all three; cargo-fuzz 1h in 002 (correctly emphatic); property tests 10k+100k in 001/002/003.
- **Internal consistency**: 001 has the SpliceBlob verify boundary contradiction + sig delegation API gap; 002 has the Iterator allocation contradiction; 003 has the multipart_sessions UNIQUE-vs-blob_digest claim mismatch with WI-S05-004 schema.
- **Prior-WI consistency**: ADR path drift across all three (`02_governance/decisions/` vs actual `03_architecture/adrs/`); error taxonomy drift in 001 (8 undefined codes); 002 is mostly clean; 003 references `RB-FM-060` runbook that is not yet defined (sprint contract delegates to WI-S05-006); audit_outbox correctly cited as WI-S01-004 across all three (Lote 10.4bis lesson absorbed).
- **Customer-facing readiness**: persona narratives are concrete (Bazel CI Docker layer, ML model, compliance reviewer); SLA addenda explicit; SLSA L3 alignment for chunker (002) is a customer-facing differentiator; REAPI conformance suite invocation (001) is the customer-trust anchor.

---

## P0 Findings (must-fix before promotion to Lote 10.5bis)

### WI-S05-001 — 8 new `COR_MULTIPART_*` and `COR_AUTH_*` error codes are not defined in `error_taxonomy.md §3.9`

**Severity**: P0 (citation correctness; same defect class as WI-S04-001 carried forward). **WI**: WI-S05-001 §1, §6, §23.

`error_taxonomy.md §3.9` defines exactly **3 multipart codes**: `COR_MULTIPART_PART_MISSING`, `COR_MULTIPART_MERKLE_INVALID`, `COR_MULTIPART_TIMEOUT`.

WI-S05-001 invents and uses (without amending taxonomy):
1. `COR_MULTIPART_BLOB_TOO_LARGE` (413; §1 SplitError, §23)
2. `COR_MULTIPART_BACKEND_UNAVAILABLE` (503; §1, §23)
3. `COR_MULTIPART_CONCURRENCY_LIMITED` (429; §1, §23)
4. `COR_MULTIPART_ALGO_UNSUPPORTED` (422; §1, §23)
5. `COR_MULTIPART_SIG_INVALID` (422; §1 SpliceError, §23)
6. `COR_MULTIPART_CHUNK_MISSING` (422; §1 SpliceError, §23)
7. `COR_MULTIPART_MANIFEST_NOT_FOUND` (404; §1 SpliceError)
8. `COR_AUTH_SCOPE_INSUFFICIENT` (403; this one **may** be reused from `error_taxonomy.md §3.6` if defined there — verify; if not, it's a 9th undefined code).

WI-S05-003 also references several of the above (e.g. `COR_MULTIPART_TIMEOUT` is correctly reused but `COR_MULTIPART_BLOB_TOO_LARGE`, `COR_MULTIPART_CONCURRENCY_LIMITED`, `COR_MULTIPART_BACKEND_UNAVAILABLE`, `COR_MULTIPART_PART_MISSING` cascade through).

**Fix** (Lote 10.5bis): amend `error_taxonomy.md §3.9` to add all 7 multipart codes (BLOB_TOO_LARGE, BACKEND_UNAVAILABLE, CONCURRENCY_LIMITED, ALGO_UNSUPPORTED, SIG_INVALID, CHUNK_MISSING, MANIFEST_NOT_FOUND) with HTTP code, retryable, SDK exception class, customer message, and next_action — same row schema as the existing 3. Verify `COR_AUTH_SCOPE_INSUFFICIENT` exists in §3.6 (auth domain); if not, add. Add a CI gate `validate_error_codes.py` that greps WIs for `COR_*` and asserts every code is in the taxonomy (this is the same gate that should have caught WI-S04-001's 8 undefined codes; the lesson was not extended to a CI guard).

This is the **single highest-incidence prior-WI-consistency defect across both S-04 part 1 and S-05 part 1**; the WI authors are aware (WI-S04-001 audit flagged it) but the fix has been to fix the WI text, not to add the CI gate. Add the gate now.

### Cross-WI — ADR canonical path mismatch (`02_governance/decisions/` vs actual `03_architecture/adrs/`)

**Severity**: P0 (file-system reality). **WIs**: WI-S05-001 §13 (ADR-0038 path), WI-S05-002 §6.1.10 + §13 (ADR-0022 + ADR-0039), WI-S05-003 §9.6 (ADR-0022 reference).

All three WIs cite `specs/02_governance/decisions/ADR-XXXX-...md`. The actual on-disk ADR directory is `specs/03_architecture/adrs/` (verified: ADR-0012 .. ADR-0020 live there; `specs/02_governance/` does not exist; `specs/_governance/decisions/` does not exist either; the directory `specs/_governance/` only contains `reviewer_staffing_strategy.md`).

WI-S05-002 §6.1.10 says literally:
> Update `specs/02_governance/decisions/ADR-0022-chunk-size-vs-part-size-decoupling.md` from DRAFT to ACCEPTED.

That file does not exist. The "ratificação plan" promotes a file at a path the validator and CI will not find.

**Fix** (Lote 10.5bis): pick canonical path globally — recommended `specs/03_architecture/adrs/` (existing convention with ADR-0012..0020). Migrate the WI text in S-04 (already shipped) and S-05 to match. Add `validate_adr_paths.py` CI gate. **This is not strictly an S-05 defect** (it pre-exists in S-04) but S-05 carries it forward unfixed — the ADR ratificação plan in WI-S05-002 cannot succeed against the wrong path, so it must be fixed before WI-S05-002 SEALs.

### WI-S05-001 — SpliceBlob streaming-verify boundary is contradictory across §1, §6.1.4, §6.2, and §15 chaos #6

**Severity**: P0 (load-bearing security control; INV-MULTIPART-DUAL-SIDE-VERIFY + INV-MULTIPART-STREAMING-VERIFY-FAIL-FAST). **WI**: WI-S05-001 §1, §6.1.4, §6.2, §15 chaos experiments.

The spec describes the verify boundary in three incompatible ways:

1. §1 SpliceBlob flow step [5]: "FOR EACH chunk_digest in order: a. r2::stream_get(chunk-<region>/<tenant_prefix>/<chunk_digest>) → stream chunks back-to-back" — **no per-chunk verify mentioned in the canonical sequence**.

2. §6.1.4 step [5] "streaming reassembly": "Per-chunk hash verify in stream (BLAKE3 incremental) — fail-fast if mismatch. Forward bytes to client sink via gRPC `Stream<ByteStream>` OR REST chunked transfer encoding." — **per-chunk verify mandatory mid-stream**.

3. §6.2 out-of-scope: "Streaming progressive verify (verify chunk hash mid-stream): **partial in this WI; full em WI-S05-005**." — **partial verify only**.

4. §15 chaos #6: "Chunk missing mid-stream (S-06 GC race): chunk C tombstoned mid-Splice; handler fail-fast at chunk N; partial response with error trailer (gRPC)." — **fail-fast assumed**.

5. §12 invariants: "INV-MULTIPART-MANIFEST-SIGNED" listed but not "INV-MULTIPART-STREAMING-VERIFY-FAIL-FAST" — yet the registry §3.16 lists it as a HIGH invariant.

The contradiction matters: SpliceBlob's threat model (per §2 narrative point 2) is "if R2 envelope was tampered post-persist (insider scenario from WI-S04-003 dual-side analysis), client receives compromised reassembled blob." The mitigation (§2.2) says "step [5] streaming verify each chunk's hash matches manifest_chunks[i].chunk_digest (delegate WI-S05-005 verifier API)." If the verify is only "partial in this WI" (per §6.2), then S-05 GA ships **without** the dual-side verify defense — the customer-facing INV-MULTIPART-DUAL-SIDE-VERIFY claim is false until WI-S05-005 ships, and WI-S05-005 is in part 2 of the same sprint.

**Fix** (Lote 10.5bis): pick a single position. Recommended:
- (a) WI-S05-001 owns the **invocation surface** (calls verifier between R2 GET and client sink write per chunk).
- (b) WI-S05-005 owns the **verifier implementation** (`StreamingManifestVerifier::verify_chunk(chunk_index, expected_digest, bytes) -> Result<()>`).
- (c) WI-S05-001 §6.1.4 explicitly states the dependency: "step [5] uses `corelink-manifest::StreamingManifestVerifier` from WI-S05-005; this WI implements the call site, WI-S05-005 implements the verifier; both must SEAL together for INV-MULTIPART-STREAMING-VERIFY-FAIL-FAST."
- (d) §6.2 deletes "partial in this WI; full em WI-S05-005" and replaces with "**Verifier impl** lives in WI-S05-005; **invocation site** is in this WI's step [5]."
- (e) §12 adds INV-MULTIPART-STREAMING-VERIFY-FAIL-FAST to the validated list.

Document WI-S05-001 ↔ WI-S05-005 SEAL coupling explicitly.

### WI-S05-001 — manifest sig delegation API does not exist in WI-S04-004 SignatureVerifier

**Severity**: P0 (API design; cripto domain separation). **WI**: WI-S05-001 §1 step [8], §6.1.3 step [8], §6.1.4 step [4], §9.5.

§1 step [8]: "manifest::sign(envelope) → HKDF (delegate WI-S04-004 sig pattern)."
§6.1.3 step [8]: "**manifest::sign** (delegate WI-S04-004 sig pattern): HKDF info=`b"manifest-sig"` (different from `b"ac-sig"`)."
§9.5: "**Why manifest sig HKDF info=`b"manifest-sig"` (different from `b"ac-sig"`)**: domain separation; prevents sig of manifest from being valid as AC envelope sig (cross-domain replay). Lesson learned from WI-S04-004 ADR-0021."

The domain separation rationale is **correct** — exactly the right cripto pattern, and it directly addresses one of the P0 findings from the S-04 part 2 audit (the salt-vs-info-vs-key-id binding question).

But the API delegation is unspecified. WI-S04-004 §1 (verified in the part 2 audit) declares:
```rust
pub trait SignatureVerifier {
    fn verify_sig(canonical_bytes: &[u8], sig: &[u8], sig_key_id: u32) -> Result<(), SigError>;
}
```
The HKDF `info` parameter is hard-coded to `b"ac-sig"` inside the WI-S04-004 implementation (per §1 / §9.3 of that WI). There is **no parameterization for `info`**. So WI-S05-001's claim of "delegate WI-S04-004 sig pattern with info=`b"manifest-sig"`" requires either:

1. **API change in WI-S04-004**: parameterize `verify_sig`/`sign` over `info: &'static [u8]` or a `domain: SigDomain` enum; **but WI-S04-004 is upstream and possibly already SEALED** (sprint contract puts it in S-04). Changing a SEALED WI requires ADR-level governance.

2. **New trait/wrapper in WI-S05-001 (or a new shared crate)**: a `ManifestSignatureVerifier` that internally calls `corelink-sig-core` primitives with `info=b"manifest-sig"`. This is not documented in the WI's artifact list (§13 mentions "manifest::sign" but no separate crate).

3. **Constructor variant**: `SignatureVerifier::for_manifest(...)` or `SignatureVerifier::new_with_info(info)`. Not specified.

WI-S05-001 §13 artifact table does **not** list a manifest-sig crate or module — it lists "Split/Splice handler" (split_splice.rs) but no `corelink-manifest-sig` or equivalent. The "delegate WI-S04-004 sig pattern" claim has no concrete API surface.

**Fix** (Lote 10.5bis):
- (a) Add `corelink-manifest-sig` crate as an artifact (or document that WI-S05-005 ships the manifest-sig wrapper).
- (b) Specify the wrapper signature explicitly: `pub fn sign_manifest(envelope: &ManifestEnvelope, tdk: &TdkHandle) -> ManifestSig` and `pub fn verify_manifest_sig(...)`, both internally invoking the `corelink-sig-core` HKDF primitives with `info=b"manifest-sig"`.
- (c) Add a CI byte-equal test asserting the constant `b"manifest-sig"` byte-string is exactly what the test vectors expect (mirror WI-S04-004 §6.1.X CI byte-equal test on `b"ac-sig"`).
- (d) Add an integration test asserting cross-domain replay rejection: an `AcEnvelope` sig (`info=b"ac-sig"`) presented as a `ManifestEnvelope` sig fails verification (and vice-versa).
- (e) Update §18 dependencies to add the new artifact / WI-S05-005 SEAL coupling.

This is the single **highest cripto rigor** P0 in S-05 part 1 — domain separation is the reason the design is correct, but the API surface that implements it is missing.

### WI-S05-002 — Iterator return type is `Box<dyn Iterator>` but spec claims "zero-allocation Iterator"

**Severity**: P0 (correctness). **WI**: WI-S05-002 §1, §6.1.4, §3 SLA addendum, §6.1.5 streaming Iterator zero-allocation, §8 Gherkin "zero-allocation", §22 cost analysis, §32 anti-patterns.

§1:
```rust
pub trait Chunker {
    fn feed<'a>(&'a mut self, bytes: &'a [u8]) -> Box<dyn Iterator<Item = Chunk<'a>> + 'a>;
    fn finalize<'a>(&'a mut self) -> Option<Chunk<'a>>;
    fn reset(&mut self);
}
```

The return type is `Box<dyn Iterator<...>>`. **A `Box::new(...)` is a heap allocation per `feed()` call.** Many places in the spec claim "zero-allocation":

- §1 doc-comment: "Returns Iterator that yields chunks; Iterator is lazy (zero-allocation)."
- §3 SLA addendum: "Streaming memory bound ≤ 4 MiB stack per invocation."
- §6.1.4 Streaming Iterator API: "Zero-allocation: Chunk borrows internal buffer; consumer copies bytes if storing past next feed."
- §6.1.5 design constraint: "**Streaming Iterator zero-allocation**: per-chunk `&[u8]` slice; no Vec allocation per chunk; backpressure native."
- §8 Gherkin "Streaming Iterator zero-allocation".
- §32 anti-pattern: "❌ Buffered chunker (zero-allocation Iterator)."

The contradiction is internal. **Either** the trait is `fn feed<'a>(&'a mut self, bytes: &'a [u8]) -> impl Iterator<Item = Chunk<'a>> + 'a` (return-position `impl Trait` — zero-allocation, but **not object-safe**, so the trait can't be used as `dyn Chunker`), **or** the `Box<dyn Iterator>` allocates exactly one box per `feed()` call (so "zero-allocation per chunk" is correct but "zero-allocation per call" is false).

The performance claim is largely fine — one 24-byte trait object box per `feed()` is negligible — but the spec language is wrong, and a Crypto SME or perf-conscious reviewer will flag it. Also: the chunker is invoked by WI-S05-001 inside a streaming pipeline that calls `feed()` multiple times as R2 GetObject yields buffers; if R2 stream chunks are 64 KiB and the blob is 1 GiB, that's 16384 `feed()` calls × 1 Box = 16384 heap allocations per blob. Not catastrophic but not "zero-allocation."

**Fix** (Lote 10.5bis):
- (a) Either change the trait to use generic associated types (GAT-stable since Rust 1.65: `type Iter<'a>: Iterator<Item = Chunk<'a>>;`) which is zero-allocation and object-safe-via-erasure, **OR**
- (b) Keep `Box<dyn Iterator>` and update all "zero-allocation" claims to "zero-allocation per chunk; one trait-object box per `feed()` call (~24 bytes)."
- (c) Recommended: GAT version — Rust 1.75+ supports `impl Trait` in trait return position natively without GAT boilerplate; spec should target Rust 1.75+ as the toolchain (verify in `rust-toolchain.toml` planning).
- (d) Spec also claims "backpressure native" — a `Box<dyn Iterator>` does not provide backpressure; backpressure comes from the consumer pulling the iterator. State this correctly: "backpressure: chunker drains as iterator is pulled; downstream consumer (R2 PUT) controls pace."

### WI-S05-003 — multipart_sessions UNIQUE constraint claim is inconsistent with WI-S05-004 schema and the registry

**Severity**: P0 (cross-WI consistency; INV-MULTIPART-IDEMPOTENT correctness). **WI**: WI-S05-003 §1 invariant 2.

§1 invariant 2: "**Idempotent completion**: re-issuing CompleteMultipartUpload with same upload_id + same parts list = no-op (R2 native idempotency); `UNIQUE (tenant_id, blob_digest)` em D1 multipart_sessions table prevents double-record."

But:
1. `multipart_sessions` is the table for **in-flight** sessions (per `invariant_registry.md §3.16` row INV-MULTIPART-IDEMPOTENT: "UNIQUE `(tenant_id, blob_digest, state='in_progress')` em multipart_sessions"). The natural primary key for in-flight is `(upload_id)` from R2; the **partial unique** that prevents concurrent in-flight is `(tenant_id, blob_digest) WHERE state='in_progress'`.
2. SQLite/D1 supports partial unique indexes via `CREATE UNIQUE INDEX ... ON multipart_sessions(tenant_id, blob_digest) WHERE state='in_progress'`. **Plain `UNIQUE (tenant_id, blob_digest)`** as claimed in WI-003 §1 would block the same `(tenant, blob)` from ever being multipart-uploaded a second time, even after a successful completion or abort 7 days ago — this is wrong semantically.
3. WI-S05-004 (D1 schema) is the authoritative source; WI-003 §1 makes a claim about another WI's schema without citing it. The two could be written inconsistently and only caught at deploy time.

**Fix** (Lote 10.5bis):
- (a) WI-003 §1 invariant 2: replace "`UNIQUE (tenant_id, blob_digest)` em D1 multipart_sessions table" with "**partial UNIQUE** `(tenant_id, blob_digest) WHERE state='in_progress'` em D1 multipart_sessions table per WI-S05-004 schema (matches INV-MULTIPART-IDEMPOTENT in registry §3.16)."
- (b) Add a §18 dependency on WI-S05-004 with a note: "WI-S05-004 schema must define `multipart_sessions` with the partial UNIQUE index per §3.16 row 1; WI-S05-003 adapter relies on the partial UNIQUE for idempotent completion semantics."
- (c) Add an integration test: same `(tenant, blob)` initiate twice in-flight → second initiate rejected; one in-flight + one completed → second initiate allowed (asserts partial-unique semantics).

### WI-S05-002 — determinism property test claim is statistically vacuous

**Severity**: P0 (test-design rigor; affects EVT-002 evidence). **WI**: WI-S05-002 §1 INV-CAS-IDEMPOTENCY claim, §6.1.5, §8 Gherkin "Determinism — Fixed2MiB", §10.s05.002.6.

§1: "INV-CAS-IDEMPOTENCY: same input bytes + same `ChunkerConfig` → same chunk boundaries + same digests byte-identical. Property test 1000 random blobs × 100 chunkings = 100% byte-equal."

§8 Gherkin "Determinism — Fixed2MiB":
```
Given 1000 random blobs × 100 chunkings each (Fixed2MiB)
When property test runs
Then 100 chunkings per blob produce IDENTICAL chunks (digest + bytes)
```

This is **statistically vacuous as a determinism gate**. The chunker is a pure function written in safe Rust with `#![forbid(unsafe_code)]` — running it 100 times with the same input on the same binary is **trivially identical by construction** (no I/O, no time, no PRNG, no thread interleavings). The test will always pass; it doesn't exercise the failure mode the spec wants to detect.

The actual determinism risks the spec lists are:
1. **HashMap iteration order** (§2 narrative point 1) — caught by Rust compile-time if the impl uses `std::collections::HashMap` with the random hasher; but the test as written wouldn't catch it because all 100 runs share the same isolate and the same `RandomState`.
2. **FastCDC mask seeds drift between versions** (§2 narrative point 2) — requires a **cross-version** test (current-build vs golden vector from a prior version), not 100 runs of the same build.
3. **Memory layout dependency** — would only manifest with `-C codegen-units=N` randomization or different rustc versions; same build, same runs, same output.

**Fix** (Lote 10.5bis):
- (a) Reframe as **two distinct tests**:
  - **Property test (proptest)**: `prop_chunker_pure(blob)` → `chunker(blob) == chunker(blob)` over 10k randomly-generated blobs. This catches **non-determinism within a single binary** (which would be a memory-safety bug, very rare in safe Rust but checks the compile).
  - **Test vectors regression test**: 50 known blobs + golden chunk-digests committed to repo; CI asserts current build matches golden bytes byte-for-byte. This catches **cross-version drift** (mask seeds change, library upgrade introduces non-determinism, refactor accidentally changes boundary detection). This is what §6.1.9 (test vectors Annex) already plans — but the §1/§8 framing should explicitly point to this as the determinism gate, not the 100-runs vacuity.
- (b) Add a third test: **cross-platform regression**: CI matrix runs on `linux/x86_64`, `linux/aarch64`, `macos/aarch64` (CF Workers does not vary at runtime, but developer machines do); test vectors must match byte-for-byte on all three.
- (c) Drop "× 100 chunkings" from the Gherkin and §10 criteria; replace with "× 1 chunking, asserted against golden vectors".
- (d) Update §15 chaos experiment #1 to use the cross-version harness, not the 100-runs harness.

This finding does not affect the API design — the chunker is fine — but the spec's evidence chain for INV-CAS-IDEMPOTENCY is weak.

### WI-S05-002 — BLAKE3 throughput claim "≥ 2 GB/s single core" is a desktop-x86 ceiling, not a CF Workers realistic target

**Severity**: P0 (perf gate calibration; affects acceptance criteria). **WI**: WI-S05-002 §1.5, §3 SLA addendum, §6.1.7 criterion, §8 Gherkin "BLAKE3 throughput benchmark", §10.s05.002.3, §14.s05.002.4.

§1.5: "BLAKE3 SIMD: `blake3::Hasher` SIMD-optimized; throughput ≥ 2 GB/s single core (criterion benchmark gate)."
§3 SLA addendum: "Chunker throughput ≥ 2 GB/s single core (BLAKE3 SIMD; AVX2/AVX-512 dispatch)."
§8 Gherkin: "When 1 GiB random data chunked Then ≥ 2 GB/s single core (BLAKE3 SIMD AVX2/AVX-512 dispatch)."
§10.s05.002.3: "Criterion benchmarks: Fixed throughput ≥ 2 GB/s single core."

Issues:
1. The 2 GB/s figure is the **BLAKE3 reference benchmark on x86-64 with AVX-512** (BLAKE3 paper Table 3, 2020). On AVX2 only (most CF Workers fleet hardware as of 2026 — Cloudflare's worker fleet is heterogeneous, mostly Intel Skylake-X / AMD EPYC Milan, with mixed AVX-512 availability), BLAKE3 reaches ~1-1.5 GB/s. On ARM NEON (CF Workers nodes with Graviton2/3) it's ~700-900 MB/s.
2. **CF Workers does not run native code.** The Worker runtime is V8 + WASM. Rust→WASM compilation of `blake3` loses SIMD on most Workers configurations (WASM SIMD128 is supported, but SIMD-via-WASM is ~30-50% of native AVX2 throughput; the `blake3` crate has a WASM SIMD128 path but it's not the same as the native AVX-512 SIMD path). **Realistic CF Workers throughput is ~300-600 MB/s for BLAKE3.**
3. Sprint contract goal is **"throughput ≥ 100 MB/s"** end-to-end (sprint.md §14). The chunker is **not** the bottleneck at 100 MB/s; R2 PUT serialization is. Setting the chunker bench gate at 2 GB/s is overkill **and** unachievable on the deploy target.

**Fix** (Lote 10.5bis):
- (a) Reframe the bench gate as **"chunker throughput ≥ 500 MB/s on developer hardware (x86-64 native, AVX2 baseline)"** — sufficient headroom over the 100 MB/s sprint goal, achievable on commodity hardware, anchored to the actual perf budget.
- (b) Add a separate **"chunker throughput ≥ 200 MB/s on CF Workers (Wrangler dev simulation)"** as the deploy-target gate.
- (c) Rationale section: "We do not target the BLAKE3 paper's 2 GB/s ceiling because (i) CF Workers WASM runtime caps SIMD at ~50% of native, (ii) the multipart pipeline bottleneck is R2 PUT (~50-100 MB/s sustained), (iii) the sprint contract requires only 100 MB/s end-to-end. Bench gate at 500 MB/s native + 200 MB/s WASM gives 5×+ headroom."
- (d) Update §14.s05.002.4 latency claim and §22 cost analysis (the "$900/yr" claim implicitly assumes ~5ms per 2 MiB chunk, which is 400 MB/s — correct for WASM but contradicts the 2 GB/s claim a few sections earlier).

### WI-S05-001/003 — bounded concurrency rationale is shallow and the defaults compose wrong

**Severity**: P0 (operational correctness). **WIs**: WI-S05-001 §6.1.5 (semaphore=4 SplitBlob), §9.4; WI-S05-003 §6.1.4 (semaphore=8 UploadPart), §9.3.

WI-S05-001 §9.4: "Avoid R2 PUT storm; per-tenant fairness; tunable per-tier S-13 forward (free=2, business=8, enterprise=16)."
WI-S05-003 §9.3: "Per-tenant semaphore 8 default (tunable per-tier)."

Problems:
1. **Composition undocumented**: SplitBlob calls multipart adapter for chunks UPSERT and `r2::put(chunk-...)`. Each SplitBlob spawns N PUT operations (5 chunks for 10 MiB blob, up to 80k chunks for 160 GiB blob). If 4 SplitBlob calls run concurrently and each spawns 8 parallel UploadParts, that's 32 R2 PUTs concurrent per tenant. Is this the intended cap or accidental?
2. **R2 rate-limit reasoning missing**: R2 lists 1000 PUT/s per bucket per region as the soft rate limit. With 5 regions and per-tenant 32 PUTs, a tenant can sustain 32 × 5 = 160 PUTs/s comfortably, but a flood of SplitBlobs (160 GiB blobs × 4 concurrent × 80k chunks each) saturates instantly. The semaphore caps prevent the **per-tenant unboundedness** but don't connect to the **R2 quota math**.
3. **WI-001 says 4/8/16 (free/business/enterprise) but this isn't in the WI-003 sphere** — different WIs have different defaults for semaphores that compose. A "business" tenant on WI-001 (8 SplitBlob concurrent) feeds 8 × 8 = 64 UploadParts to WI-003's semaphore (which caps at 8). So 7 of 8 SplitBlob calls block on the UploadPart semaphore — this is the **actual** rate-limit, not the SplitBlob semaphore. The WI-001 "8/business" default is decorative.
4. **Sprint contract §14 throughput goal is 100 MB/s end-to-end** — at 100 MB/s with 16 MiB R2 parts = 6.25 PUTs/s per tenant. A semaphore of 8 is 30× over-provisioned. Either the goal is wrong or the semaphore is wrong.

**Fix** (Lote 10.5bis):
- (a) Add a **§6.1.X "Concurrency budget composition"** subsection to WI-S05-001 explaining: "WI-S05-001 SplitBlob semaphore (default 4) × WI-S05-003 UploadPart semaphore (default 8) = 32 R2 PUTs/sec/tenant cap. Per the sprint contract §14 throughput target of 100 MB/s and 16 MiB part size, sustained throughput requires 6 PUTs/sec; 32 cap is 5× headroom for burst."
- (b) Document the cap math in WI-S05-003 §9.3 as well: "Default 8 chosen to allow 8 × 16 MiB = 128 MiB/s burst per concurrent upload session, 5× headroom over 100 MB/s sprint goal."
- (c) Either align WI-001 defaults (4/8/16) with WI-003 (8/16/32) per tier, or document why they differ (e.g., "SplitBlob semaphore caps the **count** of concurrent multipart sessions; UploadPart semaphore caps the **part-PUT rate** within a session; they are orthogonal").
- (d) Add a chaos test #N+1 for the composition: 100 SplitBlob × 100 parts each → assert R2 PUT rate stays ≤ tenant PUT budget.

### WI-S05-001 — REAPI v2.3+ "SplitBlob/SpliceBlob" status is overstated

**Severity**: P0 → P1 (depends on definition of "REAPI v2.3+"). **WI**: WI-S05-001 §1 §2 §6.1 §6.1.2 §10.s05.001.3 §11.

The WI consistently asserts "REAPI v2.3+ SplitBlob + SpliceBlob" as if these are standardized methods. Verified `remote_cache_product_profile.md §1.2` carries the same assumption ("REAPI v2.3+ tem SplitBlob (client solicita decomposição) e SpliceBlob (server reassemble pra download cliente legacy)"). That assumption pre-dates S-05.

External reality check (as of 2026): the bazelbuild/remote-apis repo has **proposed but not standardized** `SplitBlob`/`SpliceBlob` extensions; they appeared in the proto in 2023 as experimental and the v2.3 tag history is intermittent. Bazel 7's adoption of these methods is also uneven (Bazel 7.0 ships the proto definitions but server-side implementation is rare; BuildBuddy and Buildbarn implement them with their own dialect; bazel-remote does not).

WI-S05-001 §10.s05.001.3 says "REAPI v2.3+ conformance suite (bazelbuild/remote-apis) — SplitBlob/SpliceBlob subset 100% green nightly". This implicitly assumes the conformance suite **has** a SplitBlob/SpliceBlob test set in the upstream repo. Last verified state: there are tests for `SplitBlob` in `tests/integration/grpc_test.rs` of bazel-remote-apis but they are not part of the canonical conformance suite — they are vendor-specific.

**Fix** (Lote 10.5bis):
- (a) Drop "REAPI v2.3+" framing and replace with "REAPI v2.X (SplitBlob/SpliceBlob extensions; status: shipped in proto, partial server adoption; CoreLink reference impl)".
- (b) §10.s05.001.3 conformance: clarify "we vendor the SplitBlob/SpliceBlob proto from bazelbuild/remote-apis pinned commit and run our own conformance harness; if upstream adds a canonical conformance suite, we converge."
- (c) Add ADR-0038 (the WI's planned ADR) explicitly to document "SplitBlob/SpliceBlob: REAPI experimental → CoreLink GA reference; Bazel 7+ client compat documented; non-Bazel client SDK ships in S-15 with proto schema pinned."

This is a **customer-facing readiness** issue: a Bazel community reviewer or REAPI WG member will read the WI and immediately fact-check the v2.3+ claim. Better to be upfront about the experimental status than to overclaim.

### WI-S05-003 — chaos suite is 5 scenarios; SOTA bar is ≥ 10

**Severity**: P0 (SOTA discipline; sprint contract §10 mandates ≥ 10 chaos for HIGH_RISK). **WI**: WI-S05-003 §15.

§15 lists 5 chaos experiments. Sprint contract §10 mandates ≥ 10 for HIGH_RISK lane. WI-S05-001 has 12; WI-S05-002 has 11; WI-S05-003 has 5.

The §9-32 compact form in WI-S05-003 is permissible but cannot compress chaos coverage below the sprint-contract floor.

**Fix** (Lote 10.5bis): expand to ≥10. Suggested additions:
6. **R2 quota exceeded mid-Complete**: simulate R2 returns 429 quota; adapter returns 503 + sweeper aborts pending.
7. **Region fail-over mid-multipart**: R2 us-east-1 fails; adapter routes to us-west-2; ETag tracking continues.
8. **upload_id TTL expiry**: 7d session expires; UploadPart returns 404; handler routes to fresh Initiate.
9. **D1 multipart_sessions UNIQUE collision**: race two Initiates same `(tenant, blob)`; one wins, one rejected; integration test asserts.
10. **Wrangler binding rotated mid-deploy**: deploy guard catches binding name change; CI red.
11. **R2 ListMultipartUploads pagination edge**: 1000+ orphans; pagination correctness asserted.

### WI-S05-003 — "Crypto SME (advisory)" sign-off is inconsistent with cross-tenant upload_id binding being CRITICAL

**Severity**: P0 (sign-off compliance). **WI**: WI-S05-003 §30 sign-off table row 13.

Sprint contract Lote 10.4bis lesson: "Crypto SME mandatory non-waivable" for security-domain WIs. WI-S05-003 §30 row 13: "Crypto SME (advisory) | _ETag tracking pattern review_".

The WI itself classifies INV-MULTIPART-PATH-TENANT-SCOPED as CRITICAL (§12) and the cross-tenant upload_id binding is a load-bearing security control (§1 invariant per WI; §6 chaos; §28 R-006 with CRITICAL impact). When the WI's own threat model frames this as cripto-adjacent, the Crypto SME should not be advisory.

**Fix** (Lote 10.5bis): change Crypto SME row 13 to "**mandatory** — cross-tenant upload_id binding + ETag content-hash review (S3-compatible MD5/SHA hex; collision analysis)". The R2 ETag is content-derived; its collision properties (MD5 multipart ETag is not cripto-grade per S3 spec) deserve a Crypto SME pass even if the conclusion is "infrastructure-level integrity is acceptable here."

---

## P1 Findings

### WI-S05-001 — `SignatureVerifier` API surface is exposed, allowing the misuse pattern that WI-S04-003 chaos #1 was designed to prevent

**Severity**: P1. **WI**: WI-S05-001 §6.1.4 step [4] "manifest::verify_signature".

Cross-reference WI-S04-003 §15 chaos #1 (per S-04 audit): "verify_sig and verify_structure as independent traits — exactly the API misuse the chaos test #1 of WI-S04-003 was meant to prevent." If WI-S05-001's SpliceBlob uses an analogous independent `verify_signature` without forcing `verify_structure` first, an attacker with envelope-tamper capability flips bytes in the manifest's chunk list (without changing merkle_root) and the sig still verifies — the lie is signed.

**Fix**: WI-S05-001 §6.1.4 should cite the **same** API-misuse mitigation pattern as WI-S04-003. Specifically: "manifest::verify_full(envelope) calls verify_structure() THEN verify_signature(); the public API is `verify_full` only; `verify_signature` is `pub(crate)` and not exposed to handlers."

### WI-S05-002 — `feed()` Iterator returns `Box<dyn Iterator<Item = Chunk<'a>> + 'a>` is not object-safe-friendly with the `'a` lifetime

**Severity**: P1 (Rust language). **WI**: WI-S05-002 §1, §23.

`Box<dyn Iterator<Item = Chunk<'a>> + 'a>` requires the dyn-Iterator object to outlive `'a`; the lifetime is captured in the trait object. This works in current Rust (1.78+) but has been a friction point with the borrow checker around drop-order for years. A simpler signature for object-safety + zero-allocation is:

```rust
pub trait Chunker {
    fn next_chunk<'a>(&'a mut self, bytes: &'a [u8]) -> Option<Chunk<'a>>;
    fn finalize<'a>(&'a mut self) -> Option<Chunk<'a>>;
    fn reset(&mut self);
}
```

i.e. caller drives the iteration; chunker is a streaming state machine. This is the canonical streaming-parser pattern in Rust (cf. `nom`, `combine`, `serde-json`'s deserialize_streaming).

**Fix**: P1 (not P0 because the current shape works) — consider replacing with `next_chunk` style; document the choice in §9 design decisions.

### WI-S05-001 — outbox vs in-band audit emission mid-Splice is inconsistent

**Severity**: P1. **WI**: WI-S05-001 §1 step [12] (Split) vs §6.1.4 step [6] (Splice), §15 chaos #12.

Split emits `cas.split.ok` "post-handler" (single emit). Splice has two-phase: §6.1.4 step [6] says "audit emit: `cas.splice.ok` post-stream-complete; `cas.splice.error` if mid-stream fail." §15 chaos #12 ("Audit emission gap mid-stream") says "outbox already emitted `cas.splice.start`; reconcile catches missing `cas.splice.ok`." But §1 sequence does **not** mention `cas.splice.start`; only `cas.splice.ok`. So the chaos test references an audit event that the sequence flow doesn't emit.

**Fix**: align §1 to emit `cas.splice.start` at step [4] (post-sig-verify, pre-stream); update §6.1.4 step [6] accordingly; add the metric `corelink.multipart.splice.in_flight_total{tenant_id}` to track the gap window.

### WI-S05-002 — `MAX_BLOB_SIZE = 160 GiB` and `MAX_CHUNKS_PER_BLOB = 80000` derivation is correct but unannotated

**Severity**: P1. **WI**: WI-S05-002 §6.1, §9.6, §9.7.

§9.6: "R2 multipart hard limit: 10000 parts × 16 MiB part = 160 GiB single multipart session."
§9.7: "160 GiB / 2 MiB chunk = 80000 chunks."

Math is correct. But: when ADR-0022 documents customer-tunable chunk size per-tenant via S-13 admin plane (§9.10 forward), the 80000 constant becomes wrong (e.g., 1 MiB chunks → 160000; 4 MiB chunks → 40000). The WI should document the constant as a derived quantity, not a magic number.

**Fix**: define constants as `MAX_BLOB_SIZE_BYTES: u64 = 160 * GiB` and `MAX_CHUNKS_PER_BLOB: u32 = (MAX_BLOB_SIZE_BYTES / MIN_CHUNK_SIZE_BYTES)` where `MIN_CHUNK_SIZE_BYTES = 2 * MiB` is the **minimum** allowed chunk size (per default 2 MiB; with S-13 future tunable allowing down to 1 MiB, this becomes 160000); add a sanity check at config validation time.

### WI-S05-003 — `list_orphans()` semantics are R2-API-dependent

**Severity**: P1. **WI**: WI-S05-003 §1, §6.1.5.

R2 `ListMultipartUploads` returns sessions with no Complete/Abort, but the API has pagination (1000-item page limit, S3-compatible) and the `Initiated` timestamp the API returns is **server-time at Initiate**, not last-activity time. The WI says "sessions older than `max_age` (default 7d)" but doesn't specify which timestamp.

**Fix**: clarify §1 doc-comment of `list_orphans`: "Returns sessions where `R2_initiated_at < now - max_age`. Note: R2 ListMultipartUploads paginates at 1000 items; impl must follow continuation token. Sessions with active in-flight UploadPart calls within `max_age` are still returned (R2 does not track last-activity); cross-check against D1 `multipart_sessions.last_activity_at` if present."

### WI-S05-001 — Mann-Whitney rationale "5ms middleware-grade" is justified but the comparison pair is weak

**Severity**: P1. **WI**: WI-S05-001 §6.1.10, §10.s05.001.2.

§6.1.10: "Goal: cliente cannot distinguish 'blob_not_found' vs 'manifest_invalid' via timing."

The two compared paths are:
- `blob_not_found`: scope_check pass → D1 SELECT cas_blobs returns 0 rows → 404 path. Approximate latency: D1 roundtrip ~5-15ms.
- `manifest_invalid`: scope_check pass → D1 SELECT manifest_chunks returns rows → R2 GET manifest envelope (~30-100ms) → sig verify (~1ms) → 422 path.

These two paths have **wildly different work profiles** (one D1 hop vs one D1 hop + one R2 GET + one BLAKE3-keyed-hash). The expected `|Δmedian|` is ~50-100ms, not the spec's 5ms. The Mann-Whitney 3-prong test as written **will fail** in any realistic benchmark — either because the two paths really are timing-distinguishable (which is the truth and the spec's 5ms gate is wrong), or because the test will be tuned by adding artificial delays to the fast path (cargo-cult constant-time).

**Fix**: pick a more honest goal:
- (a) "blob_not_found vs blob_chunked_not_yours" (both are D1-only paths; one returns 404, one returns 404-because-tenant-mismatch; same work profile; actual constant-time gate makes sense).
- (b) Drop Mann-Whitney for the multipart handler entirely — the timing-leak surface here is materially smaller than the AC sig path; document "constant-time not pursued for handler timing; sig path covered in WI-S04-004."

### WI-S05-002 — cargo-fuzz target list is 2; missing fuzz on FastCDC config validation

**Severity**: P1. **WI**: WI-S05-002 §6.1.8.

Two targets (`fuzz_chunker_fixed`, `fuzz_chunker_fastcdc`). Missing: `fuzz_chunker_config` — fuzz `ChunkerConfig::default()` mutations + `FastCDCConfigInvalid` boundary cases. Adversarial input here is `min > avg` pathology, `mask_s == mask_l` collision, `fastcdc_avg = 0` div-by-zero, etc.

**Fix**: add a third target.

### WI-S05-001 — `prop_chunk_refcount_consistent` property test design is ambitious but underspecified

**Severity**: P1. **WI**: WI-S05-001 §6.1.9 property test 4.

"prop_chunk_refcount_consistent: 1000 ops; refcount in `chunks` table matches actual references in `manifest_chunks`."

This is an **eventual-consistency** invariant that requires either:
- (a) Single-threaded sequential ops (refcount and manifest_chunks updated atomically per op).
- (b) Multi-threaded with atomic D1 batch (chunks UPSERT + manifest_chunks INSERT in same D1 batch transaction).

Sprint contract §14.s05 says D1 batch is the unit of atomicity. WI-001 §6.1.3 step [10] says "manifest_chunks INSERT batch (capped 250 rows/batch per Lote 10.4bis D1 100KB limit lesson)". This means a manifest with 80000 chunks needs **320 batches** — atomicity at batch boundary, not at handler boundary. So mid-handler crash leaves chunks UPSERT'd but only 250/80000 manifest_chunks rows inserted. Refcount diverges.

**Fix**: document the multi-batch consistency story in WI-001 §9.X. Options:
- (a) Single transactional D1 batch (250-row limit caps total manifest size at 250 chunks ≈ 500 MiB blob — too small).
- (b) Multi-batch with idempotent retry: if handler crashes mid-batch, re-Split is idempotent (chunks UPSERT ON CONFLICT increments refcount but **only on first success per blob_digest**; need a "batch_id" or staging table to prevent double-increment).
- (c) Reconcile-after-failure via S-06 GC: refcount is eventual; reconcile job catches divergence.

Pick (b) or (c); (a) doesn't scale.

### WI-S05-001/002 — cost regression gate unit is `$0.000020/op` but Worker pricing is per-50ms slot

**Severity**: P1 (cost rigor; same defect class as S-04 part 2 audit P1 #12). **WI**: WI-S05-001 §10.s05.001.9, §22; WI-S05-002 §10.s05.002.8, §22.

CF Workers Bundled pricing: $0.30/M requests + $0.02 per million CPU-ms-bundles (50ms each). The `$0.000033/Split` figure in §22 implicitly mixes request-billing + CPU-ms-billing without showing the assumption.

**Fix**: same as S-04 part 2 fix — add sensitivity analysis: "assumes Workers Bundled plan; per-request fixed $0.30/M; per-CPU-ms variable; cache hit ratio assumed 95%; if KMS cold ≤ 10ms p99 occurs >5% requests, cost rises to $X."

### WI-S05-003 — RB-FM-060 runbook is referenced but not produced in this WI

**Severity**: P1. **WI**: WI-S05-003 §14.s05.003.7.

"Runbook: RB-FM-060 (multipart orphan; consumed by WI-S05-006 sweeper)."

The artifact is owned by WI-S05-006 (per sprint contract). WI-S05-003 references it for sign-off compliance. This is fine, but WI-003 should explicitly document the **interface contract** between the adapter and the runbook (i.e., what the runbook expects from `list_orphans()` output: pagination guarantee, timestamp format, `OrphanedUpload` fields).

**Fix**: add §6.1.X "RB-FM-060 interface contract: `list_orphans` output fields used by runbook: `upload_id`, `object_key`, `initiated_at` (UTC ISO 8601). Runbook dry-run validated against this contract in WI-S05-006 §X."

---

## P2 Findings

1. **WI-S05-001 §22 cost analysis: $30k/yr at 1M Split + 10M Splice/dia is plausible but the BuildBuddy comparison "similar per-op cost; less dedup → 2× R2 storage" is not citable.** Add a footnote: "BuildBuddy multipart pricing baseline: <link or 'estimated from BuildBuddy public pricing as of 2026-04>'."

2. **WI-S05-002 §6.1.10 ADR-0039 is "small registry-extension; ratificada em WI-S05-006 ship gate."** The WI text says "small" but the chunker public API stability is not small — semver discipline + mask seeds versioning policy is a load-bearing customer commitment. ADR-0039 deserves the same density as ADR-0022 (which gets §1.5 prose + 4-row table + future-tunable rationale).

3. **WI-S05-003 §22 TCO 12m: $9.3k/yr** — assume 1M sessions/dia. Sprint contract sets a different baseline (§14.s05 throughput ≥ 100 MB/s; not session count). Reconcile: "1M sessions/dia × 50 MiB avg = 50 PB/yr; at $0.015/GB/mo = $750k/yr R2 storage **before dedup**, ~$500k after 1.5× dedup." The $9.3k/yr is **only the multipart API ops cost**, not R2 storage. Disambiguate.

4. **WI-S05-002 §15 chaos #6 is "Iterator lifetime issue ... borrow checker compile-time enforce; chaos test asserts compile failure."** Compile-failure tests are not chaos tests; they are `compile_fail` doctest entries. Move to §11 quality standards section as a `#[doc(test = "compile_fail")]` example.

5. **WI-S05-001 §20 time-boxing 76h hard limit** is reasonable for a 65h PERT, but "split em 'core handler' + 'conformance integration' sub-WIs" needs an owner. WI-005 already exists for manifest builder/verifier; if WI-001 splits, the conformance harness should land in WI-006 (ship gate), not a new WI. Document.

6. **WI-S05-003 §13 artifact "Provisioning script `scripts/provision_multipart_buckets.sh`"** is a Bash script with an SH file extension. CI policy in this repo (verify) should mandate `set -euo pipefail` + shellcheck. Add the convention to §14 quality standards.

7. **WI-S05-001/002/003 §27 Knowledge Transfer "Tech talk (1.5h)"** are nice-to-have but the workshop attendees include "downstream WI authors (WI-S05-002..006)" — circular dependency: WI-001 KT requires WI-002 author, and WI-002 KT lists "downstream WI authors" too. These talks are S-05 retrospective material, not pre-merge gates.

8. **WI-S05-002 §28 risk register R-007 "Throughput regression > 10%"** has Det = L (low detectability). Criterion benchmarks are a CI gate; detectability should be H (high). Update to H.

9. **WI-S05-001 §23 HTTP error mapping has 9 rows; sprint contract §10 anti-scope §B says "no overflow into anti-scope codes."** Cross-check: COR_AUTH_SCOPE_INSUFFICIENT (403) is reused from auth domain; COR_CAS_BLOB_NOT_FOUND (404) is reused from CAS domain. Both are correct. The 7 new COR_MULTIPART_* are the P0 finding above.

10. **WI-S05-002 §9 has "9.10 ADR potencial?"** but the question mark is awkward formatting; should be "9.10 ADRs to author / ratify" with the answer (ADR-0022 ratificada + ADR-0039 forward).

11. **WI-S05-003 §28 R-008 "R2 backend cascade unavailable"** has Imp = HIGH but Mitigation "circuit breaker S-XX forward" — undefined sprint reference. Replace S-XX with concrete sprint (likely S-08 rate limiting / S-14 multi-region failover; verify with sprint roadmap).

---

## Cross-WI Consistency Check

### Pattern alignment (WI-S05-001 / 002 / 003)

| Pattern | WI-001 | WI-002 | WI-003 | Aligned? |
|---|---|---|---|---|
| TenantCtx-only enforcement | Yes (explicit §6, §7) | N/A (lib) | Yes (§1 invariant + binding) | YES |
| `with_tenant_ctx!` macro absent | Yes (§7 anti-pattern) | N/A | Yes (correctly absent) | YES |
| audit_outbox cited as WI-S01-004 | Yes (§9.8 explicit) | N/A | N/A (§28 doesn't cite) | YES |
| D1 batch 100KB / 250-row cap | Yes (§9.7) | N/A | Yes (Lote 10.4bis lesson §0.1) | YES |
| Wrangler `lifecycle add` (not `set`) | N/A | N/A | Yes (§9.4 explicit) | YES |
| CORS via REST API (not wrangler) | N/A | N/A | Yes (§6.1.5) | YES |
| HKDF sig domain separation `b"manifest-sig"` | Yes (§9.5) | N/A | N/A | YES |
| Crypto SME mandatory for cripto WIs | Yes (mandatory) | Yes (mandatory **emphatic**) | **No (advisory)** — P0 fix | NO |
| INV §3.16 promovida preemptivamente | Yes | Yes | Yes | YES |
| Cost regression gate per-op | Yes (§14) | Yes (§14) | Yes (§10) | YES |
| Mann-Whitney 3-prong | Yes (5ms middleware) | Yes (5ms middleware) | N/A (adapter) | YES |
| Property tests 10k PR + 100k nightly | Yes | Yes | Yes | YES |
| 32 sections numbered | Yes | Yes | Yes (compact §9-32) | YES |
| Chaos ≥ 10 (sprint contract §10) | Yes (12) | Yes (11) | **No (5)** — P0 fix | NO |
| Risk register ≥ 10 rows 6-col | Yes (14) | Yes (12) | Yes (10) | YES |
| 13-row sign-off | Yes | Yes | Yes (compact) | YES |
| Bounded parser MAX_BLOB_SIZE 160 GiB | Yes (§2 #4) | Yes (§6.1) | Implicit (§6.1.4 part_number ≤ 10000) | YES |
| ADR ratificação plan | ADR-0038 (forward) | ADR-0022 (ratify) + ADR-0039 (forward) | None new | YES |
| ADR storage path | `02_governance/decisions/` (WRONG) | same (WRONG) | references only (WRONG) | NO — global P0 |
| Error taxonomy alignment | **8 codes undefined** — P0 | N/A | partially undefined | NO — P0 |
| SpliceBlob streaming verify boundary | Inconsistent (§1, §6.1.4, §6.2 contradict) | N/A | N/A | NO — P0 |
| Bounded concurrency rationale | Default 4 (no R2 quota math) | N/A | Default 8 (no R2 quota math) | NO — P0 |

### Sequence flow consistency

WI-001 §1 SplitBlob steps [0]..[12] cite:
- `r2::stream_get` (R2 SDK; WI-003 owns)
- `chunker::feed` (corelink-chunker; WI-002 owns)
- `chunks::upsert` (D1; WI-004 owns)
- `r2::put` chunk + manifest (R2 SDK; WI-003 owns)
- `manifest::build` + `manifest::sign` (corelink-manifest; WI-005 owns)
- `manifest_chunks::insert_batch` (D1; WI-004 owns)
- `cas_blobs::set_chunked` (D1; WI-004 owns)
- `audit emit cas.split.ok` (outbox; WI-001-004 — Lote 10.4bis lesson absorbed)

This is the correct dependency graph. **But WI-001 §1 step [4] says `r2::stream_get(tenant_prefix, blob_digest)`** — implying the adapter takes `tenant_prefix` directly, while WI-S05-003 §1 `MultipartAdapter::initiate(tenant_id, bucket, object_key)` takes `tenant_id` not `tenant_prefix`. These should reconcile: the adapter takes either (a) `tenant_id` + derives prefix internally (more secure; one place to enforce), or (b) `tenant_prefix` (more performant; trusts caller). Pick (a) — caller passes `TenantId` newtype, adapter derives the path; matches Layer 4 enforcement model.

### Sprint contract alignment

Sprint contract S-05 §3 goals:
- Multipart blobs até 5 TiB → **WI-001 §6 says 160 GiB single multipart, > 160 GiB stitched (WI-006)**. Math check: 5 TiB / 160 GiB = 32 stitched sessions. OK.
- Merkle dual-side → WI-001 §1 + §12 INV-MULTIPART-DUAL-SIDE-VERIFY. **But the verify boundary contradiction (P0 above) means dual-side is partial in WI-001; needs WI-005 SEAL to be customer-facing complete.** Document this dependency.
- Dedup intra-tenant ≥ 1.5× → WI-001 §6.1.13 SLA + Gherkin "Chunk dedup intra-tenant"; WI-002 §6.1.5 SLA addendum.
- Throughput ≥ 100 MB/s → WI-001 §6.1.6 Streaming pipeline; WI-002 §3 throughput targets (but the 2 GB/s claim is the P0 finding above).

All four sprint goals trace to WI-001/002/003 deliverables; the trio is not under-scoped.

### Lote 10.4bis lessons absorption

The S-04 part 1/2 audit findings were largely absorbed:
- TenantCtx-only: YES (explicit in WI-001).
- audit_outbox WI-S01-004 (not 005): YES (cited correctly).
- No `with_tenant_ctx!` on D1: YES (correctly absent and anti-patterned).
- D1 batch 100KB limit: YES (250-row cap).
- CHECK constraints inline (not ALTER ADD): N/A in S-05 part 1 (D1 schema is WI-S05-004).
- BEGIN/COMMIT removed: N/A.
- Wrangler `lifecycle add` + CORS via REST: YES (WI-003 §9.4, §6.1.5).
- tenant_prefix materialized column: referenced in WI-003 §1 (lesson Lote 10.4bis); will be schema'd in WI-S05-004.
- HKDF sig domain separation: YES (`b"manifest-sig"` in WI-001 §9.5).
- Crypto SME mandatory non-waivable: YES in WI-001/002, **NO in WI-003** (P0 fix).
- 4-tier incident classification: implicit (CRITICAL/HIGH/MEDIUM/LOW used throughout).
- validate_inv_promotion.py CI gate: assumed in place (registry §3.16 populated).
- INV §3.16 promovida preemptivamente: YES (12 INV-MULTIPART-* entries).

The **single missed lesson** is the error taxonomy CI gate (`validate_error_codes.py`); the 8 undefined codes in WI-001 prove it's not yet active.

---

## Comparison to Canonical SOTA Bar (WI-S04-003 = 8.6/10)

WI-S04-003 was rated 8.6 best-in-class because of:
1. **Concrete cripto rationale** (BLAKE3 + RFC 6962 domain separation; bounded parser bounds; dual-side defense-in-depth).
2. **Test vectors Annex A/B** for SLSA L3 reviewer reproducibility.
3. **Cargo-fuzz harness 1h CI nightly** with bounded recursion.
4. **Crypto SME mandatory emphatic** with concrete review checklist.
5. **API misuse prevention** (`verify_full` only public; `verify_sig` and `verify_structure` private to crate).
6. **11 chaos experiments** including chunk-tampering scenarios.
7. **Risk register 12 rows 6-col** with explicit residual.
8. **ADR-0037 forward-looking promotion** with quantitative rationale.

### WI-S05-002 vs WI-S04-003 (delta to reach 8.6+)

WI-S05-002 sits at 8.4. To clear WI-S04-003's 8.6:
- **Fix Iterator allocation contradiction** (P0 above) — adds 0.1.
- **Fix BLAKE3 throughput portability** (P0 above) — adds 0.1.
- **Fix determinism property test design** (P0 above) — adds 0.1.
- **Add cargo-fuzz config target** (P1) — adds 0.05.
- **Tighten ADR-0039 to ADR-0022 density** — adds 0.05.

After P0 patches: realistic ceiling 8.7. **WI-S05-002 has the highest 9.0 potential** of the trio.

### WI-S05-001 vs WI-S04-003 (delta)

WI-S05-001 sits at 8.0. To clear 8.6:
- **Fix SpliceBlob streaming-verify boundary contradiction** (P0) — adds 0.2.
- **Fix manifest-sig delegation API gap** (P0) — adds 0.2.
- **Fix error taxonomy** (P0) — adds 0.1.
- **Fix bounded-concurrency composition rationale** (P0) — adds 0.1.
- **Adopt `verify_full`-only API misuse prevention** (P1) — adds 0.05.

After P0 patches: realistic ceiling 8.65. Achievable.

### WI-S05-003 vs WI-S04-003 (delta)

WI-S05-003 sits at 7.9. To clear 8.6:
- **Expand chaos suite to ≥ 10** (P0) — adds 0.2.
- **Fix multipart_sessions UNIQUE claim** (P0) — adds 0.1.
- **Crypto SME mandatory** (P0) — adds 0.1.
- **Add §22 cost reconciliation R2 storage vs API ops** (P2) — adds 0.05.
- **Expand §9-32 compact form to full** for 13-section discipline — adds 0.15.

After P0+P2 patches: realistic ceiling 8.5. Just shy of WI-S04-003 because it is by nature an adapter (less cripto rigor surface).

---

## Verdict per WI + Aggregate Part 1 Score + Lote 10.5bis P0 Fix Plan

### Verdicts

- **WI-S05-001**: **GO-WITH-FIXES (P0)**. Score 8.0/10. P0s: error taxonomy (8 undefined codes), SpliceBlob verify boundary contradiction, manifest-sig delegation API gap, bounded-concurrency composition rationale.
- **WI-S05-002**: **GO-WITH-FIXES-LIGHT (P0)**. Score 8.4/10. P0s: Iterator allocation contradiction, BLAKE3 throughput portability, determinism property test redesign, ADR storage path.
- **WI-S05-003**: **GO-WITH-FIXES (P0)**. Score 7.9/10. P0s: chaos suite expansion to ≥10, multipart_sessions UNIQUE claim correction, Crypto SME mandatory, ADR storage path.

**Aggregate Part 1: 8.10/10** (above S-04 part 1 average 8.05; below WI-S04-003 best-in-class 8.6 ceiling).

None is REJECT. None is SEAL-as-is. All three need a Lote 10.5bis P0 patch pass.

### Lote 10.5bis P0 Fix Plan

**Priority 1 (must-fix; ship-blocker for WI SEAL):**

1. **Add `validate_error_codes.py` CI gate + amend `error_taxonomy.md §3.9`** with 7 new multipart codes (BLOB_TOO_LARGE, BACKEND_UNAVAILABLE, CONCURRENCY_LIMITED, ALGO_UNSUPPORTED, SIG_INVALID, CHUNK_MISSING, MANIFEST_NOT_FOUND). Verify COR_AUTH_SCOPE_INSUFFICIENT in §3.6. Same gate must pass for S-04 retroactively.

2. **Resolve ADR canonical path**: pick `specs/03_architecture/adrs/` (existing) or migrate to `specs/02_governance/decisions/`. Update WI-S05-001/002/003 + sprint contract + S-04 WIs. Add `validate_adr_paths.py` CI gate.

3. **Fix WI-S05-001 SpliceBlob streaming-verify boundary**: pick single position (recommended: invocation in WI-001, impl in WI-005, both SEAL-coupled). Update §1/§6.1.4/§6.2/§12/§15 consistently.

4. **Fix WI-S05-001 manifest-sig delegation API**: add `corelink-manifest-sig` artifact (or document WI-005 ownership), specify `sign_manifest`/`verify_manifest_sig` signature with `info=b"manifest-sig"` hard-coded, add CI byte-equal test, add cross-domain replay rejection integration test.

5. **Fix WI-S05-002 Iterator allocation contradiction**: choose GAT-based zero-allocation API or honest "1 box per `feed()` call" framing; update §1, §6.1.4, §3, §22, §32 consistently.

6. **Fix WI-S05-002 BLAKE3 throughput claim**: replace 2 GB/s with `≥ 500 MB/s native + ≥ 200 MB/s WASM CF Workers`; document the WASM SIMD ceiling; reconcile with $900/yr cost claim.

7. **Fix WI-S05-002 determinism property test design**: split into (a) `prop_chunker_pure` proptest within-binary, (b) test-vectors regression test cross-version, (c) cross-platform CI matrix.

8. **Fix WI-S05-001/003 bounded-concurrency rationale**: add concurrency-budget composition subsection; document SplitBlob × UploadPart → R2 PUTs/sec math; align tier defaults across WIs.

9. **Fix WI-S05-003 multipart_sessions UNIQUE claim**: replace plain UNIQUE with partial UNIQUE `WHERE state='in_progress'`; reference WI-S05-004 schema authority; add integration test.

10. **Fix WI-S05-003 chaos suite**: expand from 5 to ≥10 scenarios.

11. **Fix WI-S05-003 Crypto SME**: change row 13 from advisory to mandatory.

**Priority 2 (P1; this-sprint, before PRR):**

12. WI-S05-001: adopt `verify_full`-only API misuse prevention pattern from WI-S04-003 chaos #1.
13. WI-S05-001: align Splice audit emission (§1 missing `cas.splice.start`).
14. WI-S05-001: Mann-Whitney comparison pair redesign (5ms gate is unrealistic for current pair).
15. WI-S05-001: prop_chunk_refcount_consistent multi-batch consistency story.
16. WI-S05-002: cargo-fuzz config validation target (3 targets total).
17. WI-S05-002: MAX_CHUNKS_PER_BLOB derived-from-MIN_CHUNK_SIZE constant.
18. WI-S05-003: `list_orphans` pagination + R2 timestamp semantics.
19. WI-S05-003: RB-FM-060 interface contract documented.
20. All three: cost regression gate Workers Bundled vs Unbound CPU disambiguation.

**Priority 3 (P2; before SEAL):**

21. WI-S05-001 §22 BuildBuddy comparison citation footnote.
22. WI-S05-002 ADR-0039 density bump.
23. WI-S05-003 §22 R2 storage vs API ops cost reconciliation.
24. WI-S05-002 §15 chaos #6 reclassify as compile_fail doctest.
25. WI-S05-002 §28 R-007 detectability H (not L).
26. WI-S05-003 §28 R-008 circuit breaker sprint reference (S-08 or S-14, not "S-XX").
27. WI-S05-001/002/003 §27 KT talks: deferred to S-05 retro, not pre-merge gate.

### Estimated Lote 10.5bis P0 patch effort

- **WI-S05-001 P0 patch**: ~6-8h (taxonomy amend + verify boundary unification + sig API spec + concurrency rationale).
- **WI-S05-002 P0 patch**: ~4-5h (Iterator contract + BLAKE3 portability + determinism redesign + ADR path).
- **WI-S05-003 P0 patch**: ~3-4h (chaos expansion + UNIQUE clarification + Crypto SME promotion + ADR path).
- **Cross-WI infrastructure**: ~3-4h (validate_error_codes.py + validate_adr_paths.py CI gates + error_taxonomy.md §3.9 amendment).

**Total Lote 10.5bis P0 budget: ~16-21h.** Achievable in a single 2-day patch sprint.

---

## Closing Notes

The S-05 part 1 trio demonstrates the program is **converging toward SOTA** — the Lote 10.4bis lessons-learned channel is working (TenantCtx-only, audit_outbox citation, no `with_tenant_ctx!` on D1, INV §3.16 preemptive promotion are all correctly internalized), and the average has nudged from 7.83 (S-04 part 2) → 8.05 (S-04 part 1) → **8.10 (S-05 part 1)**. WI-S05-002 is the second strongest WI in the program after WI-S04-003 (8.4 vs 8.6) and a credible candidate to **clear 8.7+ post-Lote 10.5bis**. The trio's residual defects are structural (verify boundary, sig API delegation, error taxonomy, ADR path) rather than conceptual — none of the cripto primitives are wrong, none of the threat models are unrealistic, none of the customer-facing claims are ungrounded.

**The user directive "average não serve. SOTA puro 9-10 é o target" is not yet met.** WI-S04-003's 8.6 ceiling stood unbroken by S-05 part 1. To break it, the program needs:
- **A WI with a primary cripto contribution** (S-05 part 2 WI-S05-005 manifest builder/verifier is the strongest candidate; if it lands tighter than WI-S04-003, the ceiling moves).
- **A WI with first-principles novelty** (WI-S05-002 has the right shape — chunker is a reusable cripto primitive — but the Iterator allocation contradiction + BLAKE3 portability hand-wave keep it at 8.4 rather than 9.0).
- **A WI with all P0s pre-resolved at draft time** (WI-S05-002 came closest; the other two carry forward defects from S-04).

The 9-10 SOTA bar requires **zero P0 findings** at draft submission. WI-S05-002's defect count is 4 (P0) + 2 (P1) — closer to the bar than WI-S05-001 (5 P0 + 4 P1) or WI-S05-003 (5 P0 + 2 P1) — but still not zero.

**Recommendation**: Lote 10.5bis P0 patch pass for all three; then SEAL WI-S05-002 first (lightest patch); WI-S05-001 SEAL coupled with WI-S05-005 (verify boundary dependency); WI-S05-003 SEAL after WI-S05-004 schema confirms partial UNIQUE.

---

**End of audit. Generated by Agent R4 (Claude Opus 4.7, 1M context, independent SOTA reviewer round 4) on 2026-04-25.**
