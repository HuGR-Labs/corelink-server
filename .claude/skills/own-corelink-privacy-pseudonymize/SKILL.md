---
name: own-corelink-privacy-pseudonymize
description: Ownership routing for corelink-privacy-pseudonymize; static draft only and never production authorization.
metadata:
  schema: corelink-ownership/1.1
  package: corelink-privacy-pseudonymize
  manifest: crates/corelink-privacy-pseudonymize/Cargo.toml
  source-commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
  evidence-set: corelink-privacy-pseudonymize-structural-normalization-20260921
---

# Ownership — corelink-privacy-pseudonymize

[S01](#s01) · [S02](#s02) · [S03](#s03) · [S04](#s04) · [S05](#s05) · [S06](#s06) · [S07](#s07)

This guide is limited to `Cargo.toml` and the one production source file,
`src/lib.rs`, at the recorded baseline. It records a pure helper contract only:
SHA-256 derivation, marker construction, and constant-time equality. It records
no salt custody, storage write, erasure, forensic operation, route, provider,
deployment, or runtime result.

[Baseline](#s01) · [Boundary](#s02) · [Digest](#s03) · [Marker](#s04) · [Verify](#s05) · [Static maintenance](#s06) · [Handoff](#s07)

<a id="s01"></a>
## S01 — Establish the static baseline

**Condition:** an ownership or public-helper change is proposed. **Action:** record the revision and inspect `crates/corelink-privacy-pseudonymize/Cargo.toml` and `src/lib.rs`. **Evidence:** package declaration and [R01](../../../docs/ownership/crates/corelink-privacy-pseudonymize/REFERENCE.md#r01). **Stop:** the claim needs execution or an external effect.

<a id="s02"></a>
## S02 — Preserve the pure-helper boundary

**Condition:** a dependency, IO path, salt source, writer, or provider is proposed. **Action:** keep the owned conclusion to the functions, value types, and direct crate dependencies visible in the manifest/source. **Evidence:** [R02](../../../docs/ownership/crates/corelink-privacy-pseudonymize/REFERENCE.md#r02) and [B06](../../../docs/ownership/crates/corelink-privacy-pseudonymize/BLAST_RADIUS.md#b06). **Stop:** do not treat a marker value or digest as evidence that data was redacted, stored, erased, or made anonymous.

<a id="s03"></a>
## S03 — Preserve the SHA-256 representation contract

**Condition:** `pseudonymize`, `pseudonymize_subject_id`, `PseudonymHash`, or a digest/hex constant changes. **Action:** compare the exact input order `subject_id_bytes` then `erasure_salt`, the `[u8; 32]` salt and output shapes, and the 64-character lowercase-hex rendering. **Evidence:** [R03](../../../docs/ownership/crates/corelink-privacy-pseudonymize/REFERENCE.md#r03) and [B01](../../../docs/ownership/crates/corelink-privacy-pseudonymize/BLAST_RADIUS.md#b01). **Stop:** a compatibility assertion for stored or transmitted values needs consumer/data evidence outside this package.

<a id="s04"></a>
## S04 — Preserve the marker construction contract

**Condition:** `PseudonymizationMarker`, `PII_REDACTED_MARKER_KEY`, or `PII_REDACTED_MARKER_VALUE` changes. **Action:** retain the source-defined `"pii_redacted"`, `"true"`, and digest-derived pseudonym fields, or explicitly record every representation change. **Evidence:** [R04](../../../docs/ownership/crates/corelink-privacy-pseudonymize/REFERENCE.md#r04) and [B02](../../../docs/ownership/crates/corelink-privacy-pseudonymize/BLAST_RADIUS.md#b02). **Stop:** construction of this serializable value does not prove a backend inserted it or removed any PII.

<a id="s05"></a>
## S05 — Preserve re-derivation and constant-time comparison

**Condition:** `verify_pseudonym` or its inputs change. **Action:** preserve the re-derivation through `pseudonymize_subject_id` and the final `subtle::ConstantTimeEq::ct_eq` comparison of the two 32-byte arrays. **Evidence:** [R05](../../../docs/ownership/crates/corelink-privacy-pseudonymize/REFERENCE.md#r05) and [B03](../../../docs/ownership/crates/corelink-privacy-pseudonymize/BLAST_RADIUS.md#b03). **Stop:** this source call is not a timing measurement, authorization check, or live re-identification operation.

<a id="s06"></a>
## S06 — Keep maintenance source-static

**Condition:** validation, recovery, or compatibility work is requested. **Action:** use the source-static procedures in [M02](../../../docs/ownership/crates/corelink-privacy-pseudonymize/MAINTENANCE.md#m02). **Evidence:** [M03](../../../docs/ownership/crates/corelink-privacy-pseudonymize/MAINTENANCE.md#m03). **Stop:** Cargo execution, tests, secrets, salt material, KMS, database/object operations, deployment, or publication are outside this guide.

<a id="s07"></a>
## S07 — Hand off bounded evidence

**Condition:** the scoped record is ready for another owner. **Action:** report baseline, source symbols, static relations, changed paths, documentary checker results, and unknowns. **Evidence:** [M06](../../../docs/ownership/crates/corelink-privacy-pseudonymize/MAINTENANCE.md#m06). **Stop:** an author check is not a semantic approval, runtime observation, or cold review.
