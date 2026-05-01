---
id: "AUDIT-2026-05-01-ADVERSARIAL-S05"
type: "audit_report"
doc_status: "FROZEN"
audit_status: "SEALED"
version: "1.0.0"
created: "2026-05-01"
updated: "2026-05-01"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "adversarial", "s05", "multipart", "summary"]
---

# Adversarial test summary — S-05 Multipart CAS

> **Sprint:** S-05 (Multipart Upload + Chunking + Merkle dual-side) · **WI:** WI-S05-006 §6.1.5 + §15
> **Date:** 2026-05-01 · **Mode:** aggregation across S-05 individual WIs

Aggregates the adversarial test surfaces stitched into the
WI-S05-001..005 canonical test corpus + the WI-S05-006 sweeper
sub-module. Pairs with the internal pentest report
(`2026-05-01-pentest-s05-internal.md`) — the pentest narrates the
attack-surface verdict; this doc enumerates the test cases.

## 1. Per-WI adversarial scenarios

### WI-S05-001 — REAPI SplitBlob/SpliceBlob pure-logic handler module

| ID | Scenario | Test surface | Result |
|---|---|---|---|
| ADV-S05-001-01 | Cross-tenant SpliceBlob probe surfaces ManifestNotFound (INV-MULTIPART-PATH-TENANT-SCOPED + INV-TENANT-ISOLATION) | `prop_split_splice::prop_split_tenant_isolation` | 10k iter, 0 leak |
| ADV-S05-001-02 | Re-init on the same `(tenant, blob_digest)` post-finalize echoes `AlreadyChunked` with byte-equal manifest_digest | `prop_split_splice::prop_split_idempotent_finalize` | 10k iter, 0 mismatched outcome |
| ADV-S05-001-03 | Aborted session refuses every subsequent mutation; never produces a manifest | `prop_split_splice::prop_abort_safety` | 1k iter, 0 post-abort manifest |
| ADV-S05-001-04 | Out-of-order chunk_index rejected with `ChunkOrderingViolation`; canonical ordering succeeds | `prop_split_splice::prop_chunk_ordering_canonical` | 1k iter, 0 acceptance |
| ADV-S05-001-05 | Append duplicate chunk_index with **different** bytes rejected (`bytes_mismatch` arm) | `cas::session::tests::append_chunk_bytes_mismatch_rejected` | 0 acceptance |
| ADV-S05-001-06 | Cross-tenant session_id append rejected as `SessionNotFound` (mask via lookup) | conformance `ct_split_blob_cross_tenant_session_isolation` | 0 acceptance |
| ADV-S05-001-07 | Append on Live → bound chunk count grows monotonically | `cas::session::tests::append_chunk_canonical_order` | 0 backwards step |

### WI-S05-002 — corelink-chunker FastCDC + ADR-0022

| ID | Scenario | Test surface | Result |
|---|---|---|---|
| ADV-S05-002-01 | Chunker determinism (same input → byte-equal chunk sequence) | `corelink-chunker::tests::prop::prop_chunker_determinism` | 10k iter, 0 drift |
| ADV-S05-002-02 | FastCDC mask seed determinism + Gear table compile-time fixed-from-SplitMix64 | `corelink-chunker::tests::canonical_vectors::*` | byte-stable across runs |
| ADV-S05-002-03 | Bounds enforcement: MAX_CHUNKS_PER_BLOB = 81920; MAX_BLOB_SIZE = 160 GiB; INV-MULTIPART-BOUNDED-PARSER | `corelink-chunker::tests::bounds_enforcement::*` | 13 bound boundaries enforced |
| ADV-S05-002-04 | Streaming memory bound O(staging buffer); zero allocation hot path | `corelink-chunker::tests::prop::prop_zero_alloc_hot_path` | 10k iter, 0 allocation outside staging buffer |
| ADV-S05-002-05 | Cross-input chunk-digest distinction (no false dedup of distinct content) | `corelink-chunker::tests::prop::prop_distinct_inputs_distinct_digests` | 10k iter, 0 false collision |
| ADV-S05-002-06 | wasm32-clean (no tokio in src/) | crate-level lint + Cargo deps audit | 0 reactor deps |

### WI-S05-003 — R2 multipart adapter

| ID | Scenario | Test surface | Result |
|---|---|---|---|
| ADV-S05-003-01 | Tenant isolation: `(tenant_a, upload_id_a)` ↔ `(tenant_b, upload_id_a)` mismatch rejected (`CrossTenantUpload`) | `corelink-r2-multipart::tests::prop::prop_tenant_isolation` | 10k iter, 0 acceptance |
| ADV-S05-003-02 | Idempotent operations: re-`initiate` same blob echoes prior upload_id; re-`complete` returns cached `CompletedObject` | `corelink-r2-multipart::tests::prop::prop_idempotent_operations` | 10k iter, 0 divergent state |
| ADV-S05-003-03 | Abort safety: abort on Completed rejected (INV-MULTIPART-FINALIZE-IRREVOCABLE) | `corelink-r2-multipart::tests::chaos::abort_on_completed_rejected` | 0 acceptance |
| ADV-S05-003-04 | Part ordering canonical (canonical PartNumber 1..=10_000) | `corelink-r2-multipart::tests::prop::prop_part_ordering` | 10k iter, 0 mis-ordering |
| ADV-S05-003-05 | Concurrency limit per tenant (default 8 permits; configurable) trips `ConcurrencyLimitReached` | `corelink-r2-multipart::tests::chaos::concurrency_limit_trips` | 0 unbounded acceptance |
| ADV-S05-003-06 | Part body > 5 GiB rejected; PartNumber > 10_000 rejected (`MaxPartsExceeded`) | `corelink-r2-multipart::tests::chaos::*` | 7 chaos scenarios pass |
| ADV-S05-003-07 | Cross-tenant replay rejected + legitimate tenant unblocked (no cross-tenant DoS) | `corelink-r2-multipart::tests::chaos::cross_tenant_replay_isolated` | 0 cross-tenant impact |
| ADV-S05-003-08 | `list_orphans` consumed only by sweeper; no public abort-by-bucket-scan API | `corelink-r2-multipart::adapter` trait surface | structural seam |

### WI-S05-004 — D1 multipart_chunks + manifest + sessions schema

| ID | Scenario | Test surface | Result |
|---|---|---|---|
| ADV-S05-004-01 | chunks PK uniqueness on `(tenant_id, chunk_digest)` | `corelink-multipart-schema::tests::prop::pk_uniqueness_holds` | 10k iter, 0 acceptance |
| ADV-S05-004-02 | chunks idempotent UPSERT — refcount += 1 on same `(tenant_id, chunk_digest)` | `corelink-multipart-schema::tests::idempotency_canonical::*` | 15 canonical scenarios pass |
| ADV-S05-004-03 | manifest_chunks PK + UNIQUE violation on PK collision with mismatched chunk_digest | `corelink-multipart-schema::tests::prop::manifest_chunks_unique` | 10k iter, 0 acceptance |
| ADV-S05-004-04 | multipart_sessions partial UNIQUE INDEX `uq_multipart_sessions_in_progress` allows completed/aborted coexist | `corelink-multipart-schema::tests::prop::partial_unique_in_progress` | 10k iter, semantics correct |
| ADV-S05-004-05 | State graph monotone `in_progress → completed | aborted` (INV-MULTIPART-STATE-MONOTONIC) | `corelink-multipart-schema::tests::prop::state_monotone` | 10k iter, 0 reverse |
| ADV-S05-004-06 | Cross-tenant query structurally impossible (every PK leftmost is tenant_id) | `corelink-multipart-schema::tests::prop::cross_tenant_isolation` | 10k iter, 0 leak |
| ADV-S05-004-07 | Migration re-apply preserves rows (additive-only) | `corelink-multipart-schema::tests::migration_canonical::*` | 19 canonical migration scenarios pass |
| ADV-S05-004-08 | tenant_prefix BLOB(16) materialized on chunks + multipart_sessions per ADR-0035 H-3 | `corelink-multipart-schema::tests::prop::tenant_prefix_materialized` | 10k iter, 0 plaintext leakage |

### WI-S05-005 — corelink-manifest Merkle builder + dual-side verifier

| ID | Scenario | Test surface | Result |
|---|---|---|---|
| ADV-S05-005-01 | Manifest root determinism (same input → byte-equal root) | `corelink-manifest::tests::prop::prop_manifest_determinism` | 10k iter, 0 drift |
| ADV-S05-005-02 | Manifest tampering detected (bit-flip of any leaf / inner node / sig) | `corelink-manifest::tests::prop::prop_tampering_detection` | 10k iter, 0 missed |
| ADV-S05-005-03 | Streaming memory bound O(1) — verifier reads one chunk at a time from manifest store seam (INV-MULTIPART-STREAMING-MEMORY) | `corelink-manifest::tests::streaming_memory::*` | 3 canonical bound checks pass |
| ADV-S05-005-04 | Chunk ordering canonical (manifest leaf order matches chunker output) | `corelink-manifest::tests::prop::prop_chunk_ordering` | 10k iter, 0 mis-ordering |
| ADV-S05-005-05 | Bounds enforcement: MAX_CHUNKS_PER_BLOB = 81920 cross-crate aligned | `corelink-manifest::tests::prop::prop_bounds_enforcement` | 10k iter, every overflow rejected |
| ADV-S05-005-06 | Round-trip codec stable + pre-allocation rejection on bounds | `corelink-manifest::tests::prop::prop_round_trip_codec` | 10k iter, 0 reject-then-accept asymmetry |
| ADV-S05-005-07 | Cross-tenant manifest rejected (cross-crate adapter projection) | `corelink-manifest::tests::prop::prop_cross_tenant_rejected` | 10k iter, 0 acceptance |
| ADV-S05-005-08 | RFC 6962 domain separation `\x00`-leaf / `\x01`-inner — sibling-domain to corelink-ac (HKDF info `b"manifest-sig"` ≠ `b"ac-sig"`) | 10 canonical_vectors + 13 tampering | 0 cross-domain collision |
| ADV-S05-005-09 | HKDF-SHA256 manifest signer real impl; sibling-domain separation | `corelink-manifest::sig::*` 52 unit tests | 0 sig drift |

### WI-S05-006 — Sweeper Cron DO + RB-FM-060 dry-run + cross-component prop + REAPI conformance

| ID | Scenario | Test surface | Result |
|---|---|---|---|
| ADV-S05-006-01 | Sweeper aborts ONLY tenant-scoped stale Live sessions (cross-tenant abort structurally unreachable) | `prop_multipart_full::prop_multipart_full_stack_sweeper_orphan_abort_isolated` | 10k iter, 0 cross-tenant abort |
| ADV-S05-006-02 | Sweeper region-pinned (no cross-region abort) | `cas::sweeper::tests::tick_pinned_to_region_skips_other_regions` | 0 cross-region acceptance |
| ADV-S05-006-03 | Sweeper bounded batch (`MAX_BATCH_SIZE = 250`); ceiling rejected at construction | `cas::sweeper::tests::batch_size_above_ceiling_rejected` | 0 unbounded acceptance |
| ADV-S05-006-04 | Sweeper canonical reason `orphan_swept` on every emitted audit | `cas::sweeper::tests::tick_aborts_orphans_older_than_age_cutoff` | 0 reason drift |
| ADV-S05-006-05 | Sweeper idempotent under repeat tick (no double-abort on already-Aborted rows) | `cas::sweeper::tests::tick_idempotent_under_repeat` | 0 duplicate audit emits |
| ADV-S05-006-06 | Sweeper finalized session under cutoff is NOT swept (state-monotone defense) | `cas::sweeper::tests::finalized_session_under_age_cutoff_is_not_swept` | 0 sweep on finalized |
| ADV-S05-006-07 | Cross-component 100k iter SHIP-GATE on tenant isolation across full stack (handler + cas + chunker + r2-multipart + multipart-schema + manifest + audit + sweeper) | `prop_multipart_full::prop_multipart_full_stack_tenant_isolation_100k` | 100k iter, 0 leak |
| ADV-S05-006-08 | Streaming verify fail-fast: tampered chunk MUST raise `ChunkVerificationFailed` BEFORE bytes leak to caller's sink | `prop_multipart_full::prop_multipart_full_stack_streaming_verify_fail_fast` | 10k iter, 0 tampered-byte leak |
| ADV-S05-006-09 | REAPI v2 SplitBlob/SpliceBlob conformance suite 100% pass non-negotiable | `reapi_v2_split_splice_conformance::reapi_v2_split_splice_conformance_suite_all_pass` | 10/10 pass |
| ADV-S05-006-10 | Idempotent finalize across full stack (re-init echoes prior manifest_digest + chunk_count) | `prop_multipart_full::prop_multipart_full_stack_idempotent_finalize` | 10k iter, 0 mismatch |
| ADV-S05-006-11 | RB-FM-060 host-side dry-run drift detection (`scripts/rb_fm_060_dry_run.sh`) | host-side cargo-driven walkthrough | 0 drift; every step OK |
| ADV-S05-006-12 | Sweeper canonical metric pairs surface in DASH-MULTIPART (per WI §6.1.1 metrics) | `cas::sweeper::canonical_metric_pairs` + DASH-MULTIPART JSON | 6 canonical names match |

## 2. Cross-component / cross-stack scenarios (WI-S05-006 §6.1.5)

| ID | Scenario | Test surface | Result |
|---|---|---|---|
| ADV-S05-CROSS-01 | Tenant isolation under FULL pipeline (handler → r2-multipart → multipart-schema → chunker → manifest → audit) at 100k iter PR cripto-grade | `prop_multipart_full_stack_tenant_isolation_100k` | 100k iter, 0 leak |
| ADV-S05-CROSS-02 | Idempotent finalize under FULL pipeline | `prop_multipart_full_stack_idempotent_finalize` | 10k iter, 0 mismatch |
| ADV-S05-CROSS-03 | Sweeper orphan abort isolated under FULL pipeline (mixed Live/Finalized/Aborted cohort) | `prop_multipart_full_stack_sweeper_orphan_abort_isolated` | 10k iter, 0 cross-tenant abort |
| ADV-S05-CROSS-04 | Streaming SpliceBlob fail-fast under FULL pipeline tampered chunk injection | `prop_multipart_full_stack_streaming_verify_fail_fast` | 10k iter, 0 tampered-byte leak |
| ADV-S05-CROSS-05 | DSR multipart purge cascade — tenant erasure removes chunks + manifest_chunks + multipart_sessions atomically | inherits S-03 DSR PAT export integration; multipart-schema simulator preserves cascade | DEFERRED to S-13 admin plane (forward); host-side simulator covers cascade structurally |

## 3. Counts

- Per-WI prop tests: ~70 distinct properties at 10k iter PR.
- Cross-component prop tests: 4 properties at 100k SHIP-GATE + 3 × 10k = 130k iter PR.
- REAPI v2 conformance: 10 pinned tests; 100 % pass non-negotiable.
- Canonical vectors: ~50 across S-05 (chunker + manifest + r2-multipart + sig).
- Sweeper sub-module: 10 unit tests + 4 cross-component property tests.
- Total adversarial scenarios catalogued: **~80** across S-05 implementation.

## 4. Verdict

Across the WI-S05-001..006 corpus the adversarial review surfaced
**zero HIGH/CRITICAL** findings during S-05 implementation. The
codex / Sonnet adversarial review across cycles closed all P0 + P1
findings with documented changelog entries (per spec contract S-05
§16 v1.7.0..v1.8.0 + this WI). The single MEDIUM (S-CROSS-05 DSR
cascade) is forward-looking — S-13 admin plane scope; the multipart-
schema simulator already covers the cascade invariant structurally so
the production wiring inherits the property.

**Authored:** 2026-05-01 by Gustavo Schneiter (via Claude Opus 4.7
1M; orchestrator-finalized as part of WI-S05-006 SEAL Lote).
