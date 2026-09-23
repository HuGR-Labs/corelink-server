---
type: "CacheSurface"
title: "Native CAS surface"
description: "CoreLink's first-party content-addressable storage surface — the GET/PUT/DELETE/list + bulk-batch CAS routes every other surface ultimately stores into."
source_files:
  - "crates/corelink-container/src/routes/cas.rs"
  - "crates/corelink-container/src/routes/cas/foundation_core.rs"
  - "crates/corelink-container/src/routes/cas/foundation_state.rs"
  - "crates/corelink-container/src/routes/cas/single_setup.rs"
  - "crates/corelink-container/src/routes/cas/single_handlers.rs"
  - "crates/corelink-container/src/routes/cas/batch_write.rs"
  - "crates/corelink-container/src/routes/cas/batch_read.rs"
  - "crates/corelink-container/src/routes/cas/list_delete.rs"
  - "crates/corelink-container/src/routes/cas/tests_core_part1.rs"
  - "crates/corelink-container/src/routes/cas/tests_core_part2.rs"
  - "crates/corelink-container/src/routes/cas/tests_batch_part1.rs"
  - "crates/corelink-container/src/routes/cas/tests_batch_part2.rs"
  - "crates/corelink-container/src/routes/cas/tests_batch_write_part2.rs"
  - "crates/corelink-container/src/routes/cas/tests_batch_cancellation_part3.rs"
  - "crates/corelink-container/src/routes/cas/tests_read_ceiling.rs"
  - "crates/corelink-container/src/routes/cas/tests_edges.rs"
  - "crates/corelink-container/src/storage/r2_s3_parts/cas_core.rs"
source_blobs:
  - "crates/corelink-container/src/routes/cas.rs@41c65ce80c66084ef424ac4189b644bbe8461f95"
  - "crates/corelink-container/src/routes/cas/foundation_core.rs@63671990ccb9f0e9e6cba8804457737b42c18120"
  - "crates/corelink-container/src/routes/cas/foundation_state.rs@8dcd20ecd23850e331763d5ccfcf4d0260489f0e"
  - "crates/corelink-container/src/routes/cas/single_setup.rs@27e4090d053420fa0bfe07ee655d9c6ce6573439"
  - "crates/corelink-container/src/routes/cas/single_handlers.rs@f4259f925d39cff5d70ae6fb8fc5996f2109dba2"
  - "crates/corelink-container/src/routes/cas/batch_write.rs@3561b0e3188b5ff05ff5998fdad3f33ee5f6ad78"
  - "crates/corelink-container/src/routes/cas/batch_read.rs@762ed7b8f4726a6b034e5b2afd1980f3f66c20bf"
  - "crates/corelink-container/src/routes/cas/list_delete.rs@0a5e23ba9d812af9ffe5bd76e1f9a88d91dc12eb"
  - "crates/corelink-container/src/routes/cas/tests_core_part1.rs@f0a08a0cff59bfd6c324f53699d8e8859b18f7ae"
  - "crates/corelink-container/src/routes/cas/tests_core_part2.rs@18e009af00e94f05c6380d608e810ab6e5e74906"
  - "crates/corelink-container/src/routes/cas/tests_batch_part1.rs@a5fd84983e0148c662e05febc14b5797ac0f4a75"
  - "crates/corelink-container/src/routes/cas/tests_batch_part2.rs@44fe15b6dd4eee7f746990f636435ae52ccaae6d"
  - "crates/corelink-container/src/routes/cas/tests_batch_write_part2.rs@b469962afe232ee8b31c4400faa1cc5149b7aee5"
  - "crates/corelink-container/src/routes/cas/tests_batch_cancellation_part3.rs@941c674dba2a5972684450ce03a63618290e4772"
  - "crates/corelink-container/src/routes/cas/tests_read_ceiling.rs@d10335257da5705117f7c985990c91443b08cf8e"
  - "crates/corelink-container/src/routes/cas/tests_edges.rs@0865d5b8294ac197b18095b19dcc474ffd444e8f"
  - "crates/corelink-container/src/storage/r2_s3_parts/cas_core.rs@613fdde5ed6e70e5ba2079903c013e3dc33f27fb"
checkpoint_sha: "a65c7d7caed03adf00acd3a227dc20c4e857f7f0"
provenance: "AUTHORED"
tags: ["surfaces", "cas", "cache", "hot-path"]
timestamp: "2026-06-26T00:00:00Z"

---
# Native CAS surface

The native CAS surface is CoreLink's first-party content-addressable cache: a blob is named by its
own BLAKE3/SHA-256 digest, so a write is idempotent and a read is a pure key lookup. It is the
foundational data plane — the REAPI (Bazel), Turborepo, sccache/cargo and `_public` package surfaces
are all alternate front-doors that ultimately marshal into the same R2-backed blob store this surface
exposes. It sits in the Rust container plane behind the Worker→DO auth hop, so by the time a request
reaches a handler here the tenant is already authenticated; the surface's job is the cheap
canonical-key, scope, possession and quota gates BEFORE any storage I/O. It shares the
digest-validation helper with the [Action Cache surface](/surfaces/action-cache.md).

# Role
It serves the single-object read/write/delete/list contract (`/v1/cas/:tenant/:hash`) plus the bulk
length-framed batch routes (`/batch`, `/batch-read`, `/batch-exists`) that amortize the per-object D1
round-trip on cold-start uploads. Tenant isolation is keyed on the authenticated tenant, never the
path segment.

# How it works
1. The router mounts read/write/delete on `CAS_READ_ROUTE` and adds the three bulk routes as static
   siblings ranked above the `:hash` wildcard by matchit (`crates/corelink-container/src/routes/cas/single_setup.rs:183-220`).
2. A read is a digest-keyed lookup via `handle_read` (`crates/corelink-container/src/routes/cas/single_handlers.rs:8-125`).
3. A write rejects a path/auth tenant mismatch with 403, validates the canonical digest, checks
   write scope and native PAT write capability, then invokes the write handler
   (`crates/corelink-container/src/routes/cas/single_handlers.rs:133-198`).
4. A per-tenant in-flight reservation runs as an extractor BEFORE the body is buffered, returning 429
   over the concurrency cap — the `CasPutGuard` extractor (`crates/corelink-container/src/routes/cas/foundation_state.rs:154-212`). Read paths also use `CasReadConcurrencyGuard` (`crates/corelink-container/src/routes/cas/foundation_state.rs:250-305`); the read slot remains owned by a single-GET or batch-read response stream until its bytes are consumed or dropped.
5. Bulk uploads are split into a newline-framed manifest + concatenated payload at the first blank
   line by `split_manifest` (`crates/corelink-container/src/routes/cas/single_setup.rs:274-294`); a wrong/absent
   content-type is rejected 415 before parse — the `content_type_is` predicate
   (`crates/corelink-container/src/routes/cas/single_setup.rs:230-236`) at the batch-handler call-site
   (`crates/corelink-container/src/routes/cas/batch_write.rs:60-70`).
6. Batch reads consume an ordered bounded window with at most `BATCH_READ_FANOUT` (8) read (`crates/corelink-container/src/routes/cas/foundation_core.rs:207-234`; scheduler at `crates/corelink-container/src/routes/cas/batch_read.rs:189-198,315-338`).
   futures retained at once. The fanout is derived from the 220 MiB read slice: one 22 MiB
   envelope plus eight 24 MiB three-copy object reservations peaks at 214 MiB. Each storage
   read receives the 8 MiB ceiling before body collection; an over-size object or aggregate
   payload returns 413 `batch_too_large`, and explicit terminal paths abort and drain every
   pending task before returning. External cancellation drops the task guard, whose `Drop`
   aborts its owned handles but cannot await them; a synchronous read already inside
   `block_in_place` is not preemptible and only unwinds under the explicit R2 timeout/object
   cap. Every live child shares the request lease, so the tenant slot and global batch envelope
   remain held until that read unwinds; queued children are aborted and terminal paths drain
   their handles.
   Batch request parsing is independently bounded: each line is at most 1 KiB, each retained
   hash is at most 128 bytes, and the object cap is enforced before the next entry is allocated.
   The shared process-wide reservations cover body, parser strings/clones, payload, and (for
   batch-read) the streamed response (`crates/corelink-container/src/routes/cas/batch_read.rs:189-198,271-302,315-338`).
7. After the storage handler returns, the surface records a fire-and-forget usage-metering event into
   the in-process display aggregator [`crate::usage_meter`] — a `ReadHit` on a served read, a `ReadMiss`
   on a genuine `NotFound`, a `Write` on a committed write — with no await / no I/O on the hot path
   (`crates/corelink-container/src/routes/cas/single_handlers.rs:92-123`; `crates/corelink-container/src/routes/cas/single_handlers.rs:92-123`;
   `crates/corelink-container/src/routes/cas/single_handlers.rs:198-212`).
8. Byte-buffering reads reserve a weighted permit from the process-wide
   `GLOBAL_CAS_READ_BUDGET` before storage/body buffering: single-object GETs reserve 196 MiB
   (three 64 MiB copies plus metadata),
   `batch-read` reserves its 22 MiB response envelope and each fan-out object reserves 24 MiB.
   The RAII permits remain held through response assembly and a 250 ms wait timeout fails closed
   with 503 under saturation. R2 GETs use a 2 s connect, 30 s read, 30 s attempt, and 60 s
   operation timeout. After headers, `get_capped` also enforces a 30 s idle and 60 s total body
   deadline while consuming chunks incrementally; a stalled-after-headers stream therefore cannot
   park the synchronous reader, and a misleading content length cannot trigger unbounded
   `ByteStream::collect()` (`crates/corelink-container/src/storage/r2_s3_parts/client_impl.rs`).
   (`crates/corelink-container/src/routes/cas/foundation_core.rs:192-234,300-325`;
   guards at `crates/corelink-container/src/routes/cas/foundation_state.rs:321-350`).

The `#[cfg(test)]` CAS module registers the split test units in `cas.rs` in
source order, including the dedicated `tests_batch_write_part2.rs` unit. The
focused batch census is 28 named tests across the registered parser, batch,
batch-write, and edge units; the B-337 verifier treats both that registration
sequence and those names as load-bearing.

# Invariants
- A non-canonical `:hash` is rejected 400 BEFORE it derives an R2 key (`crates/corelink-container/src/routes/cas/single_handlers.rs:27-34`).
- A path `:tenant` that differs from the authenticated tenant is denied 403 BEFORE storage (`crates/corelink-container/src/routes/cas/single_handlers.rs:145-150`).
- A write requires a `cas:rw` (write-capable) scope; a read-only token is rejected 403 (`crates/corelink-container/src/routes/cas/single_handlers.rs:155-160`).
- A write/batch/delete additionally re-derives the PAT's D1 `can_write` capability at the container (`verify_write`), not just tenant possession — a read-only PAT is rejected 403 even if the Worker-set scope header claimed write (`crates/corelink-container/src/routes/cas/single_handlers.rs:161-166`; helper `crates/corelink-container/src/routes/cas/single_setup.rs:310-328`).
- A batch is capped at `BATCH_MAX_OBJECTS` objects and `BATCH_MAX_BYTES` of payload, over-cap → 413 (`crates/corelink-container/src/routes/cas/foundation_core.rs:117-120`; `crates/corelink-container/src/routes/cas/single_setup.rs:206-216`).
- Per-tenant concurrent uploads are bounded; over the limit returns 429 before buffering (`crates/corelink-container/src/routes/cas/foundation_state.rs:154-212`).
- Usage metering is fire-and-forget DISPLAY telemetry: the `record(...)` call is off the storage-decision path and never gates, bills, or fails a request (`crates/corelink-container/src/routes/cas/single_handlers.rs:92-123` and `:198-212`).
- Batch-read scheduling is bounded by the budget-derived `BATCH_READ_FANOUT = 8` (`crates/corelink-container/src/routes/cas/foundation_core.rs:207-234`); each object
  receives `BATCH_MAX_BYTES` before storage collection, and aggregate overflow is fail-closed as
  413 (`crates/corelink-container/src/routes/cas/batch_read.rs:189-198,271-302,315-338`,
  `crates/corelink-container/src/storage/r2_s3_parts/cas_core.rs:402-411`).
- The process-wide CAS read budget is `CONTAINER_MEMORY_BYTES / 2`, weighted in 1 MiB units;
  Tokio's FIFO semaphore plus the existing per-tenant eight-read pool bounds aggregate bytes and
  prevents one tenant from monopolising the process (`crates/corelink-container/src/routes/cas/foundation_core.rs:192-230`; `crates/corelink-container/src/routes/cas/foundation_state.rs:200-310`).
- A saturated or closed process-wide budget fails closed with 503; the saturation log contains
  only a static route and permit weight, never tenant/hash/request identity (`crates/corelink-container/src/routes/cas/foundation_core.rs:286-320`).
- Batch request bodies are capped at the global 10 MiB limit and parser lines/hashes are bounded before retaining entries; the shared capacity envelope includes those body and clone peaks (`crates/corelink-container/src/routes/cas/foundation_core.rs:130-160`; `crates/corelink-container/src/container_capacity.rs`).
- A single-GET or batch-read `CasReadSlot` remains owned through response consumption/drop, preserving tenant fairness for slow clients (`crates/corelink-container/src/routes/cas/foundation_state.rs:200-310`).
- Per-tenant concurrent uploads are bounded; over the limit returns 429 before buffering (`crates/corelink-container/src/routes/cas/foundation_state.rs:154-212`).
- Usage metering is fire-and-forget DISPLAY telemetry: the `record(...)` call is off the storage-decision path and never gates, bills, or fails a request (`crates/corelink-container/src/routes/cas/single_handlers.rs:92-123` and `:198-212`).
- Batch-read scheduling is bounded by the budget-derived `BATCH_READ_FANOUT = 8` (`crates/corelink-container/src/routes/cas/foundation_core.rs:207-234`); per-object
  storage metadata is checked before body collection and aggregate overflow is fail-closed as 413
  (`crates/corelink-container/src/routes/cas/batch_read.rs:189-198,271-302,315-338`).
- The process-wide CAS read budget is `CONTAINER_MEMORY_BYTES / 2`, weighted in 1 MiB units; Tokio's FIFO semaphore plus the existing per-tenant eight-read pool bounds aggregate bytes and prevents one tenant from monopolising the process (`crates/corelink-container/src/routes/cas/foundation_core.rs:192-230`; `crates/corelink-container/src/routes/cas/foundation_state.rs:200-310`).
- A saturated or closed process-wide budget fails closed with 503; the saturation log contains only a static route and permit weight, never tenant/hash/request identity (`crates/corelink-container/src/routes/cas/foundation_core.rs:286-320`).
- Batch parser lines are at most 1 KiB, retained hashes at most 128 bytes, and object caps are enforced before the next entry is allocated; shared reservations cover body, parser strings/clones, payload, and streamed response (`crates/corelink-container/src/routes/cas/foundation_core.rs:110-145`; parser `crates/corelink-container/src/routes/cas/list_delete.rs:1-27`).

# Gotchas
- The native CAS path proves PAT possession with the native HMAC gate (`pat_gate_reject`) on reads,
  and on the mutating paths escalates to the write-capability variant (`pat_gate_reject_write`, which
  re-derives D1 `can_write`) layered on top of the scope check; a CAS 401 means a bad key OR no D1 row,
  not necessarily a bad password — the full Argon2id possession proof lives on the adapter plane, not here.
- The bulk routes are real route SIBLINGS of the `:hash` wildcard, not captures of it; the framing
  contract (manifest, blank line, payload) is strict and a missing blank-line terminator fails the
  whole request 400.

# Citations
1. `crates/corelink-container/src/routes/cas/single_setup.rs:183-220` — the router: single-object + three bulk batch routes.
2. `crates/corelink-container/src/routes/cas/single_handlers.rs:8-125` — `handle_read`, the digest-keyed read.
3. `crates/corelink-container/src/routes/cas/single_handlers.rs:133-214` — `handle_write`: cross-tenant 403, canonical-digest, scope, native PAT write-capability gate order.
4. `crates/corelink-container/src/routes/cas/single_handlers.rs:145-150` — the cross-tenant 403.
5. `crates/corelink-container/src/routes/cas/single_handlers.rs:151-154` — canonical-digest reject before storage.
6. `crates/corelink-container/src/routes/cas/single_handlers.rs:155-160` — the write-scope (`cas:rw`) gate.
6b. `crates/corelink-container/src/routes/cas/single_handlers.rs:161-166` — the native PAT write-capability gate call (`pat_gate_reject_write`, after scope, before storage).
6c. `crates/corelink-container/src/routes/cas/single_setup.rs:310-328` — `pat_gate_reject_write` helper: re-derives D1 `can_write` via `NativePatGate::verify_write`.
7. `crates/corelink-container/src/routes/cas/foundation_state.rs:154-212` — pre-body per-tenant concurrency 429 (`CasPutGuard`).
8. `crates/corelink-container/src/routes/cas/single_setup.rs:274-294` — `split_manifest` length-framed bulk parser.
9. `crates/corelink-container/src/routes/cas/single_setup.rs:230-236` — `content_type_is` predicate for the 415 gate.
9b. `crates/corelink-container/src/routes/cas/batch_write.rs:60-70` — batch-handler call-site that returns 415 on wrong/absent content-type.
10. `crates/corelink-container/src/routes/cas/foundation_core.rs:110-113` — `BATCH_MAX_OBJECTS` cap.
11. `crates/corelink-container/src/routes/cas/foundation_core.rs:115-122` — `BATCH_MAX_BYTES` cap.
12. `crates/corelink-container/src/routes/cas/batch_write.rs:45-145` — over-cap 413 on bulk write.
13. `crates/corelink-container/src/routes/cas/single_handlers.rs:92-98` — read HIT usage-metering `record` (fire-and-forget display telemetry, no await/I/O).
14. `crates/corelink-container/src/routes/cas/single_handlers.rs:114-123` — read MISS usage-metering `record` (only on a genuine `NotFound`).
15. `crates/corelink-container/src/routes/cas/single_handlers.rs:198-212` — write usage-metering `record` (both fresh 201 and idempotent 200 count as a `Write`).
16. `crates/corelink-container/src/routes/cas/foundation_core.rs:192-234,300-325` — process-wide weighted CAS read budget and bounded fail-closed acquisition.
17. `crates/corelink-container/src/routes/cas/foundation_state.rs:321-350` — RAII guards for the
    196 MiB single GET and 22 MiB batch-read envelope; batch objects use the derived 24 MiB permits.
17. `crates/corelink-container/src/routes/cas.rs:45-51` — declared executable CAS modules.
18. `crates/corelink-container/src/routes/cas/foundation_core.rs:1-23` — foundation imports and shared route constants.
19. `crates/corelink-container/src/routes/cas/foundation_state.rs:1-61` — route state and shared guard state.
20. `crates/corelink-container/src/routes/cas/single_setup.rs:1-18` — single/bulk router setup.
21. `crates/corelink-container/src/routes/cas/batch_read.rs:1-20` — batch-read implementation anchor.
32. `crates/corelink-container/src/routes/cas/list_delete.rs:34-40` — current delete-handler implementation anchor.
22. `crates/corelink-container/src/routes/cas.rs:45-82` — implementation includes and test-module registration.
23. `crates/corelink-container/src/routes/cas/tests_core_part1.rs:146-181` — route constants and brace-syntax controls.
24. `crates/corelink-container/src/routes/cas/tests_core_part2.rs:9-35` — write concurrency guard controls.
25. `crates/corelink-container/src/routes/cas/tests_batch_part1.rs:319-352` — batch-write concurrency guard controls.
26. `crates/corelink-container/src/routes/cas/tests_batch_part2.rs:198-257` — batch-read admission and bounded-window controls.
27. `crates/corelink-container/src/routes/cas/tests_batch_cancellation_part3.rs:6-74` — cancellation and unwind lease controls.
28. `crates/corelink-container/src/routes/cas/tests_batch_write_part2.rs:97-133` — object and byte-cap controls.
29. `crates/corelink-container/src/routes/cas/tests_read_ceiling.rs:1-16` — read-size ceiling controls.
30. `crates/corelink-container/src/routes/cas/tests_edges.rs:3-22` — batch content-type and framing controls.
31. `crates/corelink-container/src/storage/r2_s3_parts/cas_core.rs:402-411` — storage-side object-size bound.
