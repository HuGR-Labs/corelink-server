---
schema: corelink-ownership/1.1
document: reference
package: corelink-r2-multipart
manifest: crates/corelink-r2-multipart/Cargo.toml
source_commit: 6ed297f5b2b64cf97447985111a2ecbbaa9536bb
profile: S
state: author_validated
evidence_set: r2-multipart-static-source-20260920
---

# corelink-r2-multipart — ownership reference

Static-source reference for the multipart abstraction and local fakes. SOURCE
evidence below is distinct from executed/runtime evidence: no real R2/provider
execution, worker wiring, persistence, deployment, or cold review was collected.

[Identity](#r01) · [Boundary](#r02) · [Map](#r03) · [Contracts](#r04) ·
[Invariants](#r05) · [Configuration](#r06) · [Failures](#r07) · [Evidence](#r08).

<a id="r01"></a>
## R01 — Identity

| Field | Static evidence |
|---|---|
| Package / manifest | `corelink-r2-multipart` / `crates/corelink-r2-multipart/Cargo.toml` |
| Role | Rust multipart trait surface with `InMemoryMultipartAdapter` and `AlwaysFailingMultipartAdapter` |
| Source inventory | `adapter`, `bounds`, `concurrency`, `error`, `failover`, `in_memory`, `object_key`, `types`, and crate root |
| Declared first-party dependency | `corelink-tenant-path` |
| Evidence mode | SOURCE only; no Cargo/build/test or provider/runtime result is asserted |

<a id="r02"></a>
## R02 — Boundary and authority

This package owns its Rust trait, local value types, validation/composition code,
and fake implementations. Its manifest and crate documentation describe a real
`aws-sdk-s3` Cloudflare R2 shim as deferred; no such implementation appears in
the inspected package source. `InMemoryMultipartAdapter` is source-defined test
or host-side fake behavior, not evidence that an external provider accepts,
persists, completes, lists, or aborts uploads. D1 session storage, worker route
mounting, R2 credentials, production sweeper behavior, and deployment remain
outside this ownership boundary.

<a id="r03"></a>
## R03 — Implementation map

| Source | Source-visible responsibility | Classification |
|---|---|---|
| `adapter.rs` | Defines `MultipartAdapter`, `InitiateRequest`, and five async-shaped method signatures | Public trait contract |
| `types.rs` | Defines bucket, handle, result, orphan, ETag, part, and session-state values | Public representation boundary |
| `object_key.rs` | Validates components and composes canonical object-key text | Local key construction |
| `bounds.rs` / `concurrency.rs` | Names parser/size/permit constants and per-tenant semaphore machinery | Local bound/concurrency source |
| `in_memory.rs` / `error.rs` | Supplies fake lifecycle behavior and typed error surface | Local fake/error source |
| `failover.rs` | Supplies failover inventory and audit abstraction types | Local source seam, not an operated failover system |

<a id="r04"></a>
## R04 — Public contracts

`MultipartAdapter` requires `Send + Sync` and exposes `initiate`, `upload_part`,
`complete`, `abort`, and `list_orphans`. `InitiateRequest` carries tenant ID,
typed tenant prefix, bucket, region, digest, and optional suffix. The later
lifecycle methods take an explicit tenant ID and `MultipartUpload` handle;
completion takes caller-provided `Vec<(PartNumber, PartETag)>`. The fake rejects
duplicate part numbers, but the inspected source has no monotonic-order
validation of that vector. These are source API facts only. They do not establish
an HTTP/R2 protocol, SDK implementation, durability, worker caller, or release.

<a id="r05"></a>
## R05 — Falsifiable invariants

| Identifier | Atomic predicate | Static falsifier / evidence |
|---|---|---|
| <a id="inv-001"></a>INV-001 | `MultipartAdapter` declares exactly the five named lifecycle methods in R04 | Removing/renaming a method or changing its source signature falsifies it; `src/adapter.rs` |
| <a id="inv-002"></a>INV-002 | `MultipartUpload` has public `upload_id`, `tenant_id`, `bucket`, `object_key`, and `initiated_at` fields | Removing/renaming a listed field falsifies it; `src/types.rs` |
| <a id="inv-003"></a>INV-003 | `object_key::compose` takes `Bucket`, `&str` region, `&TenantPrefix`, `&str` digest, and optional suffix, returning `Result<String, MultipartError>` | A changed parameter or result shape falsifies this signature predicate; `src/object_key.rs` |
| <a id="inv-004"></a>INV-004 | `PartNumber::new` rejects zero and values above `bounds::R2_MAX_PARTS_PER_UPLOAD` | Changed guard/bound reference falsifies it; `src/types.rs` |
| <a id="inv-005"></a>INV-005 | `Bucket::Chunk` and `Bucket::Manifest` select the two family-prefix constants | Added/removed mapping or changed constant selection falsifies it; `src/types.rs`, `src/bounds.rs` |
| <a id="inv-006"></a>INV-006 | `PerTenantSemaphore` is a local concurrency source whose acquisition API names tenant scope | Removing tenant-keyed acquire/try-acquire source falsifies it; `src/concurrency.rs` |
| <a id="inv-007"></a>INV-007 | `InMemoryMultipartAdapter` and `AlwaysFailingMultipartAdapter` are publicly re-exported by the crate root | Removing either re-export falsifies it; `src/lib.rs` |
| <a id="inv-008"></a>INV-008 | The crate root contains `#![forbid(unsafe_code)]` | Removing or weakening that attribute falsifies it; `src/lib.rs`; it says nothing about dependencies |
| <a id="inv-009"></a>INV-009 | `InMemoryMultipartAdapter::initiate` invokes `object_key::compose` before it creates/returns a session handle | Removing or replacing that call in this fake initiation path falsifies the predicate; it does not constrain another implementor; `src/in_memory.rs` |

<a id="r06"></a>
## R06 — Configuration and feature boundary

The manifest declares `tokio` with `default-features = false` and `sync`, plus
`corelink-tenant-path`, `bytes`, `hex`, `sha2`, `thiserror`, and `uuid` runtime
dependencies. It names property and chaos test targets, but this assessment did
not resolve features, build targets, dependency versions, or execute those tests.
The source uses `tokio::sync` semaphore types; it does not itself evidence
fairness, blocking characteristics in a deployed workload, or a provider call.

<a id="r07"></a>
## R07 — Failure and compatibility boundary

The source exposes `MultipartError` and local fake paths for invalid keys,
tenant mismatch, part constraints, missing sessions, concurrency, and backend
categories. Exact fake return behavior remains a source contract subject to
separate consumer compatibility assessment. No source-only reading proves an R2
error mapping, retries, S3 compatibility, pagination, atomic storage commit,
or a recovery procedure. A future `MultipartAdapter` provider implementation
must be assessed as new implementation work rather than represented as existing
contract execution.

<a id="r08"></a>
## R08 — Evidence and explicit unknowns

Evidence set: this manifest and package Rust sources, plus targeted static
manifest/import/re-export references used in the impact map. SOURCE evidence
does not include executed/runtime evidence. Unknowns include resolved Cargo
features/targets, complete consumer graph, real R2/provider implementation and
conformance, SDK behavior, worker wiring, D1 persistence, object durability,
orphan sweep operation, performance/fairness, credentials, deployment, and
cold-review status. WAVE_007_PLAN and canonical OKF are route/reference inputs
only; this document neither copies, redefines, nor revalidates them.

[Impact map](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#r01)
