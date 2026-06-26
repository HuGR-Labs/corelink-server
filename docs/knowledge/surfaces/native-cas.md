---
type: "CacheSurface"
title: "Native CAS surface"
description: "CoreLink's first-party content-addressable storage surface — the GET/PUT/DELETE/list + bulk-batch CAS routes every other surface ultimately stores into."
source_files:
  - "crates/corelink-container/src/routes/cas.rs"
checkpoint_sha: "41d84e271568cb47df664806fa3dc9798c134249"
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
   siblings ranked above the `:hash` wildcard by matchit (`crates/corelink-container/src/routes/cas.rs:515-531`).
2. A read is a digest-keyed lookup via `handle_read` (`crates/corelink-container/src/routes/cas.rs:625`).
3. A write rejects a path/auth tenant mismatch with 403, then validates the digest, then the write
   scope, then the native PAT possession gate, all before storage (`crates/corelink-container/src/routes/cas.rs:713-747`).
4. A per-tenant in-flight reservation runs as an extractor BEFORE the body is buffered, returning 429
   over the concurrency cap (`crates/corelink-container/src/routes/cas.rs:285-289`).
5. Bulk uploads are split into a newline-framed manifest + concatenated payload at the first blank
   line by `split_manifest` (`crates/corelink-container/src/routes/cas.rs:582-595`); a wrong/absent
   content-type is rejected 415 before parse — the `content_type_is` predicate
   (`crates/corelink-container/src/routes/cas.rs:538-545`) at the batch-handler call-site
   (`crates/corelink-container/src/routes/cas.rs:847-848`).

# Invariants
- A non-canonical `:hash` is rejected 400 BEFORE it derives an R2 key (`crates/corelink-container/src/routes/cas.rs:732`).
- A path `:tenant` that differs from the authenticated tenant is denied 403 BEFORE storage (`crates/corelink-container/src/routes/cas.rs:728`).
- A write requires a `cas:rw` (write-capable) scope; a read-only token is rejected 403 (`crates/corelink-container/src/routes/cas.rs:738-739`).
- A batch is capped at `BATCH_MAX_OBJECTS` objects and `BATCH_MAX_BYTES` of payload, over-cap → 413 (`crates/corelink-container/src/routes/cas.rs:115`; `crates/corelink-container/src/routes/cas.rs:121`; `crates/corelink-container/src/routes/cas.rs:881`).
- Per-tenant concurrent uploads are bounded; over the limit returns 429 before buffering (`crates/corelink-container/src/routes/cas.rs:285-289`).

# Gotchas
- The native CAS path proves PAT possession with the native HMAC gate (`pat_gate_reject`) layered on
  top of the scope check; a CAS 401 means a bad key OR no D1 row, not necessarily a bad password — the
  full Argon2id possession proof lives on the adapter plane, not here.
- The bulk routes are real route SIBLINGS of the `:hash` wildcard, not captures of it; the framing
  contract (manifest, blank line, payload) is strict and a missing blank-line terminator fails the
  whole request 400.

# Citations
1. `crates/corelink-container/src/routes/cas.rs:515-531` — the router: single-object + three bulk batch routes.
2. `crates/corelink-container/src/routes/cas.rs:625` — `handle_read`, the digest-keyed read.
3. `crates/corelink-container/src/routes/cas.rs:713-747` — `handle_write`: cross-tenant 403, canonical-digest, scope, native PAT gate order.
4. `crates/corelink-container/src/routes/cas.rs:728` — the cross-tenant 403.
5. `crates/corelink-container/src/routes/cas.rs:732` — canonical-digest reject before storage.
6. `crates/corelink-container/src/routes/cas.rs:738-739` — the write-scope (`cas:rw`) gate.
6b. `crates/corelink-container/src/routes/cas.rs:741-743` — the native PAT possession gate (after scope, before storage).
7. `crates/corelink-container/src/routes/cas.rs:285-289` — pre-body per-tenant concurrency 429.
8. `crates/corelink-container/src/routes/cas.rs:582-595` — `split_manifest` length-framed bulk parser.
9. `crates/corelink-container/src/routes/cas.rs:538-545` — `content_type_is` predicate for the 415 gate.
9b. `crates/corelink-container/src/routes/cas.rs:847-848` — batch-handler call-site that returns 415 on wrong/absent content-type.
10. `crates/corelink-container/src/routes/cas.rs:115` — `BATCH_MAX_OBJECTS` cap.
11. `crates/corelink-container/src/routes/cas.rs:121` — `BATCH_MAX_BYTES` cap.
12. `crates/corelink-container/src/routes/cas.rs:881` — over-cap 413 on bulk write.
