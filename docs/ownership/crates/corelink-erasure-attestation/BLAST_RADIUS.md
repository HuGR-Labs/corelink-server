---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-erasure-attestation
manifest: crates/corelink-erasure-attestation/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-erasure-attestation-structural-normalization-20260921
---

# corelink-erasure-attestation — blast radius

Static map of public contracts and direct relations. Declarations/imports do not
prove execution or external effect.

[Scope](#b01) · [Dependencies](#b02) · [Contracts](#b03) · [Consumers](#b04) · [Representation](#b05) · [Coverage](#b06).

<a id="b01"></a>
## B01 — Scope and reading rule

Record index: [REL-001](#rel-001) · [REL-002](#rel-002) · [REL-003](#rel-003) · [REL-004](#rel-004) · [REL-005](#rel-005) · [REL-006](#rel-006) · [REL-007](#rel-007) · [REL-008](#rel-008) · [REL-009](#rel-009) · [REL-010](#rel-010)

Package scope is local Rust signing, canonicalization, evidence, key, region,
verification, and error representation. R2/D1 persistence/indexing, rotation,
public verification delivery, and composition are external.

<a id="b02"></a>
## B02 — Declared dependency boundary

<a id="rel-001"></a>
### REL-001 — Cryptographic and representation dependencies

**Dependency:** package to ed25519-dalek, getrandom, serde_jcs, base64, sha2,
serde, serde_json, thiserror, zeroize, hex, tracing. **Flow:** imports use them
for signing/generation, canonicalization, encoding, hashing, serialization,
errors, zeroization, logging. **Impact:** exposed behavior changes require source
assessment. **Evidence:** Cargo.toml and source modules. **Stop:** no resolved
version, selected feature, or third-party behavior follows. [Relation index](#b03)

<a id="rel-002"></a>
### REL-002 — No first-party outbound dependency

**Dependency:** no corelink-* dependency declared. **Flow:** local contracts
have no declared first-party edge. **Impact:** adding one changes leaf boundary.
**Evidence:** Cargo.toml. **Stop:** does not prove workspace acyclicity/build. [Relation index](#b03)

<a id="b03"></a>
## B03 — Contract impact relations

<a id="rel-003"></a>

### REL-003 — Signed representation callers

**Dependency:** callers to payload, attestation, signer. **Flow:** callers supply
eight fields; sign returns payload, signature text, JCS text. **Impact:** field,
canonicalization, or signature changes affect source callers. **Evidence:**
attestation.rs. **Stop:** no persisted-format compatibility. [Relation index](#b03)

<a id="rel-004"></a>
### REL-004 — Evidence-hash callers

**Dependency:** callers to EvidenceBundle hash methods. **Flow:** callers build
four-field bundle and receive hash or validation error. **Impact:** changed
fields/fallback/checks affect callers. **Evidence:** evidence.rs. **Stop:** no
KMS, audit event, or erase proof. [Relation index](#b03)

<a id="rel-005"></a>
### REL-005 — Key and region callers

**Dependency:** callers to signing/public keys and Region. **Flow:** callers
construct local keys, derive public form, map regions. **Impact:** IDs, fields,
PEM/fingerprint, mappings affect callers. **Evidence:** key.rs, region.rs.
**Stop:** no secret, bucket, region, or rotation operation. [Relation index](#b03)

<a id="rel-006"></a>
### REL-006 — Offline-verifier callers

**Dependency:** callers to verifier/errors. **Flow:** caller supplies
attestation/key; source performs payload-byte, base64-length, Ed25519 checks.
**Impact:** changed order/checks/errors affect direct callers. **Evidence:**
verify.rs, error.rs. **Stop:** does not retrieve key or enforce payload key-ID/
region equality. [Relation index](#b03)

<a id="b04"></a>
## B04 — Observed static consumers and alias

<a id="rel-007"></a>

### REL-007 — Crypto umbrella re-export

**Dependency:** corelink-crypto to this package. **Flow:** ed25519.rs re-exports
full API at corelink_crypto::ed25519::attestation. **Impact:** changes may affect
alias users. **Evidence:** corelink-crypto Cargo.toml and src/ed25519.rs.
**Stop:** no migration/compatibility proof. [Relation index](#b03)

<a id="rel-008"></a>
### REL-008 — Container imports

**Dependency:** corelink-container to selected symbols. **Flow:** imports in
routes/dsr/attestation.rs, dsr.rs, public_attestation.rs, audit_drain.rs.
**Impact:** public-name/representation changes may need composition review.
**Evidence:** container Cargo.toml and named source paths. **Stop:** imports do
not prove R2/D1 behavior, mounted routes, key handling, runtime. [Relation index](#b03)

<a id="b05"></a>
## B05 — Representation effects

<a id="rel-009"></a>

### REL-009 — Canonical payload and signature text

**Dependency:** callers/readers to payload, JCS text, base64 signature. **Flow:**
sign writes all; verifier binds payload to JCS. **Impact:** field/canonicalization
changes affect readers. **Evidence:** attestation.rs, verify.rs. **Stop:** no
artifact history/storage schema. [Relation index](#b03)

<a id="rel-010"></a>
### REL-010 — Key identity and public encoding

**Dependency:** callers to key ID, region, timestamps, verifying key, PEM,
fingerprint. **Flow:** public_key copies identity and derives public values.
**Impact:** changes affect key selection/display. **Evidence:** key.rs. **Stop:**
no active, served, pinned, or deployed key. [Relation index](#b03)

<a id="b06"></a>
## B06 — Coverage and explicit unknowns

Covers direct dependencies, no-first-party edge, local representations, crypto
re-export, named container imports. Excludes reverse consumers, targets/features,
compatibility, key custody, KMS/audit, R2/D1 persistence/retention, rotation,
endpoint service, regional deployment, runtime, deployment, cold review.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#b01)
