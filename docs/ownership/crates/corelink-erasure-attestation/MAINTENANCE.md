---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-erasure-attestation
manifest: crates/corelink-erasure-attestation/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-erasure-attestation-structural-normalization-20260921
---

# corelink-erasure-attestation — maintenance

Source-static procedures only: no Cargo/tests/network/key/KMS/R2/D1/rotation/
endpoint/deploy/publication operation.

[Preparation](#m01) · [Selection](#m02) · [Procedures](#m03) · [Predicates](#m04) · [Recovery](#m05) · [Handoff](#m06).

<a id="m01"></a>
## M01 — Safe preparation

Record index: [PROC-001](#proc-001) · [PROC-002](#proc-002) · [PROC-003](#proc-003) · [PROC-004](#proc-004) · [PROC-005](#proc-005)

Work in scoped checkout at recorded revision. Record baseline, status, manifest,
and relevant source; do not inspect keys, secrets, or remote services.

**Stop:** baseline, package, or writable scope differs. **Recovery:** stop
without unrelated changes. **Evidence:** revision, status, paths read.

<a id="m02"></a>
## M02 — Procedure selection

| Condition | Mode | Predicate | Stop |
|---|---|---|---|
| Payload, JCS, signature, verifier | [PROC-001](#proc-001) STATIC_SIGN_VERIFY | Exact checks/representation enumerated | Compatibility/execution needed |
| Evidence/hash | [PROC-002](#proc-002) STATIC_EVIDENCE | Hash methods classified separately | KMS/audit operation needed |
| Key/region | [PROC-003](#proc-003) STATIC_KEY_REGION | Fields/mappings and lifecycle boundary listed | Secret/rotation/deployment needed |
| Consumer impact | [PROC-004](#proc-004) STATIC_GRAPH | Observed relations separate from full graph | Full consumer/build needed |
| Docs ready | [PROC-005](#proc-005) STATIC_HANDOFF | Four records/navigation/scope pass | Structural/scope failure |

<a id="m03"></a>
## M03 — Procedures

<a id="proc-001"></a>
### PROC-001 — Preserve signing and verification

**Mode:** STATIC_SIGN_VERIFY. **Prerequisite:** affected symbol known.
**Predicate:** JCS input, signed bytes, base64 shape, payload equality, and
caller selection responsibility compared to R03.

1. Read attestation.rs, verify.rs, R03, REL-003, REL-006.
2. Enumerate changed payload fields, checks, errors.
3. Keep retrieval and key/region/key-ID matching outside conclusion without
   separate evidence.

**Stop:** persisted-artifact/caller compatibility proof needed. **Recovery:**
request bounded evidence. **Evidence:** symbols, contract, unknowns. [Procedure index](#m02)

<a id="proc-002"></a>

### PROC-002 — Preserve evidence-hash semantics

**Mode:** STATIC_EVIDENCE. **Prerequisite:** bundle/hash behavior identified.
**Predicate:** fallback and three validated_hash checks stated separately.

1. Read evidence.rs, R04, INV-003, REL-004.
2. Record fields, serialization, digest, validation.
3. Do not call local bundle observed KMS destruction or complete evidence.

**Stop:** provider/audit/erase behavior needed. **Recovery:** operation-owner
handoff. **Evidence:** source symbols and operational unknown. [Procedure index](#m02)

<a id="proc-003"></a>

### PROC-003 — Preserve key and region representations

**Mode:** STATIC_KEY_REGION. **Prerequisite:** key, PEM/fingerprint, or region
symbol known. **Predicate:** fields/mappings and redaction/zeroization annotations
enumerated.

1. Read key.rs, region.rs, R04, INV-004, REL-005, REL-010.
2. Classify local representation, generation/derivation, or naming change.
3. State seed custody, active/overlap state, rotation, bucket, deployment unknown.

**Stop:** secret/rotation/deployment change. **Recovery:** authorized operator
evidence. **Evidence:** symbols and boundary. [Procedure index](#m02)

<a id="proc-004"></a>

### PROC-004 — Reconcile aliases and imports

**Mode:** STATIC_GRAPH. **Prerequisite:** public symbol known. **Predicate:**
crypto re-export/container imports separate from unknown consumers.

1. Inspect REL-007 and REL-008 paths.
2. Record import, alias, or local effect.
3. Retain selected features, reverse graph, mounting, runtime as unknown.

**Stop:** every consumer/build needed. **Recovery:** fresh owner evidence.
**Evidence:** paths and relation IDs. [Procedure index](#m02)

<a id="proc-005"></a>

### PROC-005 — Hand off scoped documentation

**Mode:** STATIC_HANDOFF. **Prerequisite:** only four owned artifacts changed.
**Predicate:** S01–S07, R01–R08, B01–B06, M01–M06 exist; links resolve; source/
static evidence supports claims.

1. Run four declared documentary structural checks.
2. Run git diff --check and inspect changed paths.
3. Report static validation only; never Cargo/tests/runtime/cold review.

**Stop:** structural/navigation/scope failure. **Recovery:** repair owned artifact
only. **Evidence:** command results, revision, paths. [Procedure index](#m02)


<a id="m04"></a>
## M04 — Predicates and evidence matrix

| Class | Predicate | Evidence | Stop / recovery |
|---|---|---|---|
| Sign/verify | JCS/signing/check sequence and caller responsibility compared | R03, B03 | Compatibility/execution: bounded evidence |
| Evidence | Fallback and three checks enumerated | R04, INV-003, B03 | Provider/audit: hand off |
| Key/region | Fields/mappings and lifecycle boundary stated | R04, INV-004, B03 | Secret/rotation/deploy: operator evidence |
| Consumers | Alias/imports separate from graph | R06, B04, B06 | Full graph/build: owner |
| Docs | Four structural checks and diff pass | PROC-005 | Repair owned docs |

<a id="m05"></a>
## M05 — Recovery limits

| Boundary | Recovery | Evidence | Do not assume |
|---|---|---|---|
| Payload/signature | Pause compatibility conclusion | Consumer/stored-format plan | Structs prove history compatibility |
| Evidence | Obtain provider/audit evidence | KMS/audit contract | Hash proves destruction/completeness |
| Key/region | Transfer lifecycle decision | Authorized key/region/rotation evidence | Local types prove custody/deployment |
| Persistence/service | Transfer runtime owner | R2/D1/endpoint evidence | Errors/comments prove operation |
| Operational request | Stop static procedure | Authorized execution plan | Docs authorize recovery |

<a id="m06"></a>
## M06 — Handoff and explicit unknowns

Handoff: mode, predicate, stop/recovery, source/manifest, baseline, paths, four
documentary results, git diff --check, and unknowns: features/targets, consumers,
compatibility, external libraries, key/seed custody, KMS/audit, R2/D1,
rotation, public verification service, regional deployment/routing, secrets,
runtime, deployment, publication, cold review.

Author documentary validation is not build, test, semantic approval, runtime
observation, or cold review.

[Reference](REFERENCE.md#r01) · [Relations](BLAST_RADIUS.md#b01) · [Start](#m01)
