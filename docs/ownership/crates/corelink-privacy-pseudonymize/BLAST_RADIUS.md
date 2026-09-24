---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-privacy-pseudonymize
manifest: crates/corelink-privacy-pseudonymize/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-privacy-pseudonymize-structural-normalization-20260921
---

# corelink-privacy-pseudonymize — blast radius

SOURCE-only relationship map. A relation maps code-visible dependency, flow, and impact; it does not establish that any privacy operation occurs.

[Derivation](#b01) · [Marker](#b02) · [Verification](#b03) · [Static aliases](#b04) · [Tests](#b05) · [Unknowns](#b06)

<a id="b01"></a>
## B01 — Ordered derivation relation

**Dependency:** `pseudonymize` depends on `sha2::Sha256` and caller-provided bytes. **Flow:** `subject_id_bytes` enters `update` before the 32-byte salt; the finalized digest is copied into `PseudonymHash([u8; 32])`. **Impact:** changing input order, algorithm call sequence, salt length, wrapper width, or hex rendering changes the helper's output representation and can affect any static caller that uses it. **Boundary:** no caller, data store, or external record is part of this relation.

Evidence: `src/lib.rs` (`pseudonymize`, `PseudonymHash`, constants).

<a id="b02"></a>
## B02 — Marker relation

**Dependency:** `PseudonymizationMarker::from_hash` depends on `PseudonymHash::to_hex` and the two public marker constants. **Flow:** one hash becomes a struct with `pii_redacted: "true"` and its lowercase hex pseudonym. **Impact:** changing either literal, field name, serde shape, or hex rendering changes the constructed value and may affect selected serializers or static consumers. **Boundary:** the crate constructs a value only; no insert, update, or redaction side effect is present.

Evidence: `src/lib.rs` (`PseudonymizationMarker`, `from_hash`, `PII_REDACTED_MARKER_*`).

<a id="b03"></a>
## B03 — Verification relation

**Dependency:** `verify_pseudonym` depends on UUID byte conversion, `pseudonymize_subject_id`, and `subtle::ConstantTimeEq`. **Flow:** candidate UUID and salt are re-derived into a 32-byte hash, which is compared with the expected 32-byte hash using `ct_eq`; the result becomes `bool`. **Impact:** altering UUID conversion, derivation, or equality changes match decisions. **Boundary:** this source relation proves the `ct_eq` call, not timing behavior of a whole process or a real re-correlation operation.

Evidence: `src/lib.rs` (`pseudonymize_subject_id`, `verify_pseudonym`).

<a id="b04"></a>
## B04 — Static package and alias relations

**Dependency:** the workspace root lists this crate as a member and workspace dependency; `corelink-privacy-erasure-worker` and `corelink-privacy` declare direct manifest dependencies. **Flow:** `corelink-privacy-erasure-worker` re-exports selected helper names in `src/pseudonymize.rs`; `corelink-privacy` re-exports the entire public API in `src/pseudonymize.rs`. **Impact:** a public API change can reach those paths. **Boundary:** manifest edges and re-exports do not prove every symbol is used, an operation is run, or a consumer is compatible.

Evidence: root `Cargo.toml`; `crates/corelink-privacy-erasure-worker/Cargo.toml`; `crates/corelink-privacy/Cargo.toml`; `crates/corelink-privacy-erasure-worker/src/pseudonymize.rs`; `crates/corelink-privacy/src/pseudonymize.rs`.

<a id="b05"></a>
## B05 — Test-source relation

**Dependency:** the named integration test depends on the public helper surface; the unit-test module depends on the same local functions. **Flow:** synthetic inputs exercise deterministic, mismatch, marker, and representation predicates. **Impact:** source test changes alter the documented assertion set. **Boundary:** no test was run for this ownership pass, and no test establishes an external data effect or timing measurement.

Evidence: `Cargo.toml`; `tests/pseudonymization_invariants.rs`; `src/lib.rs` unit-test module.

<a id="b06"></a>
## B06 — Unresolved or external blast radius

Unknowns include complete reverse dependencies/features, values passed by each consumer, wire or stored-value compatibility, salt provenance/custody, KMS or vault behavior, marker persistence, any live pseudonymization/erasure flow, authorization, timing on a selected target, deployment, and runtime reachability. Route each to the data, integration, security, or runtime owner with directly applicable evidence. Do not infer an irreversible or live privacy operation from B01–B05.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Ownership guide](../../../../.claude/skills/own-corelink-privacy-pseudonymize/SKILL.md#s01)
