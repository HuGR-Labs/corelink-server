---
type: "CacheSurface"
title: "Native CAS surface"
description: "CoreLink's first-party content-addressable storage surface — the GET/PUT/DELETE/list + bulk-batch CAS routes every other surface ultimately stores into."
source_files:
  - "crates/corelink-container/src/routes/cas/foundation_core.rs"
  - "crates/corelink-container/src/routes/cas/foundation_state.rs"
  - "crates/corelink-container/src/routes/cas/single_setup.rs"
  - "crates/corelink-container/src/routes/cas/single_handlers.rs"
  - "crates/corelink-container/src/routes/cas/batch_write.rs"
  - "crates/corelink-container/src/routes/cas/batch_read.rs"
  - "crates/corelink-container/src/routes/cas/list_delete.rs"
  - "crates/corelink-container/src/routes/cas/tests_edges.rs"
  - "crates/corelink-container/src/routes/cas/tests_batch_part2.rs"
source_blobs:
  - "crates/corelink-container/src/routes/cas/foundation_core.rs@0dba9d2c8044223ec110cf8c852730a87e245885"
  - "crates/corelink-container/src/routes/cas/batch_read.rs@0dba9d2c8044223ec110cf8c852730a87e245885"
  - "crates/corelink-container/src/storage/r2_s3_parts/cas_core.rs@0dba9d2c8044223ec110cf8c852730a87e245885"
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
   siblings ranked above the `:hash` wildcard by matchit (`crates/corelink-container/src/routes/cas/single_setup.rs:152-187`).
2. A read is a digest-keyed lookup via `handle_read` (`crates/corelink-container/src/routes/cas/single_handlers.rs:8-125`).
3. A write rejects a path/auth tenant mismatch with 403, then validates the digest, then the write
   scope, then the native PAT **write-capability** gate, all before storage — `handle_write` (`crates/corelink-container/src/routes/cas/single_handlers.rs:133-214`). The write/batch/delete paths run `pat_gate_reject_write`, which re-derives the PAT's D1-stored `can_write` at the container via `NativePatGate::verify_write` (not just tenant possession) — a read-only PAT is rejected 403 even if the Worker-set scope header claimed write, upholding the Option-B invariant that a compromised Worker cannot grant write on its own (`crates/corelink-container/src/routes/cas/single_handlers.rs:133-214`; helper at `crates/corelink-container/src/routes/cas/single_setup.rs:275-294`).
   scope, then the native PAT **write-capability** gate, all before storage — `handle_write` (`crates/corelink-container/src/routes/cas/single_handlers.rs:133-214`). The write/batch/delete paths run `pat_gate_reject_write`, which re-derives the PAT's D1-stored `can_write` at the container via `NativePatGate::verify_write` (not just tenant possession) — a read-only PAT is rejected 403 even if the Worker-set scope header claimed write, upholding the Option-B invariant that a compromised Worker cannot grant write on its own (`crates/corelink-container/src/routes/cas/single_handlers.rs:133-214`; helper at `crates/corelink-container/src/routes/cas/single_setup.rs:275-294`).
4. A per-tenant in-flight reservation runs as an extractor BEFORE the body is buffered, returning 429
   over the concurrency cap — the `CasPutGuard` extractor (`crates/corelink-container/src/routes/cas/foundation_state.rs:144-204`). Read paths also use `CasReadConcurrencyGuard` (`crates/corelink-container/src/routes/cas/foundation_state.rs:240-303`); the read slot remains owned by a single-GET or batch-read response stream until its bytes are consumed or dropped.
5. Bulk uploads are split into a newline-framed manifest + concatenated payload at the first blank
   line by `split_manifest` (`crates/corelink-container/src/routes/cas/single_setup.rs:235-252`); a wrong/absent
   content-type is rejected 415 before parse — the `content_type_is` predicate
   (`crates/corelink-container/src/routes/cas/single_setup.rs:190-203`) at the batch-handler call-site
   (`crates/corelink-container/src/routes/cas/single_setup.rs:169-185`).
6. Batch reads consume an ordered bounded window with at most `BATCH_READ_FANOUT` (8) read
   futures retained at once. The fanout is derived from the 220 MiB read slice: one 22 MiB
   envelope plus eight 24 MiB three-copy object reservations peaks at 214 MiB. Each storage
   read receives the 8 MiB ceiling before body collection; an over-size object or aggregate
   payload returns 413 `batch_too_large`, and terminal errors abort and drain every pending
   task before returning. Batch request parsing is independently bounded: each line is at most
   1 KiB, each retained hash is at most 128 bytes, and the object cap is enforced before the
   next entry is allocated. The shared process-wide reservations cover body, parser
   strings/clones, payload, and (for batch-read) the streamed response
   (`crates/corelink-container/src/routes/cas/batch_read.rs:80-290`).
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
   with 503 under saturation (`crates/corelink-container/src/routes/cas/foundation_core.rs:190-330`;
   guards at `crates/corelink-container/src/routes/cas/foundation_state.rs:319-350`).

# Invariants
- A non-canonical `:hash` is rejected 400 BEFORE it derives an R2 key (`crates/corelink-container/src/routes/cas/single_handlers.rs:27-34`).
- A path `:tenant` that differs from the authenticated tenant is denied 403 BEFORE storage (`crates/corelink-container/src/routes/cas/single_handlers.rs:23-29`).
- A write requires a `cas:rw` (write-capable) scope; a read-only token is rejected 403 (`crates/corelink-container/src/routes/cas/single_handlers.rs:155-160`).
- A write/batch/delete additionally re-derives the PAT's D1 `can_write` capability at the container (`verify_write`), not just tenant possession — a read-only PAT is rejected 403 even if the Worker-set scope header claimed write (`crates/corelink-container/src/routes/cas/single_handlers.rs:161-166`; helper `crates/corelink-container/src/routes/cas/single_setup.rs:275-294`).
- A batch is capped at `BATCH_MAX_OBJECTS` objects and `BATCH_MAX_BYTES` of payload, over-cap → 413 (`crates/corelink-container/src/routes/cas/foundation_core.rs:117-120`; `crates/corelink-container/src/routes/cas/single_setup.rs:206-216`).
- Per-tenant concurrent uploads are bounded; over the limit returns 429 before buffering (`crates/corelink-container/src/routes/cas/foundation_state.rs:144-204`).
- Usage metering is fire-and-forget DISPLAY telemetry: the `record(...)` call is off the storage-decision path and never gates, bills, or fails a request (`crates/corelink-container/src/routes/cas/single_handlers.rs:92-123` and `:198-212`).
- Batch-read scheduling is bounded by the budget-derived `BATCH_READ_FANOUT = 8`; each object
  receives `BATCH_MAX_BYTES` before storage collection, and aggregate overflow is fail-closed as
  413 (`crates/corelink-container/src/routes/cas/batch_read.rs:80-290`,
  `crates/corelink-container/src/storage/r2_s3_parts/cas_core.rs:402-435`).
- The process-wide CAS read budget is `CONTAINER_MEMORY_BYTES / 2`, weighted in 1 MiB units;
  Tokio's FIFO semaphore plus the existing per-tenant eight-read pool bounds aggregate bytes and
  prevents one tenant from monopolising the process (`crates/corelink-container/src/routes/cas/foundation_core.rs:192-230`; `crates/corelink-container/src/routes/cas/foundation_state.rs:200-310`).
- A saturated or closed process-wide budget fails closed with 503; the saturation log contains
  only a static route and permit weight, never tenant/hash/request identity (`crates/corelink-container/src/routes/cas/foundation_core.rs:286-320`).
- Batch request bodies are capped at the global 10 MiB limit and parser lines/hashes are bounded before retaining entries; the shared capacity envelope includes those body and clone peaks (`crates/corelink-container/src/routes/cas/foundation_core.rs:130-160`; `crates/corelink-container/src/container_capacity.rs`).
- A single-GET or batch-read `CasReadSlot` remains owned through response consumption/drop, preserving tenant fairness for slow clients (`crates/corelink-container/src/routes/cas/foundation_state.rs:200-310`).
- Per-tenant concurrent uploads are bounded; over the limit returns 429 before buffering (`crates/corelink-container/src/routes/cas/foundation_state.rs:144-204`).
- Usage metering is fire-and-forget DISPLAY telemetry: the `record(...)` call is off the storage-decision path and never gates, bills, or fails a request (`crates/corelink-container/src/routes/cas/single_handlers.rs:92-123` and `:198-212`).
- Batch-read scheduling is bounded by the budget-derived `BATCH_READ_FANOUT = 8`; per-object
  storage metadata is checked before body collection and aggregate overflow is fail-closed as 413
  (`crates/corelink-container/src/routes/cas/batch_read.rs:80-290`).
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
1. `crates/corelink-container/src/routes/cas/single_setup.rs:152-187` — the router: single-object + three bulk batch routes.
2. `crates/corelink-container/src/routes/cas/single_handlers.rs:8-125` — `handle_read`, the digest-keyed read.
3. `crates/corelink-container/src/routes/cas/single_handlers.rs:133-214` — `handle_write`: cross-tenant 403, canonical-digest, scope, native PAT write-capability gate order.
4. `crates/corelink-container/src/routes/cas/single_handlers.rs:23-29` — the cross-tenant 403.
5. `crates/corelink-container/src/routes/cas/single_handlers.rs:30-34` — canonical-digest reject before storage.
6. `crates/corelink-container/src/routes/cas/single_handlers.rs:155-160` — the write-scope (`cas:rw`) gate.
6b. `crates/corelink-container/src/routes/cas/single_handlers.rs:161-166` — the native PAT write-capability gate call (`pat_gate_reject_write`, after scope, before storage).
6c. `crates/corelink-container/src/routes/cas/single_setup.rs:275-294` — `pat_gate_reject_write` helper: re-derives D1 `can_write` via `NativePatGate::verify_write`.
7. `crates/corelink-container/src/routes/cas/foundation_state.rs:144-204` — pre-body per-tenant concurrency 429 (`CasPutGuard`).
8. `crates/corelink-container/src/routes/cas/single_setup.rs:235-252` — `split_manifest` length-framed bulk parser.
9. `crates/corelink-container/src/routes/cas/single_setup.rs:190-203` — `content_type_is` predicate for the 415 gate.
9b. `crates/corelink-container/src/routes/cas/single_setup.rs:169-185` — batch-handler call-site that returns 415 on wrong/absent content-type.
10. `crates/corelink-container/src/routes/cas/foundation_core.rs:110-113` — `BATCH_MAX_OBJECTS` cap.
11. `crates/corelink-container/src/routes/cas/foundation_core.rs:115-122` — `BATCH_MAX_BYTES` cap.
12. `crates/corelink-container/src/routes/cas/batch_write.rs:45-145` — over-cap 413 on bulk write.
13. `crates/corelink-container/src/routes/cas/single_handlers.rs:92-98` — read HIT usage-metering `record` (fire-and-forget display telemetry, no await/I/O).
14. `crates/corelink-container/src/routes/cas/single_handlers.rs:114-123` — read MISS usage-metering `record` (only on a genuine `NotFound`).
15. `crates/corelink-container/src/routes/cas/single_handlers.rs:198-212` — write usage-metering `record` (both fresh 201 and idempotent 200 count as a `Write`).
16. `crates/corelink-container/src/routes/cas/foundation_core.rs:190-330` — process-wide weighted CAS read budget and bounded fail-closed acquisition.
17. `crates/corelink-container/src/routes/cas/foundation_state.rs:319-350` — RAII guards for the
    196 MiB single GET and 22 MiB batch-read envelope; batch objects use the derived 24 MiB permits.
17. `crates/corelink-container/src/routes/cas.rs:34-47` — declared executable CAS modules.
18. `crates/corelink-container/src/routes/cas/foundation_core.rs:1-23` — foundation imports and shared route constants.
19. `crates/corelink-container/src/routes/cas/foundation_state.rs:1-61` — route state and shared guard state.
20. `crates/corelink-container/src/routes/cas/single_setup.rs:1-18` — single/bulk router setup.
21. `crates/corelink-container/src/routes/cas/batch_read.rs:1-20` — batch-read implementation anchor.
1. `crates/corelink-container/src/routes/cas/list_delete.rs:34` — current implementation anchor.
