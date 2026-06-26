---
type: "StorageComponent"
title: "Chunk / manifest multipart buckets"
description: "How large objects are stored across the chunk and manifest R2 bucket families via the multipart adapter, with structurally tenant-scoped, content-addressed object keys."
source_files:
  - "crates/corelink-r2-multipart/src/lib.rs"
  - "crates/corelink-r2-multipart/src/object_key.rs"
checkpoint_sha: "5571b910292cbe3d53cbf46d7e0f120dbef877e2"
provenance: "AUTHORED"
tags: ["storage", "r2", "multipart", "chunk", "manifest", "tenant-isolation"]
timestamp: "2026-06-26T00:00:00Z"
---

# Chunk / manifest multipart buckets

Large objects do not land in a single CAS PUT — they are chunked, and a manifest stitches the chunks
back together. The `corelink-r2-multipart` crate owns that storage shape: two bucket families, `chunk`
and `manifest`, each addressed by a canonical key `<family>-<region>/<tenant_prefix>/<digest_hex>`. The
crate is the integration boundary for R2 multipart (the in-memory fake is the reference impl the
production `aws-sdk-s3` shim must observably match), and its defining property is that tenant scoping is
structural: the object-key composer takes typed inputs only, so no client-controlled string can move the
tenant prefix or inject path traversal. It shares the per-tenant-prefix isolation discipline of the
[R2 CAS bucket](/storage/r2-cas-bucket.md).

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
3. The `MultipartAdapter` trait is the production integration seam; the in-memory fake is the reference
   impl the real R2 shim must match (`crates/corelink-r2-multipart/src/lib.rs:1-39`).
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
- The `<region>` literal here is the worker's `Region::bucket_suffix()` output passed in as a plain
  string — the crate stays wasm32-clean and intentionally does NOT type the region against the worker's
  `Region`, to avoid the dep cycle.
- Resumable mid-stream restart, multi-region replication of sessions, and customer-tunable part size are
  explicit anti-scope at GA — do not assume the adapter offers them.

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
