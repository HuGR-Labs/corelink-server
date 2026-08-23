---
type: "StorageComponent"
title: "Chunk / manifest multipart buckets"
description: "STATUS: designed-not-wired. How large objects are INTENDED to be stored across the chunk and manifest R2 bucket families via the multipart adapter, with structurally tenant-scoped, content-addressed object keys — the object-key composer + invariants below are real and tested, but `corelink-r2-multipart` ships only `InMemoryMultipartAdapter` / `AlwaysFailingMultipartAdapter` (no real R2 impl exists), and `corelink-container` has zero multipart write-sites (`crates/corelink-container/src/routes/dsr/adapter_r2_cas.rs:102-107` states this explicitly). No production object is stored this way today; large objects go through the single-PUT CAS path."
source_files:
  - "crates/corelink-r2-multipart/src/lib.rs"
  - "crates/corelink-r2-multipart/src/object_key.rs"
  - "crates/corelink-r2-multipart/src/in_memory.rs"
  - "crates/corelink-container/src/routes/dsr/adapter_r2_cas.rs"
checkpoint_sha: "8af9ed65caf286d3f800e91d3f823face3aefd31"
provenance: "AUTHORED"
tags: ["storage", "r2", "multipart", "chunk", "manifest", "tenant-isolation"]
timestamp: "2026-06-26T00:00:00Z"
---

# Chunk / manifest multipart buckets

STATUS — designed-not-wired. This concept describes a storage shape that is **not in production use**:
today, large objects go through the single-PUT CAS path, not chunk+manifest multipart. The
`corelink-r2-multipart` crate DESIGNS that storage shape: two bucket families, `chunk` and `manifest`,
each addressed by a canonical key `<family>-<region>/<tenant_prefix>/<digest_hex>`. The object-key
composer and its tenant-scoping invariants are real, tested Rust — but the crate ships only two
adapters, `InMemoryMultipartAdapter` and `AlwaysFailingMultipartAdapter`
(`crates/corelink-r2-multipart/src/in_memory.rs:206,528`); `grep -rn "impl MultipartAdapter"` across the
workspace finds no real R2/`aws-sdk-s3` implementation. `corelink-container` — the only place that could
call it in production — has **zero multipart write-sites**: a GDPR-erasure code comment states this
explicitly ("no `corelink-r2-multipart` dep; `R2_CHUNK_BUCKET` is never written",
`crates/corelink-container/src/routes/dsr/adapter_r2_cas.rs:102-107`). Treat the object-key composer and
its invariants as a tested-but-dormant design artifact, not evidence that chunk/manifest storage is how
CoreLink stores large objects today. It shares the per-tenant-prefix isolation discipline of the
[R2 CAS bucket](/storage/r2-cas-bucket.md), which IS the live large-object path.

# Role
- The canonical multipart adapter trait + the structurally tenant-scoped object-key constructor
  (`crates/corelink-r2-multipart/src/lib.rs:1-39`).
- The composer that sandwiches the tenant prefix between the bucket family and the content digest
  (`crates/corelink-r2-multipart/src/object_key.rs:1-22`).

# How it works
1. Every multipart object uses the canonical key `<family>-<region>/<tenant_prefix>/<digest_hex>[.suffix]`,
   where `<family>` is `chunk` or `manifest` (`.json` suffix for manifest envelopes)
   (`crates/corelink-r2-multipart/src/object_key.rs:1-22`).
2. `compose` takes typed inputs only and validates the region, the digest, and the optional suffix before
   constructing the key string (`crates/corelink-r2-multipart/src/object_key.rs:59-93`).
3. The `MultipartAdapter` trait is the intended production integration seam; today the ONLY
   implementations are `InMemoryMultipartAdapter` and `AlwaysFailingMultipartAdapter`
   (`crates/corelink-r2-multipart/src/in_memory.rs:206,528`) — there is no real R2 shim
   (`crates/corelink-r2-multipart/src/lib.rs:1-39`).
4. The lifecycle (initiate / upload-part / complete / abort) is idempotent: re-initiating an in-progress
   `(tenant_id, object_key)` returns the existing upload id and re-completing returns the cached object
   (`crates/corelink-r2-multipart/src/lib.rs:14-19`).
5. Per-tenant upload concurrency is bounded by a semaphore, and part numbers are a bounded `1..=10_000`
   per the R2 hard limit (`crates/corelink-r2-multipart/src/lib.rs:26-39`).

# Invariants
- The tenant-prefix segment is structurally sandwiched between the family and the digest, so no
  caller-controlled string can move it or inject `..` — `INV-MULTIPART-PATH-TENANT-SCOPED`
  (`crates/corelink-r2-multipart/src/object_key.rs:24-32`,
  `crates/corelink-r2-multipart/src/object_key.rs:54-58`).
- The region literal must match `[a-z0-9_-]{1,16}` or `compose` returns `InvalidObjectKey`
  (`crates/corelink-r2-multipart/src/object_key.rs:95-110`).
- The content digest must be exactly 64-char lower-case hex or `compose` rejects it
  (`crates/corelink-r2-multipart/src/object_key.rs:112-127`).
- An `upload_id` is bound to its `tenant_id` at initiate; every later call with a disagreeing tenant is
  rejected with `CrossTenantUpload` and the session is left untouched
  (`crates/corelink-r2-multipart/src/lib.rs:20-25`).
- The public types carry `#[non_exhaustive]` so additive variants/fields don't break downstream callers
  post-v1 (`crates/corelink-r2-multipart/src/lib.rs:62-70`).

# Gotchas
- **⚠️ Designed, not wired — gated-inert.** `corelink-r2-multipart` has no real R2 adapter
  (`grep -rn "impl MultipartAdapter"` finds only `InMemoryMultipartAdapter` and
  `AlwaysFailingMultipartAdapter` in `crates/corelink-r2-multipart/src/in_memory.rs`), and
  `corelink-container` never references it — the GDPR-erasure code says so directly ("no
  `corelink-r2-multipart` dep; `R2_CHUNK_BUCKET` is never written",
  `crates/corelink-container/src/routes/dsr/adapter_r2_cas.rs:102-107`). Do not cite this concept as
  evidence CoreLink chunks large objects in production today — it doesn't.
- **Known consequence if this ships un-audited:** the same erasure code comment flags that turning on
  multipart writes without first extending the erasure sweep to the `corelink-chunk-*`/`corelink-manifest-*`
  buckets would let chunked content survive a "complete" erasure while the Ed25519 attestation falsely
  signs `Complete` — gate any future multipart-enabling PR on that erasure-sweep extension.
- The `<region>` literal here is the worker's `Region::bucket_suffix()` output passed in as a plain
  string — the crate stays wasm32-clean and intentionally does NOT type the region against the worker's
  `Region`, to avoid the dep cycle.
- Resumable mid-stream restart, multi-region replication of sessions, and customer-tunable part size are
  explicit anti-scope at GA — do not assume the adapter offers them, even once a real adapter exists.

# Citations
1. `crates/corelink-r2-multipart/src/lib.rs:1-39` — `MultipartAdapter` trait + tenant-scoped keys, idempotent lifecycle, cross-tenant rejection, bounded concurrency.
2. `crates/corelink-r2-multipart/src/lib.rs:14-19` — idempotent init/upload/complete/abort semantics.
3. `crates/corelink-r2-multipart/src/lib.rs:20-25` — `upload_id ↔ tenant_id` binding + `CrossTenantUpload` rejection.
4. `crates/corelink-r2-multipart/src/lib.rs:26-39` — bounded per-tenant concurrency + `1..=10_000` part-number bound.
5. `crates/corelink-r2-multipart/src/lib.rs:62-70` — `#[non_exhaustive]` public-type stability.
6. `crates/corelink-r2-multipart/src/object_key.rs:1-22` — canonical key `<family>-<region>/<tenant_prefix>/<digest_hex>[.suffix]`.
7. `crates/corelink-r2-multipart/src/object_key.rs:24-32` — structural tenant scoping (typed inputs, no traversal).
8. `crates/corelink-r2-multipart/src/object_key.rs:54-58` — `INV-MULTIPART-PATH-TENANT-SCOPED` captured by construction.
9. `crates/corelink-r2-multipart/src/object_key.rs:59-93` — `compose` validate-then-build.
10. `crates/corelink-r2-multipart/src/object_key.rs:95-110` — region-literal alphabet validation.
11. `crates/corelink-r2-multipart/src/object_key.rs:112-127` — 64-char lower-hex digest validation.
12. `crates/corelink-r2-multipart/src/in_memory.rs:206` — `impl MultipartAdapter for InMemoryMultipartAdapter` (test/dev fake).
13. `crates/corelink-r2-multipart/src/in_memory.rs:528` — `impl MultipartAdapter for AlwaysFailingMultipartAdapter` (chaos fake); together these are the ONLY two `MultipartAdapter` impls in the workspace — no real R2 adapter exists.
14. `crates/corelink-container/src/routes/dsr/adapter_r2_cas.rs:102-107` — explicit code comment: the container has zero multipart write-sites, no `corelink-r2-multipart` dependency, `R2_CHUNK_BUCKET` is never written.
