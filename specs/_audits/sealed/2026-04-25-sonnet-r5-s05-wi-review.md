---
id: AUDIT-SONNET-R5-S05-WI-REVIEW
parent_audit: specs/_audits/sealed/2026-04-25-agent-r4-s05-part1-wi-review.md
sprint_contract: specs/04_sprints/_sealed/S05/_spec_contract.md
tags: [audit, sota, lote-10.5, s-05, sonnet-r5, independent]
---

# Sonnet R5 — Lote 10.5 S-05 All WIs (001–006) Independent Adversarial Review

> **Reviewer**: Sonnet 4.6 (independent; round 5; different model from Agent R4 Opus 4.7).
> **Scope**: All 6 WIs of Sprint S-05 (post-Lote-10.5bis fixes applied).
> **Calibration**: User directive "SOTA puro 9-10". Program historical ceiling: WI-S04-003 at 8.6.
> **Method**: Independent read of all 6 WIs + spec contract + invariant registry §3.16 + ADR-0040 +
> both R4 audits. Cross-validation rather than summarization. Finding what Opus missed.

---

## Executive Summary

After Lote 10.5bis the S-05 package has absorbed the majority of Part 1 P0s: the Iterator allocation
contradiction is resolved (pull-based `next_chunk` API), BLAKE3 throughput is recalibrated to
≥ 500 MB/s native / ≥ 200 MB/s WASM, the partial UNIQUE index now uses the correct WHERE clause,
the canonical_bytes 102-byte layout is published, tree shape is pinned to chunk_index ascending,
and the CancellationToken abort mechanism is explicit. ADR-0040 now contains an actual sharding
decision (per-region, 5 shards, 80% trigger, dual-write migration plan). The program has meaningfully
improved.

Yet five classes of defects survive the Lote 10.5bis pass, and three of them are not in R4's
findings. Opus caught the big architectural gaps. Sonnet's independent pass surfaces: (1) a
**manifest signing race** — WI-S05-001 signs the manifest BEFORE chunk verification is complete
on the SplitBlob path, creating a window where an invalid manifest can be signed and persisted;
(2) a **streaming verify ordering inversion** — WI-S05-001 SpliceBlob flow calls verify_full
(structure + sig) in step [4] then calls verify_streaming in step [5], but verify_streaming checks
individual chunk hashes against manifest.chunks — if the manifest's sig was already verified, a
structurally-valid but content-stale manifest (chunks in R2 changed post-signing) still passes sig
verify and only fails streaming verify after bytes have begun flowing to the client; (3) **bounded
parser memory math is wrong** — 81920 chunks × 64 bytes per ChunkRef = 5.24 MB heap for the
`chunks` Vec alone, which is above the 4 MiB stack budget declared in §14.s05.005.9 and
INV-MULTIPART-STREAMING-MEMORY; (4) ADR-0040 is now substantive but introduces a **cross-shard
orphan session blind spot** — the sweeper queries one D1 shard per region, but the sharding
decision defers sub-shard design to "when 80% threshold hit," meaning orphan multipart sessions
may live in a different sub-shard than the one the sweeper queries during a shard-split transition
window; (5) the **determinism property test remains statistically vacuous** after Lote 10.5bis —
the fix removed the "× 100 chunkings" framing but the Gherkin still says "1000 manifests × 100
builds = byte-identical" which is the same semantically vacuous claim (pure-function same-binary
same-input → always identical).

Beyond these, several R4 Part 2 P0s are confirmed still unresolved and one R4 finding is challenged
(the Mann-Whitney |Δmedian| ≤ 1ms for manifest verify — Sonnet disagrees with R4's framing of this
as an insoluble tension; see §Sonnet-vs-Opus differential below).

**Aggregate S-05 post-Lote-10.5bis score: 8.20/10** (up from R4's 8.05/8.10 baselines; below the
9.0 SOTA target; WI-S05-005 remains the highest-quality individual WI at 8.5).

---

## Per-WI Score (post-Lote-10.5bis)

| WI | Score | Cripto | Complete | Clarity | SOTA | Internal | Cross-WI | Verdict |
|---|---|---|---|---|---|---|---|---|
| WI-S05-001 | **8.1** | 7.5 | 8.5 | 8.0 | 8.5 | 7.0 | 8.0 | GO-WITH-FIXES (P0 new) |
| WI-S05-002 | **8.4** | 8.5 | 8.5 | 8.5 | 8.5 | 8.0 | 8.5 | GO-WITH-FIXES-LIGHT |
| WI-S05-003 | **8.0** | 7.5 | 8.0 | 8.5 | 8.0 | 8.0 | 8.0 | GO-WITH-FIXES |
| WI-S05-004 | **8.1** | 8.0 | 8.0 | 8.5 | 7.5 | 7.5 | 8.5 | GO-WITH-FIXES |
| WI-S05-005 | **8.5** | 8.5 | 9.0 | 8.5 | 9.0 | 8.0 | 8.5 | GO-WITH-FIXES (P0 residual) |
| WI-S05-006 | **8.0** | 7.5 | 8.0 | 8.0 | 7.5 | 7.0 | 8.0 | GO-WITH-FIXES (P0 staffing) |

**Aggregate: 8.18/10.**

---

## P0 Findings (Sonnet-new; not in R4 audits)

### P0-SR5-001 — Manifest signing race: SplitBlob signs manifest BEFORE per-chunk R2 write verification completes

**Severity**: P0 — load-bearing cripto; INV-MULTIPART-MANIFEST-SIGNED + INV-MULTIPART-MANIFEST-VALID.
**WI**: WI-S05-001 §1 SplitBlob flow steps [7] → [8] → [9] vs step [6]; §6.1 in-scope step [6].

WI-S05-001 §1 SplitBlob flow:

```
[6] FOR EACH chunk: ... r2::put(chunk-<region>/...)
[7] manifest::build(chunks_in_order) → Merkle root
[8] manifest::sign(canonical_bytes(envelope))
[9] r2::put(manifest-<region>/...)
```

Step [6] performs parallel R2 PUTs (§6.1 "Parallel R2 PUT chunks (bounded concurrency 8 per
blob)"). Step [7] builds the Merkle root from chunk digests. Step [8] signs the manifest. Step [9]
persists the signed manifest.

**The race**: step [7] builds the Merkle root from the in-memory chunk digest list collected during
step [6]. But step [6] is "Parallel R2 PUT chunks (bounded concurrency 8 per blob)" — the parallel
R2 PUTs happen concurrently. The spec says the Merkle root is built from "chunks_in_order" after
step [6], implying step [6] completes fully before step [7] runs. However, the spec never explicitly
states that ALL R2 PUT results are awaited and verified before the manifest is built and signed.

**Why this matters**: if a single R2 PUT fails silently (R2 eventual-consistency response ambiguity,
partial write, or timeout), the chunk digest is still in the in-memory list (computed from local
bytes via BLAKE3 before the PUT), and the manifest is built and signed with that digest — but the
chunk object may not exist in R2. The signed manifest now points to a non-existent chunk. SpliceBlob
will later fail with `ChunkMissing` — but the manifest sig will verify correctly (it was signed
over correct bytes), the structure will be valid, and the streaming verify will fail at step [5].
This is not a security violation in the narrow sense, but it is a **cripto-operational inconsistency**:
the manifest's integrity guarantee is "sig over canonical_bytes which binds merkle_root which binds
chunk digests" but the chunk objects in R2 are not atomically linked to the manifest at signing time.

**Worse: the stitched flow** (> 160 GiB; WI-S05-006 §6.1.3) builds a meta-manifest from multiple
manifest digests. If one manifest was signed with a non-persisted chunk, the meta-manifest sig also
commits to a stale child manifest. The integrity claim "meta-manifest binds all sessions" fails at
the session-level missing chunk.

**Concrete attack scenario**: adversary with R2 write access (insider) intercepts one R2 PUT
mid-flight (drops it). The handler logs 503 or timeout for that PUT and retries. During the retry
window, the handler may proceed to step [7] if the semaphore-bounded parallel loop has sufficient
other completed chunks. Depending on the implementation of the "Parallel R2 PUT chunks (bounded
concurrency 8)" loop, a timeout exception from one PUT may be swallowed if the loop doesn't have a
hard join-all pattern. The spec says "parallel R2 PUT chunks" but does not say "await all PUTs to
completion before building manifest."

**Fix (Lote 10.5ter)**:
- (a) WI-S05-001 §6.1 step [6] must explicitly state: "Await ALL R2 PUT results via `join_all`
  or equivalent; abort SplitBlob with 503 if ANY chunk PUT fails; manifest::build NOT invoked
  until all chunk PUTs return `Ok(CompletedChunk)`."
- (b) Introduce a `ChunkPutReceipt` type (unit struct returned by `r2::put`) that must be produced
  for each chunk before `manifest::build` can accept the chunk. This makes the ordering enforced at
  the type level, not as an implicit flow ordering assumption.
- (c) Add a chaos test: "One R2 PUT returns 503 mid-SplitBlob → handler aborts; no manifest written;
  retry re-initiates full SplitBlob."
- (d) Document in §9 design decisions: "Manifest is signed only after all chunk PUTs return Ok;
  signing a manifest with unverified chunk persistence is prohibited (INV-MULTIPART-MANIFEST-SIGNED
  load-bearing dependency on chunk persistence prior to signing)."

---

### P0-SR5-002 — SpliceBlob streaming verify: bytes forwarded to client BEFORE verify_streaming confirms integrity

**Severity**: P0 — load-bearing security control; INV-MULTIPART-STREAMING-VERIFY-FAIL-FAST;
INV-MULTIPART-DUAL-SIDE-VERIFY.
**WI**: WI-S05-001 §6.1 SpliceBlob step [5]; §1 SpliceBlob flow step [5].

WI-S05-001 §6.1 SpliceBlob (Lote 10.5bis P0 fix applied):

> "Handler creates `tokio_util::sync::CancellationToken`. Handler invokes WI-S05-005
> `ManifestVerifier::verify_streaming(manifest, chunk_stream, cancel_token)` em parallel with R2
> chunk streaming. Verifier per-chunk hash verify in stream (BLAKE3 incremental); on mismatch chunk
> N → cancels token → upstream R2 stream aborted before chunk N+1 read; verifier returns
> `Err(StreamingChunkMismatch { index: N })`. Forward bytes to client sink via gRPC `Stream<ByteStream>`
> OR REST chunked transfer encoding **only after per-chunk verify passes**."

The phrase "only after per-chunk verify passes" is the critical claim. The architecture is:

```
R2 chunk stream → verify_streaming (runs in parallel) → client sink
```

Running "in parallel" with forwarding-only-after-verify is architecturally contradictory. If
`verify_streaming` and forwarding to client are parallel tasks, the forwarding task CANNOT wait for
`verify_streaming` without buffering the chunk. But the spec also claims "zero-allocation" and
"streaming memory bound ≤ 4 MiB."

**Three possible architectural interpretations** — the spec text is ambiguous on which one is
implemented:

1. **Sequential per-chunk**: receive chunk bytes from R2 → verify hash → if OK, forward to client.
   BLAKE3(chunk) then write to sink. Zero additional buffering needed; latency = hash-per-chunk.
   **This is the correct security posture.** But "parallel with R2 chunk streaming" implies this
   is NOT what's intended.

2. **Parallel pipeline with in-flight buffering**: R2 GET feeds a channel; verify_streaming
   consumes from channel; forwarding task reads from a separate channel that verify_streaming
   feeds after approval. Each chunk is buffered (≥ 2 MiB) between R2 GET and forwarding. Memory
   = 2 MiB × pipeline_depth. For 2-stage pipeline this is 4 MiB; correct — within budget. But the
   spec never describes the two-channel architecture; the phrase "in parallel" implies overlapping
   I/O, not sequential verification.

3. **Parallel with cancel-on-error but optimistic forward**: R2 GET and forwarding to client run
   together; verify_streaming signals cancel on mismatch; handler aborts forwarding after cancel.
   **This is the security nightmare**: if chunk N bytes are forwarded to the client before verify
   catches the mismatch at chunk N (because verify runs concurrently with forwarding), the client
   already received N-1 valid chunks plus the beginning of chunk N. The cancel only stops
   SUBSEQUENT chunks, not the already-forwarded bytes. A tampered-chunk-N attack delivers partial
   poisoned content to the client before detection.

The spec text "forward bytes to client sink **only after per-chunk verify passes**" plus "in parallel
with R2 chunk streaming" is the contradiction R4 Part 1 P0 #3 identified and claimed to be fixed.
Reading the Lote 10.5bis fix in §6.1: the cancel_token is passed to `verify_streaming`; the
mechanism "verify_streaming cancels token → upstream R2 stream aborted" describes cancellation of
the R2 read, not the client write. The forwarding pipeline is still underspecified.

**Fix (Lote 10.5ter)**:
- (a) Explicitly declare the per-chunk pipeline: "For each chunk i: (1) R2 GET returns chunk_bytes;
  (2) verifier.verify_chunk(i, chunk_bytes) → Ok or abort; (3) if Ok, write chunk_bytes to client
  sink. Steps 1→2→3 are sequential per chunk; parallelism is at the chunk-level (pipeline: while
  verifying chunk N+1, the client sink is writing chunk N already confirmed); no verified chunk is
  ever held beyond the verify-then-write sequence."
- (b) Add a latency analysis: pipelining verified chunks (1-chunk lookahead) adds 2 MiB buffer per
  connection but eliminates any window where unverified bytes reach the client. Document explicitly.
- (c) Add chaos test: "Chunk N bytes are correct; verify OK; forwarded. Chunk N+1 bytes tampered;
  verify catches; handler signals client abort via gRPC status ABORTED or HTTP 499. Assert: client
  receives exactly N complete chunks + error; no partial chunk N+1 bytes delivered."

---

### P0-SR5-003 — Bounded parser memory budget: 81920 chunks × Vec<ChunkRef> exceeds the 4 MiB stack claim

**Severity**: P0 — INV-MULTIPART-STREAMING-MEMORY; INV-MULTIPART-BOUNDED-PARSER clash.
**WI**: WI-S05-005 §1 (Manifest struct); §14.s05.005.9 (≤ 32 KiB stack); WI-S05-001 §6.1 (4 MiB
stack per request); spec contract §14.s05.1 (zero allocation hot path).

WI-S05-005 §1 declares:
```rust
pub struct Manifest {
    pub chunks: Vec<ChunkRef>,  // ordered chunk references
}

pub struct ChunkRef {
    pub index: u32,             // 4 bytes
    pub digest: [u8; 32],       // 32 bytes
    pub size_bytes: u32,        // 4 bytes
}
```

`ChunkRef` = 40 bytes per instance (with alignment). `Vec<ChunkRef>` with capacity 81920 =
81920 × 40 bytes = **3.28 MB heap allocation** just for the chunks vector.

WI-S05-005 §14.s05.005.9 says "Memory bounded ≤ 32 KiB stack per verify." The 3.28 MB is heap
not stack, so this specific claim is not literally violated. But:

- WI-S05-001 §6.1 "Memory bound: per-request stack ≤ 4 MiB (chunker buffer 2 MiB + headroom)."
  The 3.28 MB Vec<ChunkRef> plus the manifest's other fields (blob_digest 32 bytes, merkle_root
  32 bytes, sig Vec<u8> 32 bytes, tenant_id, etc.) brings the manifest in-memory size to ≥ 3.3 MB.
- INV-MULTIPART-STREAMING-MEMORY in the registry §3.16: "Per-request stack ≤ 4 MiB (chunker buffer
  2 MiB + headroom)." If both the chunker (2 MiB buffer) and the manifest (3.3 MB heap) are live
  simultaneously during SplitBlob, total heap ≥ 5.3 MB per request. Under high concurrency
  (semaphore 4 per tenant × N tenants), the CF Worker 128 MiB memory cap can be reached with
  128 / 5.3 ≈ 24 concurrent split operations — less than 6 tenants at max concurrency.
- WI-S05-005 §3 SLA addendum: "Streaming memory bound ≤ 4 MiB stack per invocation." This claim
  explicitly includes the streaming verify path. During `verify_streaming`, the entire Manifest
  (including all 81920 ChunkRefs) must be resident to validate incoming chunks against
  `manifest.chunks[i].digest`. So the full 3.3 MB Vec is live during streaming verify.

**The bounded-parser protection correctly rejects manifests claiming > 81920 chunks at decode time,
but a valid 81920-chunk manifest then consumes 3.3 MB + overhead for its lifetime. The spec claims
"bounded" but the bound is 5.3 MB+ per request, not the 4 MiB stated.**

**Fix (Lote 10.5ter)**:
- (a) Update INV-MULTIPART-STREAMING-MEMORY to: "Chunker: ≤ 2 MiB buffer; Manifest (worst-case
  81920 chunks): ≤ 3.5 MB heap; combined per-request peak: ≤ 6 MB. CF Worker 128 MiB cap supports
  ≥ 21 concurrent split/verify ops per Worker instance."
- (b) Update WI-S05-001 §6.1 memory bound: "per-request stack ≤ 4 MiB (chunker) + heap ≤ 3.5 MB
  (manifest 81920-chunk worst case) = ≤ 7.5 MB total per split op. CF Worker 128 MiB / 7.5 MB =
  17 max concurrent ops; semaphore at 4 per tenant caps tenant concurrency but not global."
- (c) Design a streaming manifest decoder that does NOT require the full Vec<ChunkRef> in memory at
  once during `verify_streaming`: verifier reads from D1 `manifest_chunks` table per-chunk (already
  there from the batch INSERT in step [10]) rather than from the in-memory `Manifest.chunks`. This
  reduces verify memory to O(1) per chunk (one D1 row at a time). Document as a design decision:
  "verify_streaming reads from D1 manifest_chunks per-chunk; does NOT hold full Vec<ChunkRef> in
  memory; caps streaming verify memory at O(1)." This is architecturally cleaner and already
  aligned with the D1 manifest_chunks table's purpose.
- (d) If the in-memory Vec<ChunkRef> is retained for other paths (e.g., manifest::build), cap its
  lifetime and document drop timing.

---

### P0-SR5-004 — ADR-0040 substantive but cross-shard orphan session blind spot in sweeper during shard split

**Severity**: P0 — FM-060 mitigation correctness; INV-MULTIPART-ORPHAN-DETECTABLE.
**WI**: WI-S05-006 §6.1.1 (sweeper); ADR-0040 (migration plan D-30 to D-0).

ADR-0040 now has a real sharding design (per-region, 5 shards, dual-write migration). The migration
plan describes a "dual-write window (handler writes to BOTH old + new shard; reads from old;
reconcile job migrates historical rows)" at D-10 to D-5 before read cutover.

**The sweeper blind spot**: WI-S05-006 §6.1.1 says "D1 SELECT multipart_sessions WHERE
state=in_progress AND last_activity_at < now-7d." The sweeper queries a D1 shard. During the
shard-split dual-write window (D-10 to D-5 of a shard split), new multipart sessions are written
to BOTH old and new shards. A session initiated during dual-write is present in both shards. The
sweeper (which queries per-region) may query only the old shard and abort the session, then
encounter the duplicate session record in the new shard — attempting to abort an already-aborted
upload_id. R2 AbortMultipartUpload for an already-completed or already-aborted upload returns 404.
The sweeper should handle this gracefully, but the spec's orphan detection guarantees
("detect ALL orphans ≤ 7d") are violated during the shard split window if the sweeper is not
updated to query both old and new shards.

**Worse**: if a session is initiated in the OLD shard only (before dual-write starts, i.e., a
session from D-40 that's still in_progress at D-10) and the READ CUTOVER at D-5 switches reads to
the new shard, the sweeper post-cutover queries the new shard and CANNOT SEE that session (it was
never migrated to the new shard because dual-write started at D-10 and the session predates it).
ADR-0040 §4 "D-10 to D-5: reconcile job migrates historical rows" — but the reconcile job scope
and the multipart_sessions table migration are not specified. If reconcile migrates cas_blobs and
chunks but not multipart_sessions (which is a "live" table, not historical), in-progress sessions
from before the shard split become orphan detection black holes.

**Fix (Lote 10.5ter)**:
- (a) ADR-0040 migration plan: add explicit clause for `multipart_sessions` table: "multipart_sessions
  rows with state='in_progress' at D-10 are migrated to new shard before read cutover; reconcile
  job must include multipart_sessions in scope."
- (b) WI-S05-006 §6.1.1 sweeper: add shard-split awareness: "Sweeper queries all active D1 shards
  per region, not just the current primary shard; during shard split dual-write window, queries
  both old and new shard; handles 404 from R2 AbortMultipartUpload gracefully (already aborted or
  completed)."
- (c) Add chaos test #N to WI-S05-006: "Mid-shard-split orphan detection: session S started
  pre-dual-write; sweeper fires post-read-cutover; assert S is detectable and aborted within 7d
  SLA."
- (d) INV-MULTIPART-ORPHAN-DETECTABLE in registry §3.16 must add: "Guarantee holds across D1 shard
  splits; sweeper multi-shard aware during split transitions."

---

## P0 Findings Confirmed from R4 (still unresolved in Lote 10.5bis)

### P0-CONF-001 — 13-row sign-off table: 10-of-13 roles unfilled (R4 Part 2 P0 #5, carried forward)

**WI**: WI-S05-006 §30; all individual WI sign-off tables.

Confirmed still unresolved. WI-S05-006 §30 shows Gustavo covering roles 1-2 (Owner/Final Approver)
and role 8 (Product) = 3 entries for 1 person. Roles 3-7, 9-12, 13 show `_TBD_` or
`_staffing-blocked_`. Nine human roles unidentified. ADR-0034 waives only the SRE Lead role.

No new mitigation visible in Lote 10.5bis. This is a program-level defect, not a per-WI defect.
The sprint cannot literally SEAL with 9 mandatory roles absent. Either: (a) the program accepts
that S-05 will not ship at SOTA bar without naming real humans; or (b) the program explicitly
documents a reduced staffing model (e.g., 5 sign-offs instead of 13) with an ADR reducing the
HIGH_RISK requirement for a solo-engineer project.

**Sonnet assessment**: R4 is correct; the finding is unresolved; it is the single largest gap
between specification quality and operational reality. The spec at this level of detail deserves
at minimum a realistic staffing model, even if that model is "5 sign-offs with ADR-0034 waivers
for the other 8 roles." Writing 13 rows with 10 TBDs is the definition of rubber-stamp.

### P0-CONF-002 — meta_manifests table not in WI-S05-004 but 160 GiB stitched flow integration test depends on it (R4 Part 2 P0 #1)

**WI**: WI-S05-006 §6.1.3; WI-S05-004 §6.2.

Confirmed. WI-S05-004 §6.2 out-of-scope explicitly lists "Cross-region replication (S-14)" and
"DSR cascade integration (S-11 forward)" but does NOT list meta_manifests — i.e., meta_manifests is
neither in-scope nor out-of-scope in WI-S05-004. WI-S05-006 §6.1.3 says "D1 schema additions
(forward S-14 OR S-09): meta_manifests table." The integration test "200 GiB synthetic blob; stitched
flow successful" is a DoD item for WI-S05-006 but there is no persistent store for meta_manifests
in this sprint. The test can only pass with mocked/in-memory meta-manifest storage. Still unresolved.

### P0-CONF-003 — REAPI conformance "AC subset" typo in WI-S05-006 §6.1.4 (R4 Part 2 P0 #2)

Confirmed. WI-S05-006 §6.1.4 says "Subset enumerated: 10 conformance tests covering CAS multipart
subset (Lote 10.5bis P0 fix: was wrongly AC subset — copy-paste from WI-S04-006; actual CAS
SplitBlob/SpliceBlob subset)." R4's finding is marked as fixed in the Lote 10.5bis text, but the
fix text itself is garbled: "Lote 10.5bis P0 fix: was wrongly AC subset" is inline commentary, not
clean spec text. The normative claim is now "CAS multipart subset" which is correct. **Partially
resolved** — the wrong value is corrected but the fixup language leaves editorial noise.

---

## P1 Findings (Sonnet-new)

### P1-SR5-001 — WI-S05-002 `bounds.rs` exports MAX_CHUNKS_PER_BLOB = 80000 but WI-S05-005 uses 81920

**WI**: WI-S05-002 §6.1 `src/bounds.rs`; WI-S05-005 §1 MAX_CHUNK_COUNT = 81920.

WI-S05-002 §6.1 in-scope item 1: "`src/bounds.rs`: constants (MAX_BLOB_SIZE = 160 GiB;
MAX_CHUNKS_PER_BLOB = 80000)." WI-S05-005 §1: "pub chunk_count: u32 // ≤ 81920 (Lote 10.5bis P0
fix off-by-one)." WI-S05-005 §14.s05.005.4: "Criterion benchmarks: verify ≤ 50ms p99 @ 80k chunks."

The off-by-one fix was applied to WI-S05-005 (81920) and WI-S05-004 schema CHECK
(`chunk_index < 81920`) but the WI-S05-002 `bounds.rs` constant still says 80000. These are
different crates (`corelink-chunker` and `corelink-manifest`). If WI-S05-001 handler imports
`MAX_CHUNKS_PER_BLOB` from `corelink-chunker::bounds` to validate the request, it uses 80000.
If it imports `MAX_CHUNK_COUNT` from `corelink-manifest::bounds`, it uses 81920. Cross-crate
constant mismatch means the rejection boundary depends on which crate the handler imports from.

**Fix**: Align both crates to 81920 OR introduce a shared `corelink-multipart-constants` crate
with a single source of truth. The WI-S05-002 §6.1 `bounds.rs` must read `MAX_CHUNKS_PER_BLOB =
81920`.

### P1-SR5-002 — SplitBlob manifest signing uses canonical_bytes BEFORE manifest is persisted, creating a sign-then-persist-or-abort pattern with orphan sig risk

**WI**: WI-S05-001 §6.1 steps [8] → [9]; WI-S05-005 `canonical_bytes()` layout.

The 102-byte canonical_bytes layout includes `created_at_ms` (bytes 93-100). The manifest is
signed at step [8] with `created_at_ms = now()`. The signed manifest is then persisted to R2 at
step [9]. If step [9] fails (R2 PUT timeout), the handler retries. On retry, does it re-sign with
a NEW `created_at_ms`, or reuse the old signature? The spec is silent.

If the handler generates a new `created_at_ms` on retry but reuses the previous signature, the
signature verification would fail (sig was over the old timestamp, new canonical_bytes has new
timestamp). If the handler re-signs with new timestamp, it generates a different signature, but the
prior (failed) R2 PUT may have partially written the old signed manifest. R2 S3-compatible PUT is
not atomic in all edge cases for large objects.

**Fix**: WI-S05-001 §6.1 step [8] must specify: "Sign exactly once; `created_at_ms` is captured
BEFORE signing and reused on retry of step [9]. The canonical_bytes is computed once and stored
in a local variable; subsequent retries of R2 PUT use the same envelope bytes and signature."

### P1-SR5-003 — WI-S05-005 Gherkin determinism scenario is still statistically vacuous after Lote 10.5bis

**WI**: WI-S05-005 §8 Gherkin; §10.s05.005.6; §1 invariant 2.

WI-S05-005 §8 Gherkin "Determinism — 1000 manifests × 100 builds = byte-identical" and
§10.s05.005.6 "Determinism property: 1000 × 100 builds = byte-identical" are unchanged from the
pre-Lote-10.5bis text. R4 Part 1 P0 #5 flagged this for WI-S05-002; the parallel claim in
WI-S05-005 survived the fix pass.

Running the same pure Rust function 100 times on the same input on the same binary is trivially
deterministic. The property test passes vacuously without testing the actual determinism risks:
(a) FastCDC mask seeds drift between crate versions; (b) platform-specific endianness differences;
(c) HashMap iteration order if any maps are used internally. The test as written is evidence
theater — it will always pass regardless of whether the actual determinism properties hold.

**Fix**: Same as R4 Part 1 P0 #5 recommendation — add test vectors regression test (golden
canonical_bytes hex per known manifest, asserted byte-for-byte on CI across platforms). The
"1000 × 100" framing should be replaced with "1000 unique inputs × golden-vector assertion."

### P1-SR5-004 — INV-MULTIPART-BOUNDED-PARSER in registry says MAX_CHUNKS 80000 but WI-S05-005 uses 81920

**WI**: invariant_registry.md §3.16 INV-MULTIPART-BOUNDED-PARSER; WI-S05-005 §1.

The registry row INV-MULTIPART-BOUNDED-PARSER says: "Chunker MAX_BLOB_SIZE 160 GiB;
MAX_CHUNKS_PER_BLOB 80000; manifest MAX_CHUNK_COUNT 80000; MAX_TOTAL_SIZE 160 GiB."

WI-S05-005 §1 now uses 81920 (Lote 10.5bis off-by-one fix). The registry §3.16 was NOT updated.
This is a spec-vs-registry drift. The CI gate `validate_inv_promotion.py` would only catch
INV-declaration presence, not numerical value consistency within the declaration.

**Fix**: Update registry §3.16 INV-MULTIPART-BOUNDED-PARSER to read "MAX_CHUNKS_PER_BLOB 81920;
manifest MAX_CHUNK_COUNT 81920." The off-by-one fix must be propagated to ALL locations: WI-S05-002
bounds.rs (see P1-SR5-001), WI-S05-005 §1 (done), WI-S05-004 CHECK (done), INV-MULTIPART-BOUNDED-PARSER
registry row (not done), sprint contract §5.1 (says "10k × 16 MiB = 160 GiB → 80,000 chunks" in
the implied math — not done).

### P1-SR5-005 — WI-S05-005 cargo-fuzz targets: 3 listed (decode + verify + sig) but §10 says 3 targets and R4 Part 2 P1 #9 recommended adding "builder" target; fix not applied

**WI**: WI-S05-005 §10.s05.005.4 and §6.1 scope item 10 and §13 artifacts.

WI-S05-005 §10.s05.005.4: "Cargo-fuzz 1h CI nightly (3 targets: decode + verify + sig) → 0 panics."
§6.1 scope item 10: "Cargo-fuzz harness 1h CI nightly (decode + verify + sig)."
§13 artifacts: "Cargo-fuzz harness: `crates/corelink-manifest/fuzz/fuzz_targets/` (3 targets)."

R4 Part 2 P1 #9 (WI-S05-005) recommended adding a builder (encode) fuzz target. The encode side
is where canonical_bytes serialization bugs live. A malformed manifest that the builder emits
(but that the verifier decodes incorrectly) could pass all decode/verify/sig targets while the
encoder has a silent divergence. Not fixed in Lote 10.5bis.

**Fix**: Add `fuzz_manifest_builder` (4th target). 1-line change in §6.1 + §10 + §13. This is
the same P1 from R4 not fixed by Lote 10.5bis.

### P1-SR5-006 — Mann-Whitney |Δmedian| ≤ 1ms for manifest verify is achievable but requires constant-time per-leaf (not whole-tree); Sonnet disagrees with R4's framing

**WI**: WI-S05-005 §8 Gherkin timing; §10.s05.005.2; §1 Mann-Whitney.

R4 Part 2 P1 #6 argued that |Δmedian| ≤ 1ms is impossible for manifest verify because a tampered
manifest exits early (fast) vs a valid manifest (slow), so the timing delta is 45ms not 1ms.

**Sonnet disagrees with R4's conclusion**, though the underlying analysis is correct. The resolution
is: the WI does not need to abandon the 1ms gate — it needs to **scope it correctly to the
per-leaf BLAKE3 comparison, not the whole-tree traversal**.

- `verify_structure` re-computes the Merkle tree from all `chunk.digest` values in `manifest.chunks`.
  This is O(N) leaf hashes + O(N) inner hashes. The operation is deterministic in time regardless
  of tampering (you still compute all N leaves before comparing to the claimed root). Time = N ×
  BLAKE3(32 bytes) + N × BLAKE3(64 bytes) ≈ constant for fixed N.
- For N = 25 (50 MiB blob), total tree computation ≈ 25 × 2 × 1μs = 50μs << 1ms. For N = 81920
  (160 GiB blob), total = 81920 × 2μs ≈ 164ms >> 1ms.
- **The correct Mann-Whitney target is: timing of valid vs tampered manifest verify for the SAME N
  chunks MUST be |Δmedian| ≤ 1ms.** This is achievable because `verify_structure` computes the full
  tree for BOTH valid AND tampered manifests (fail-fast occurs ONLY after the full recomputation;
  the early exit is `root_computed != manifest.merkle_root` which happens AFTER the O(N) hash pass).
  For the same N, valid and tampered manifests take identical time through the tree computation —
  the timing oracle is eliminated by design, not by constant-time tricks.

**Fix**: WI-S05-005 §8 Gherkin timing scenario should clarify: "Mann-Whitney |Δmedian| ≤ 1ms
between valid and tampered manifests OF THE SAME CHUNK COUNT N. Verifier computes full tree for
both; fail-fast occurs only at root comparison post-traversal. Timing equality guaranteed by O(N)
constant work per N. Gate is per-chunk-count; calibrate 10k valid vs 10k tampered at N=25 AND
N=1000 AND N=81920 (different N = different absolute timing; same N = equal timing)." This makes
the Mann-Whitney test correctly scoped rather than abandoned.

---

## Sonnet vs Opus Differential

### Where Sonnet confirms Opus

- P0-CONF-001 (staffing reality, 13-row sign-off): fully confirmed. Program needs a system-level
  fix, not WI patches. Sonnet assessment: **this is the single highest-risk program delivery gap**.
- P0-CONF-002 (meta_manifests schema missing): fully confirmed. Still unresolved.
- R4 Part 2 P0 #3 (ADR-0040 procedural rubber-stamp): SUPERSEDED — ADR-0040 is now substantive
  (Sonnet verified). R4 correctly predicted the risk; the Lote 10.5bis fix actually addressed it.
  **Credit to the author.** However, Sonnet found a new ADR-0040-derived defect (cross-shard orphan
  blind spot, P0-SR5-004).
- R4 Part 1 P0 #4 (Iterator allocation): correctly fixed by pull-based `next_chunk` API. Confirmed
  resolved.
- R4 Part 1 P0 #7 (BLAKE3 throughput recalibration): correctly fixed to ≥ 500 MB/s native +
  ≥ 200 MB/s WASM. Confirmed resolved.
- R4 Part 2 P1 #11 (MAX_CHUNK_COUNT 80000 off-by-one): fix applied in WI-S05-005 and WI-S05-004
  but NOT in WI-S05-002 bounds.rs or invariant registry (Sonnet P1-SR5-001, P1-SR5-004).

### Where Sonnet disagrees with Opus

- **R4 Part 2 P1 #6 (Mann-Whitney 1ms impossible for manifest verify)**: Sonnet disagrees with
  the conclusion. R4 says the test will fail because valid (slow) vs tampered (fast, early exit)
  manifests have wildly different timing. Sonnet's analysis: `verify_structure` computes the full
  O(N) tree for BOTH paths; there is no early exit before the full leaf computation completes;
  timing equality is achieved by design, not by constant-time primitives. The fix is to scope
  the Mann-Whitney test per-N, not to abandon it. R4's fix recommendation (reframe to per-leaf
  verify or drop the gate) would weaken the security evidence chain unnecessarily.
- **R4 Part 2 P0 #3 (ADR-0040 rubber-stamp)**: Partially disagrees. R4 flagged this as P0; Sonnet
  confirms the fix was applied substantively. However, the fix introduced P0-SR5-004 (orphan blind
  spot during shard splits) — a new defect from the new design content, not a continuation of the
  rubber-stamp defect.

### What Sonnet found that Opus did not

1. **Manifest signing race** (P0-SR5-001): the temporal ordering of chunk PUT verification and
   manifest signing is ambiguous in the spec. A parallel R2 PUT failure could produce a signed
   manifest pointing to a non-existent chunk. This is a new finding with cripto-operational
   implications.
2. **SpliceBlob forwarding-before-verify ambiguity** (P0-SR5-002): the "parallel with R2 chunk
   streaming" + "only after per-chunk verify passes" language is still contradictory post-Lote-10.5bis,
   just in a subtler way than the original three-position contradiction. The CancellationToken fix
   addresses cancellation of the R2 read, not the timing of client write vs verify.
3. **Bounded parser memory math** (P0-SR5-003): the 81920-chunk Vec<ChunkRef> at 40 bytes each
   = 3.28 MB heap directly contradicts the 4 MiB total per-request claim when the chunker's 2 MiB
   buffer is also active. The arithmetic was never done in the spec.
4. **ADR-0040 cross-shard orphan blind spot** (P0-SR5-004): the now-substantive ADR-0040
   introduces a new gap in the sweeper's orphan detection guarantee during shard-split transitions.
5. **Cross-crate constant drift** (P1-SR5-001): the 81920 fix was applied inconsistently across
   crates; corelink-chunker bounds.rs still says 80000.
6. **Sign-then-persist timestamp replay risk** (P1-SR5-002): `created_at_ms` in canonical_bytes
   is captured at signing time; retry semantics on step [9] failure must reuse the original
   `created_at_ms` + signature or risk sig verification failure.

---

## Cross-WI Consistency (Sonnet independent check)

| Check | Result |
|---|---|
| MAX_CHUNK_COUNT across WI-001, WI-002, WI-004, WI-005, registry | DRIFT: 001=80000, 002=80000, 004 CHECK <81920, 005=81920, registry=80000 |
| HKDF info strings: `b"ac-sig"` / `b"manifest-sig"` / `b"meta-manifest-sig"` | CORRECT: all distinct, no prefix relationship |
| canonical_bytes 102-byte layout consistency WI-001 vs WI-005 | CONSISTENT: WI-001 §6.1 step [8] references WI-005 §1 layout |
| CancellationToken abort mechanism WI-001 + WI-005 | CONSISTENT: both reference `tokio_util::sync::CancellationToken` |
| ADR-0040 per-region sharding in WI-S05-004 vs ADR-0040 text | CONSISTENT: 5 shards (sam/iad/lhr/nrt/syd) |
| tenant_prefix BLOB(16) materialized in WI-S05-004 schema | CONSISTENT with WI-S05-006 sweeper consumption |
| Sweeper batch 250 sessions/tick in WI-006 vs D1 100KB lesson | CONSISTENT |
| REAPI conformance subset: WI-006 §8 Gherkin (CAS) vs §6.1.4 fix commentary (CAS) | CONSISTENT post-fix text but editorial noise |
| INV §3.16 count: WI-006 §12 says ~16 but "13 INVs" in headers | DRIFT: headers say 13; §12 enumeration gives ~13 NEW; but registry §3.16 comment says 22 total IDs |

---

## Verdict per WI

| WI | Verdict | Key blocker |
|---|---|---|
| WI-S05-001 | GO-WITH-FIXES | P0-SR5-001 (signing race) + P0-SR5-002 (streaming verify ambiguity) |
| WI-S05-002 | GO-WITH-FIXES-LIGHT | P1-SR5-001 (bounds.rs constant drift) + P1-SR5-003 (vacuous determinism test) |
| WI-S05-003 | GO-WITH-FIXES | R4 Part 1 P0 #8 (chaos <10) partially resolved; cross-shard orphan (P0-SR5-004 upstream) |
| WI-S05-004 | GO-WITH-FIXES | P1-SR5-004 (registry MAX_CHUNK_COUNT drift) + R4 P0 #4 refcount race (S-06 GC) |
| WI-S05-005 | GO-WITH-FIXES | P0-SR5-003 (memory budget math); P1-SR5-005 (missing fuzz builder target) |
| WI-S05-006 | GO-WITH-FIXES | P0-CONF-001 (13 sign-offs unfilled) + P0-CONF-002 (meta_manifests) + P0-SR5-004 (sweeper sharding) |

**Program-level verdict**: S-05 post-Lote-10.5bis is at **8.18/10 average** — above the S-04
baseline (8.05 part 1, 7.83 part 2) and confirming the improvement trajectory. The highest-leverage
single fix is P0-SR5-001 (manifest signing race) because it affects the fundamental integrity
guarantee of the entire multipart path. P0-SR5-003 (memory budget math) is the cheapest fix (pure
arithmetic correction + design clarification, 2h). P0-CONF-001 (staffing) requires a program
decision, not a spec patch. The package does NOT clear the 9.0 SOTA bar with these P0s open.

---

*End of Sonnet R5 independent audit. Findings are independent of Agent R4 analysis except where
explicitly noted as confirmations or disagreements.*
