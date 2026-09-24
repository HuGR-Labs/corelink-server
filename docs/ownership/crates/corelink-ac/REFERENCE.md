---
schema: corelink-ownership/1.1
document: reference
package: corelink-ac
manifest: crates/corelink-ac/Cargo.toml
source_commit: 59c76cf260bcdeb5246772f70821ac8b7e8a9780
profile: S
state: draft
evidence_set: ac-source-static-20260920
---

# corelink-ac — reference

Static reference for the `corelink-ac` hybrid umbrella. Sources are the manifest and checked-in Rust/test files at `59c76cf260bcdeb5246772f70821ac8b7e8a9780`; no command execution, runtime behavior, deploy, or cold review is represented.

[Identity](#r01) · [Exports](#r02) · [Merkle](#r03) · [Envelope](#r04) · [Signature](#r05) · [Schema](#r06) · [Tests](#r07) · [Limits](#r08).

<a id="r01"></a>
## R01 — Package identity and ownership

`crates/corelink-ac/Cargo.toml` names the package `corelink-ac`. Its description and `src/lib.rs` place absorbed AC core and schema modules inside this crate. `corelink-handler-ac` remains a dependency, so this package does not own the handler implementation. Evidence: `Cargo.toml:1-36`, `src/lib.rs:1-55`.

<a id="r02"></a>
## R02 — Public module contract

The root forbids unsafe code and denies missing docs. It publicly re-exports `ac_core::*`, exposes `schema`, and exposes `handler` only as `pub use corelink_handler_ac::*`. Existing handler symbols are reachable through the shim, but implementation ownership remains external. Evidence: `src/lib.rs:28-55`.

<a id="r03"></a>
## R03 — Canonical Merkle contract

`merkle.rs` exports `MerkleVerifier`, `CanonicalMerkleVerifier`, root construction, root verification, and result-hash construction. Leaf and inner prefixes are respectively `0x00` and `0x01`; the builder handles an empty tree, collects file then directory digests, sorts digest bytes, and applies bounds before construction. Evidence: `src/ac_core/merkle.rs:40-217`.

<a id="r04"></a>
## R04 — Envelope, bounds, and errors

The envelope codec accepts only `AC_ENVELOPE_VERSION` 1 and rejects payloads above `MAX_PAYLOAD_BYTES` before decoding. Declared bounds include depth, fanout, nodes, payload, files, and directories. `MerkleError` exposes typed structural, mismatch, decode, and version failures plus stable audit-code mapping. Evidence: `bounds.rs:15-51`, `codec.rs:24-69`, `error.rs:15-152`, `types.rs:224-276`.

The public output-aliveness boundary consists of `BlobMetaReader`, `OutputsValidator`, and `StrictOutputsValidator`, re-exported by `ac_core.rs`. `StrictOutputsValidator` succeeds without a reader call for an empty `ActionResult`; otherwise it collects every output digest, calls a tenant-scoped batch reader once, and accepts only one `true` flag per input digest. Falsifiable invariant: for a non-empty digest set, a returned flag count different from digest count, any backend error, or any `false` flag must return `BackendError` or `BlobMissing`, never success. Evidence: `ac_core.rs:119-140`, `outputs.rs:16-131`.

<a id="r05"></a>
## R05 — Signature and rotation contract

The signature module exports signer/verifier traits, canonical-byte composition, TDK seam, and HKDF implementation. Canonical preimages are fixed at 121 bytes; signature tags are 32 bytes. Key ID zero is reserved, while verifier acceptance is configured through its public API. Static source indicates HKDF/SHA-256 derivation and keyed BLAKE3 MAC; it does not prove key custody or production rotation policy. Evidence: `sig.rs:63-132`, `sig/canonical.rs:38-131`, `sig/hkdf_signer.rs:47-428`.

<a id="r06"></a>
## R06 — Region and schema-simulation contract

`schema.rs` embeds `migrations/d1/0002_ac_meta.sql` as `MIGRATION_0002_AC_META`; source review therefore includes the SQL artifact. It declares composite primary key `(tenant_id, action_digest)`, inline checks for digest/size/region/signature/lifecycle/prefix/key constraints, and tenant-expiry, tenant-last-hit, and region indexes. `AcRegion` limits the list to `sam`, `iad`, `lhr`, `nrt`, and `syd`; `AcSchema` mirrors selected constraints and upsert outcomes. This is static SQL/simulator evidence, not D1 execution, deploy order, or live compatibility evidence. Evidence: `schema.rs:61-76`, `migrations/d1/0002_ac_meta.sql:52-122`, `schema/region.rs:14-102`, `schema/sim.rs:39-385`.

<a id="r07"></a>
## R07 — Declared integration checks

The manifest explicitly declares Merkle property/canonical/round-trip/tampering tests, signature property/canonical/key-rotation/timing tests, and schema migration/property/idempotency tests. Their source names describe intended coverage, but this reference makes no PASS claim. Evidence: `Cargo.toml:44-88`; `tests/ac_core_*.rs`; `tests/schema_*.rs`.

<a id="r08"></a>
## R08 — Evidence limits and unknowns

The reviewed static set does not establish feature-resolved dependency closure, all downstream consumers, real handler semantics, D1 migration deployment, key-service behavior, timing characteristics, or production data recovery. Treat each as unknown until a scoped, separately recorded evidence source is supplied.

[Ownership guide](../../../../.claude/skills/own-corelink-ac/SKILL.md#s01) · [Blast radius](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01).
