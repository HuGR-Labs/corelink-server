---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-ac
manifest: crates/corelink-ac/Cargo.toml
source_commit: 59c76cf260bcdeb5246772f70821ac8b7e8a9780
profile: S
state: draft
evidence_set: ac-source-static-20260920
---

# corelink-ac — blast radius

Static relations only. “Dependency” identifies a declared/source edge, “flow” identifies a source-level value path, and “impact” identifies the change consequence suggested by that evidence. Neither proves runtime invocation or deployment reachability.

[Umbrella](#b01) · [Output aliveness](#b02) · [Merkle](#b03) · [Signature](#b04) · [Schema SQL](#b05) · [Consumers](#b06).

<a id="b01"></a>
## B01 — Umbrella re-export boundary

Dependency: `corelink-ac` depends on `corelink-handler-ac`. Flow: `src/lib.rs` forwards handler public symbols through `corelink_ac::handler`. Impact: changing that shim can change consumer paths, but cannot alter or transfer handler implementation ownership. Evidence: `Cargo.toml:33-36`, `src/lib.rs:43-55`.

<a id="b02"></a>
## B02 — ActionResult-to-blob-meta aliveness relation

Dependency: `StrictOutputsValidator<R>` depends on `BlobMetaReader` and `ActionResult`. Flow: empty output digests → success without reader call; non-empty output digests → one tenant-scoped `batch_check_alive` call → one boolean per digest → validator result. Impact: for non-empty sets, a missing/tombstoned digest, backend failure, or flag-count mismatch fails closed as `BlobMissing` or `BackendError`; changing this relation can admit unalive output references. Evidence: `src/ac_core.rs:119-140`, `src/ac_core/outputs.rs:16-131`.

<a id="b03"></a>
## B03 — Merkle output-to-root relation

Dependency: `merkle.rs` consumes `ActionResult` and bound/error types. Flow: output file and directory digest bytes are sorted, domain-separated, reduced to a root, then root verification compares a claimed root. Impact: changing ordering, prefixes, bounds, or root type can invalidate canonical vectors and worker adapters. Evidence: `src/ac_core/merkle.rs:40-217`, `tests/ac_core_canonical_vectors.rs`.

<a id="b04"></a>
## B04 — Signature preimage-to-verification relation

Dependency: signer/verifier depend on canonical composition and `TdkHandle`. Flow: fixed preimage fields and key ID feed HKDF derivation and keyed MAC verification. Impact: a layout, information-string, tag-length, or key-acceptance change can break signed envelopes and rotation-vector compatibility. Evidence: `src/ac_core/sig.rs:63-132`, `sig/canonical.rs:38-131`, `sig/hkdf_signer.rs:47-428`.

<a id="b05"></a>
## B05 — Schema-source-to-SQL relation

Dependency: `schema.rs` embeds `migrations/d1/0002_ac_meta.sql`. Flow: static SQL text → `MIGRATION_0002_AC_META` → canonical migration test consumption; simulator constraints are an independent mirror and do not parse or compare this SQL. Impact: changing the composite tenant/action key, inline checks, or declared indexes changes the embedded artifact and its canonical migration-test expectations; deploy order and live compatibility are not established. Evidence: `src/schema.rs:61-76`, `migrations/d1/0002_ac_meta.sql:52-122`, `tests/schema_migration_canonical.rs:42-47`.

<a id="b06"></a>
## B06 — Known consumer and harness relations

Dependency: `corelink-cas` and `corelink-worker` directly depend on `corelink-ac`; CF bindings depends on worker; the workspace fuzz harness directly uses `hkdf`/`sha2`, not this crate. Flow: CAS imports signature types, worker projects AC values into canonical paths, and the harness targets HKDF expansion independently. Impact: a public/signature change needs scoped consumer and harness review, while feature-resolved reachability remains unknown. Evidence: CAS `Cargo.toml:30`, worker `Cargo.toml:45`, CF `Cargo.toml:33-53`, root `Cargo.toml:398-399`, fuzz `Cargo.toml:1-32`.

**Shared peer relation:** `repo:1232040291:boundary:hash-ac-digest-serde-001` — `ac_core::types::digest_serde` uses `corelink-hash::Digest` for `ActionDigest` and output digest serde. This records a source-level type/serialization boundary only; wire consumers, selected targets, and runtime are unknown. Hash-side record: [corelink-hash REL-020](../corelink-hash/BLAST_RADIUS.md#rel-020). Evidence: `src/ac_core/types.rs:15-55` at this document's source pin.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#b01).
