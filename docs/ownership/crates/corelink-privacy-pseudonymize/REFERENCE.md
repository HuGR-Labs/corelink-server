---
schema: corelink-ownership/1.1
document: reference
package: corelink-privacy-pseudonymize
manifest: crates/corelink-privacy-pseudonymize/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-privacy-pseudonymize-structural-normalization-20260921
---

# corelink-privacy-pseudonymize — reference

SOURCE-only record for `Cargo.toml`, `src/lib.rs`, and the named test target. It does not promote crate comments or a static re-export into evidence of a privacy workflow or external effect.

[Identity](#r01) · [Boundary](#r02) · [SHA-256](#r03) · [Marker](#r04) · [Verify](#r05) · [Dependencies](#r06) · [Tests](#r07) · [Unknowns](#r08)

<a id="r01"></a>
## R01 — Identity and source territory

The manifest names package `corelink-privacy-pseudonymize`. Its sole production source file is `src/lib.rs`; that file defines `PseudonymHash`, `PseudonymizationMarker`, four public constants, and three public helper functions. It forbids unsafe code and denies missing docs and missing `Debug` implementations. No additional production module, trait, IO client, clock, or runtime loop is defined in this package.

Evidence: `crates/corelink-privacy-pseudonymize/{Cargo.toml,src/lib.rs}`.

<a id="r02"></a>
## R02 — Boundary and non-claims

This crate owns deterministic in-process value construction and equality checking only. `pseudonymize` accepts bytes and a caller-supplied fixed-size salt; `pseudonymize_subject_id` accepts a `Uuid` and that salt; `verify_pseudonym` returns a `bool`; `PseudonymizationMarker::from_hash` creates a serializable value. The source has no API for sourcing, persisting, rotating, logging, or disclosing salt material, and no API that writes a marker or mutates a record.

Neither a digest nor a marker returned by this crate is evidence of actual pseudonymization, anonymization, redaction, erasure, or recovery in any environment.

Evidence: `src/lib.rs` (`pseudonymize`, `pseudonymize_subject_id`, `verify_pseudonym`, `PseudonymizationMarker::from_hash`).

<a id="r03"></a>
## R03 — Exact SHA-256 and representation contract

`pseudonymize(subject_id_bytes, erasure_salt)` creates a `Sha256`, updates it first with `subject_id_bytes` and then with `erasure_salt`, finalizes it, and copies the result into `PseudonymHash([u8; 32])`. The source-visible formula is exactly `sha256(subject_id_bytes || erasure_salt)`, with order part of the contract. `ERASURE_SALT_LEN` is `32`; the function requires `&[u8; ERASURE_SALT_LEN]`, not an arbitrary-length slice.

`PseudonymHash::from_bytes` and `as_bytes` preserve the fixed 32-byte shape. `to_hex` uses `hex::encode`, and `PSEUDONYM_HEX_LEN` is `64`; source tests assert a 64-character lowercase-hex result. These are representation contracts only, not a persisted-format compatibility policy.

**INV-001:** For the same byte slice and same 32-byte salt, the helper follows the same ordered SHA-256 update sequence and returns a 32-byte wrapper.

Evidence: `src/lib.rs` (`ERASURE_SALT_LEN`, `PSEUDONYM_HEX_LEN`, `PseudonymHash`, `pseudonymize`); `tests/pseudonymization_invariants.rs`.

<a id="r04"></a>
## R04 — Exact marker construction contract

`PII_REDACTED_MARKER_KEY` is `"pii_redacted"` and `PII_REDACTED_MARKER_VALUE` is `"true"`. `PseudonymizationMarker::from_hash` constructs a value whose public `pii_redacted` field is a newly allocated copy of the latter constant and whose public `pseudonym` field is `hash.to_hex()`. Deriving serde serialization makes this a source-defined serializable shape with those field names.

**INV-002:** A marker constructed through `from_hash` contains the literal `"true"` value and the supplied hash's hex rendering. No source predicate says a marker reaches a database, object, audit record, or schema.

Evidence: `src/lib.rs` (`PII_REDACTED_MARKER_*`, `PseudonymizationMarker`); `tests/pseudonymization_invariants.rs` (`marker_constants_pinned`, `marker_serializes_to_canonical_json`).

<a id="r05"></a>
## R05 — Re-derivation and constant-time comparison contract

`pseudonymize_subject_id` delegates to `pseudonymize(subject_id.as_bytes(), erasure_salt)`. `verify_pseudonym` recomputes that result for its candidate `Uuid` and salt, then calls `recomputed.as_bytes().ct_eq(expected.as_bytes())` from `subtle::ConstantTimeEq` and converts the choice to `bool`.

**INV-003:** The final equality decision in this function is delegated to `ConstantTimeEq::ct_eq` over the two 32-byte arrays. This SOURCE evidence does not measure timing on any target, certify every surrounding operation as constant-time, grant access to a salt, or perform a live correlation.

Evidence: `src/lib.rs` (`pseudonymize_subject_id`, `verify_pseudonym`); `tests/pseudonymization_invariants.rs` (round-trip and mismatch properties).

<a id="r06"></a>
## R06 — Direct dependency boundary

The manifest directly declares `serde`, `sha2`, `hex`, `subtle`, and `uuid` under normal dependencies; `proptest` and `serde_json` are dev dependencies. The source imports serde derives, `sha2::{Digest, Sha256}`, `subtle::ConstantTimeEq`, and `uuid::Uuid`. No first-party package dependency is declared by this crate.

This inventory is not a resolved dependency graph, security assessment, cryptographic certification, or target-compatibility result.

Evidence: `Cargo.toml`; `src/lib.rs` imports.

<a id="r07"></a>
## R07 — Named source test target

The manifest declares test target `pseudonymization_invariants` at `tests/pseudonymization_invariants.rs`. Its named assertions cover deterministic derivation, changed subject/salt examples, marker constants/serialization, re-derivation acceptance/rejection, and hex shape; property-test configuration reads `PROPTEST_CASES` and defaults to 10,000. This is a test-source inventory, not a test execution result.

Evidence: `Cargo.toml`; `tests/pseudonymization_invariants.rs`; `src/lib.rs` unit-test module.

<a id="r08"></a>
## R08 — Unknowns and prohibited extrapolations

SOURCE evidence here does not establish salt generation, per-tenant or per-request scope, secret handling, KMS/vault integration, storage writes, marker insertion, DSR handling, legal compliance, data deletion, retention, audit behavior, route mounting, caller authorization, timing measurements, complete consumer coverage, deployment, publication, or runtime reachability. Those claims require separate selected source, resolved, execution, provider, or runtime evidence from their owning boundary.

[Blast radius](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01) · [Ownership guide](../../../../.claude/skills/own-corelink-privacy-pseudonymize/SKILL.md#s01)
