---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-core
manifest: crates/corelink-core/Cargo.toml
source_commit: 5d617634662ee9475dc66cb83b5a57296eb75dc2
profile: S
state: author_validated
evidence_set: core-static-source-20260920
---

# corelink-core — blast radius

Static dependency and contract map for the apex core crate. Every relation below is atomic and source- or manifest-backed; none proves selection, migration, runtime reachability, or deployment.

[Scope](#b01) · [Outbound](#b02) · [Consumers](#b03) · [Formats](#b04) · [Secrets and time](#b05) · [Coverage](#b06).

<a id="b01"></a>
## B01 — Scope and reading rule

The package is an apex with no declared `corelink-*` dependency. Changes can propagate through its exported type contracts even when Cargo has no internal outgoing edge. The map separates manifest facts from behavior described in source comments.

<a id="b02"></a>
## B02 — Outbound dependency boundary

[REL-001](#rel-001) · [REL-002](#rel-002).

<a id="rel-001"></a>
### REL-001 — Core package to external representation dependencies

**Dependency:** `corelink-core` → serde, thiserror, uuid, subtle, secrecy.
**Flow:** public types and errors use these libraries for serialization, errors, UUIDs, comparison, and secret wrapping.
**Impact:** version or contract changes may alter compile-time or representation behavior.
**Evidence:** `Cargo.toml`; `types/*.rs`; `errors.rs`.
**Stop:** this relation does not identify external library runtime behavior or a selected feature.
[Relation index](#b02) [Relation index](#b03)

<a id="rel-002"></a>
### REL-002 — Apex restriction to CoreLink dependencies

**Dependency:** no `corelink-*` dependency is declared by the package.
**Flow:** its source can define shared types without a manifest cycle to another CoreLink package.
**Impact:** adding one may change the apex graph and cycle risk.
**Evidence:** `crates/corelink-core/Cargo.toml`.
**Stop:** manifest inspection alone does not prove the whole workspace graph or enforcement.
[Relation index](#b02) [Relation index](#b03)

<a id="b03"></a>
## B03 — Direct manifest consumers

[REL-003](#rel-003) · [REL-004](#rel-004) · [REL-005](#rel-005) · [REL-006](#rel-006). [Relation index](#b03)

<a id="rel-003"></a>
### REL-003 — Access-control manifest edge

**Dependency:** `corelink-ac` → `corelink-core`.
**Flow:** Cargo manifest declares the core package.
**Impact:** incompatible core API changes can affect an ac build that selects this edge.
**Evidence:** `crates/corelink-ac/Cargo.toml`.
**Stop:** no source import, feature choice, target selection, or runtime path is asserted. [Relation index](#b03)

<a id="rel-004"></a>
### REL-004 — Adapter-host manifest edge

**Dependency:** `corelink-adapter-host` → `corelink-core`.
**Flow:** normal and development manifest sections name the core package.
**Impact:** an incompatible core API may affect corresponding adapter-host builds.
**Evidence:** `crates/corelink-adapter-host/Cargo.toml`.
**Stop:** duplicate declarations do not prove imports, test execution, or deployment. [Relation index](#b03)

<a id="rel-005"></a>
### REL-005 — CAS manifest edge

**Dependency:** `corelink-cas` → `corelink-core`.
**Flow:** Cargo manifest declares the core package.
**Impact:** changing a shared type may require CAS compatibility review.
**Evidence:** `crates/corelink-cas/Cargo.toml`.
**Stop:** source use, selected target, and data-path reachability remain unknown. [Relation index](#b03)

<a id="rel-006"></a>
### REL-006 — Server manifest edge

**Dependency:** `corelink-server` → `corelink-core`.
**Flow:** Cargo manifest declares the core package and contains type-oriented commentary.
**Impact:** shared-type changes may affect server builds.
**Evidence:** `crates/corelink-container/Cargo.toml`.
**Stop:** comments and presence do not establish an import, migration, or runtime composition path. [Relation index](#b03)

<a id="b04"></a>
## B04 — Identity and digest format effects

[REL-007](#rel-007) · [REL-008](#rel-008).

<a id="rel-007"></a>
### REL-007 — Tenant and region textual contracts

**Dependency:** callers → `TenantId` and `Region` formatting.
**Flow:** UUID display/canonical text and region lowercase tokens leave the type boundary.
**Impact:** changing text grammar, case, or region set can break comparison or persisted-text consumers.
**Evidence:** `types/tenant.rs`; `types/region.rs`.
**Stop:** no caller, schema, or persisted record was traced in this work.
[Relation index](#b04) [Relation index](#b03)

<a id="rel-008"></a>
### REL-008 — Digest text and bytes contracts

**Dependency:** callers → `Digest` hexadecimal and raw-byte APIs.
**Flow:** parsing accepts 64 hex characters; rendering and serde emit lowercase 64-character text.
**Impact:** width, case, alphabet, or raw-byte changes can break serialized or keyed consumers.
**Evidence:** `types/digest.rs`.
**Stop:** parser behavior is not a proof of content verification or data migration.
[Relation index](#b04) [Relation index](#b03)

<a id="b05"></a>
## B05 — Secret and time effects

[REL-009](#rel-009) · [REL-010](#rel-010). [Relation index](#b03)

<a id="rel-009"></a>
### REL-009 — Secret wrapper formatting boundary

**Dependency:** callers → `SecretWrap` explicit exposure and debug formatting.
**Flow:** plaintext is returned only by an explicit method; debug writes a redacted marker.
**Impact:** changing either can expose values or break callers expecting the wrapper boundary.
**Evidence:** `types/secret.rs`.
**Stop:** no secret values, credential consumer, log sink, or operation is in scope.
[Relation index](#b05) [Relation index](#b03)

<a id="rel-010"></a>
### REL-010 — Clock default conversion boundary

**Dependency:** clock implementors → `Clock::now_unix_ms`.
**Flow:** the default derives milliseconds from `now`, with pre-epoch zero and overflow saturation.
**Impact:** a semantic change can alter timestamps received by callers of the default.
**Evidence:** `time.rs`.
**Stop:** there is no concrete implementation or runtime timing trace in this map.
[Relation index](#b05) [Relation index](#b03)

<a id="b06"></a>
## B06 — Coverage and unknowns

This map covers all four direct manifest consumers found by the prescribed static search, the no-internal-CoreLink-dependency assertion, and the six source surfaces. It does not cover reverse transitive dependencies, feature-conditioned imports, legacy types, concrete clocks, storage schemas, external effects, or deployment paths.

Before a compatibility change, obtain a fresh full graph and consumer evidence; do not represent this static set as complete migration coverage.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#b01)
