---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-r2-multipart
manifest: crates/corelink-r2-multipart/Cargo.toml
source_commit: 6ed297f5b2b64cf97447985111a2ecbbaa9536bb
profile: S
state: author_validated
evidence_set: r2-multipart-static-source-20260920
---

# corelink-r2-multipart — blast radius

Static relation map for a multipart trait and local fake package. Arrows state
only manifest declarations, imports, re-exports, or source structure. They do
not establish R2 execution, actual writes, provider compatibility, worker
mounting, persistence, runtime ordering, deployment, or operational ownership.

[Scope](#b01) · [Outbound](#b02) · [Key and type flow](#b03) ·
[Trait/fake flow](#b04) · [Observed adjacent paths](#b05) · [Closure](#b06).

[REL-001](#rel-001) · [REL-002](#rel-002) · [REL-003](#rel-003) ·
[REL-004](#rel-004) · [REL-005](#rel-005) · [REL-006](#rel-006) ·
[REL-007](#rel-007) · [REL-008](#rel-008).

<a id="b01"></a>
## B01 — Scope and reading rule

The package owns source-defined multipart abstractions, not an externally
operated object-storage system. Each relation separates a source-visible edge
from an inferred behavior. An edge can signal where an API change deserves
review, but cannot demonstrate that either side is compiled together.

It also cannot show Cargo selection, runtime reachability, or Cloudflare R2
integration. The recorded evidence mode is SOURCE, not executed/runtime.

<a id="rel-001"></a>
### REL-001 — Manifest dependency boundary

**Dependency:** `corelink-r2-multipart` → `corelink-tenant-path`, `bytes`,
`hex`, `sha2`, `thiserror`, `tokio`, and `uuid`.
**Flow:** the manifest declares these names as dependencies; package source
imports selected types and helpers from them.
**Impact:** changing a named package/version/features declaration can affect
source compilation or the public types exposed by this crate.
**Evidence:** `crates/corelink-r2-multipart/Cargo.toml`.
**Stop:** the manifest does not prove resolved versions, selected features,
target support, transitive dependency behavior, or runtime use.

[Scope](#b01) [Relation index](#b03)

<a id="b02"></a>
## B02 — Outbound dependency relations

<a id="rel-002"></a>
### REL-002 — Tenant-prefix input relation

**Dependency:** `corelink_tenant_path::TenantPrefix` →
`corelink_r2_multipart::InitiateRequest` and `object_key::compose`.
**Flow:** initiation request and canonical key construction use a borrowed typed
tenant prefix rather than a raw tenant-prefix string at these source boundaries.
**Impact:** a changed `TenantPrefix` path or constructor contract can require
multipart source compatibility assessment.
**Evidence:** `src/adapter.rs`; `src/object_key.rs`; package manifest.
**Stop:** this does not prove prefix materialization, caller identity, tenant
authorization, or a stored/R2 object key.

[Outbound](#b02) [Relation index](#b03)

<a id="b03"></a>
## B03 — Key and type flow relations

<a id="rel-003"></a>
### REL-003 — Canonical key composition relation

**Dependency:** `Bucket`, region literal, `TenantPrefix`, digest text, and
optional suffix → `object_key::compose` → `String` object key.
**Flow:** source validates inputs before returning the composed string, while
`InitiateRequest` supplies the corresponding initiation components.
**Impact:** modifying an input type, validation rule, bucket-prefix mapping, or
result shape can alter source callers’ construction contract.
**Evidence:** `src/object_key.rs`; `src/adapter.rs`; `src/types.rs`.
**Stop:** a composed Rust `String` is not evidence of provider acceptance,
object placement, cross-system isolation, or data durability.

[Key and type flow](#b03)

<a id="rel-004"></a>
### REL-004 — Lifecycle handle relation

**Dependency:** `initiate` → `MultipartUpload` → `upload_part`, `complete`,
and `abort`.
**Flow:** later methods take that borrowed handle and explicit tenant ID;
completion receives caller-provided typed part-number/ETag pairs.
**Impact:** changing handle fields, tenant parameter placement, method names, or
part tuple type can affect implementors and Rust callers selecting the trait.
**Evidence:** `src/adapter.rs`; `src/types.rs`; crate-root re-exports.
**Stop:** this does not prove any implementor, a real upload ID, monotonic
completion ordering, atomicity, or retry behavior outside the inspected source.

[Key and type flow](#b03)

<a id="b04"></a>
## B04 — Trait and fake flow relations

<a id="rel-005"></a>
### REL-005 — Local fake implementation relation

**Dependency:** `InMemoryMultipartAdapter` and `AlwaysFailingMultipartAdapter`
→ public crate-root exports and the `MultipartAdapter` trait surface.
**Flow:** `lib.rs` re-exports both adapter names alongside the trait and public
multipart values, making them source-visible to Rust importers.
**Impact:** removing or changing an exported fake can affect tests or callers
that select the public type/path, even when no provider implementation exists.
**Evidence:** `src/lib.rs`; `src/in_memory.rs`; `src/adapter.rs`.
**Stop:** fake presence is not evidence that a test ran or that its behavior is
provider-conformant, production-safe, mounted, or in use.

[Trait/fake flow](#b04) [Relation index](#b03)

<a id="rel-006"></a>
### REL-006 — Part-bound and completion representation relation

**Dependency:** `PartNumber::new` and `PartETag` → `upload_part` and `complete`.
**Flow:** construction checks zero and the named maximum; completion accepts a
caller-provided vector of typed pairs and the fake rejects duplicate part numbers.
No monotonic vector-order validation is present in inspected source.
**Impact:** changing the bound or public type shape can affect callers, fake
behavior, and a future trait implementor.
**Evidence:** `src/types.rs`; `src/bounds.rs`; `src/adapter.rs`.
**Stop:** no execution evidence establishes accepted R2 bounds, ETag meaning,
payload size, or provider ordering behavior.

[Trait/fake flow](#b04) [Relation index](#b03)

<a id="rel-007"></a>
### REL-007 — Per-tenant concurrency source relation

**Dependency:** tenant ID → `PerTenantSemaphore` acquisition API → local
`tokio::sync::Semaphore` permit source.
**Flow:** concurrency source code names tenant-keyed acquisition and the package
manifest enables Tokio’s `sync` feature without default features.
**Impact:** changing keying, permit acquisition API, or configured bound can
alter source-level back-pressure expectations for fake/trait consumers.
**Evidence:** `src/concurrency.rs`; `src/bounds.rs`; `Cargo.toml`.
**Stop:** semaphore source does not prove fairness, active request count,
throughput, exhaustion behavior in a deployment, or an actual R2 upload.

[Trait/fake flow](#b04) [Relation index](#b03)

<a id="b05"></a>
## B05 — Observed adjacent source paths

<a id="rel-008"></a>
### REL-008 — CAS re-export and real-binding adjacency

**Dependency:** `corelink-cas` → `corelink-r2-multipart`; its `src/r2_multipart.rs`
publicly re-exports the package surface.
**Flow:** the CAS module uses `pub use corelink_r2_multipart::*`; cf-bindings has
a real Cloudflare R2 binding with multipart verbs but no `MultipartAdapter` impl.
**Impact:** API/path changes may require assessment of this re-export and
the implementation owner’s boundary.
**Evidence:** CAS manifest/re-export and cf-bindings `src/r2_real.rs` source.
**Stop:** these sources do not prove a trait implementation, CAS call site,
package use of those verbs, worker route, R2 write through this trait, or deployment.

[Observed adjacent paths](#b05) [Relation index](#b03)

<a id="b06"></a>
## B06 — Coverage and explicit unknowns

Coverage includes this package manifest and nine listed source modules, the
tenant-path outbound relation, the CAS manifest/re-export, and selected
cf-bindings adjacency. It excludes a complete reverse dependency graph,
feature/target resolution, all consumer imports, test execution, fake/provider
equivalence, R2 SDK calls, worker/container wiring, D1 sessions, objects,
orphan-sweeper operation, metrics/audit emission, credentials, deployment, and
cold review.

WAVE_007_PLAN and verified canonical OKF were used only as routing reference.
Their policy is neither copied, redefined, nor revalidated here.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#b01)
