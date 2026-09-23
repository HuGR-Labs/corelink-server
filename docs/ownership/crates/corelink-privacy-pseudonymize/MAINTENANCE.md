---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-privacy-pseudonymize
manifest: crates/corelink-privacy-pseudonymize/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-privacy-pseudonymize-structural-normalization-20260921
---

# corelink-privacy-pseudonymize — maintenance

These are SOURCE-static procedures. They neither run nor authorize Cargo, tests, a privacy workflow, salt access, KMS/vault activity, data mutation, deployment, publication, or recovery against a live system.

[Preparation](#m01) · [Selection](#m02) · [Procedures](#m03) · [Checks](#m04) · [Recovery limits](#m05) · [Handoff](#m06)

<a id="m01"></a>
## M01 — Prepare a bounded source review

Record index: [PROC-001](#proc-001) · [PROC-002](#proc-002) · [PROC-003](#proc-003) · [PROC-004](#proc-004) · [PROC-005](#proc-005)

Work at the recorded revision in a scoped checkout. Record porcelain status, the four ownership paths, `Cargo.toml`, and `src/lib.rs` before evaluating a change. Inspect no secret files or actual salt material.

**Stop:** package identity, baseline, or writable scope differs. **Recovery:** stop without altering source or unrelated paths; obtain the correct scoped baseline. **Evidence:** revision, status, and inspected source paths.

<a id="m02"></a>
## M02 — Select a source-static procedure

| Condition | Procedure mode | Predicate | Stop |
|---|---|---|---|
| SHA-256 input/order/shape changes | [PROC-001](#proc-001) STATIC_DERIVATION | Exact ordered inputs and output shape are compared | Stored/wire compatibility is required |
| Marker constants or struct changes | [PROC-002](#proc-002) STATIC_MARKER | Literal values and serde-visible fields are enumerated | A backend write/redaction claim is required |
| Verify behavior changes | [PROC-003](#proc-003) STATIC_VERIFY | Re-derivation and `ct_eq` call are retained or revised explicitly | Timing or live correlation is required |
| Consumer effect is requested | [PROC-004](#proc-004) STATIC_GRAPH | Manifest/re-export edges are separated from unknowns | A full graph or selected build is required |
| Documentation is ready | [PROC-005](#proc-005) STATIC_HANDOFF | Four records, links, and scope predicates hold | A structural check or diff predicate fails |

<a id="m03"></a>
## M03 — Procedures

<a id="proc-001"></a>
### PROC-001 — Assess a derivation representation change

**Mode:** STATIC_DERIVATION. **Prerequisites:** target symbol and current/proposed shape are known. **Predicate:** the exact SHA-256 update order, 32-byte salt, 32-byte wrapper, and 64-character hex expectation are compared.

1. Read R03 and B01.
2. Record changes to `pseudonymize`, `pseudonymize_subject_id`, `PseudonymHash`, or relevant constants.
3. Preserve stored, serialized, and caller compatibility as unknown unless separately evidenced.

**Stop:** a change requires a migration, real data inspection, or consumer acceptance conclusion. **Recovery:** escalate to the owning data/consumer boundary. **Evidence:** source symbols, before/after representation, relation, and explicit unknowns. [Procedure index](#m02)

<a id="proc-002"></a>

### PROC-002 — Assess a marker construction change

**Mode:** STATIC_MARKER. **Prerequisites:** changed constant, field, derive, or constructor is identified. **Predicate:** `pii_redacted`, `true`, and the digest-to-hex mapping are either preserved or each change is listed.

1. Read R04 and B02.
2. Inspect `PseudonymizationMarker::from_hash` and serde derives.
3. State only that a value can be constructed in-process.

**Stop:** the requested conclusion says a record was updated, PII was removed, or an irreversible operation completed. **Recovery:** obtain provider/data-path evidence; do not infer it from this helper. **Evidence:** source symbols and the explicitly bounded claim. [Procedure index](#m02)

<a id="proc-003"></a>

### PROC-003 — Assess a verification change

**Mode:** STATIC_VERIFY. **Prerequisites:** target verify input/output behavior is named. **Predicate:** the candidate UUID re-derives through `pseudonymize_subject_id`, and final 32-byte equality uses `ct_eq`, unless a reviewed contract intentionally changes either fact.

1. Read R05 and B03.
2. Compare function signature, delegation, and compared byte arrays.
3. Record that code inspection is not a timing test or authorization result.

**Stop:** a request needs benchmark data, secret material, KMS/vault access, or a live re-correlation operation. **Recovery:** leave this source-static procedure and seek authorized, separately scoped evidence. **Evidence:** exact function/branch and non-operational input description. [Procedure index](#m02)

<a id="proc-004"></a>

### PROC-004 — Reconcile static consumers and aliases

**Mode:** STATIC_GRAPH. **Prerequisites:** a changed public symbol is known. **Predicate:** direct manifest edges and the selected re-export paths in B04 are reported separately from all unselected consumers.

1. Inspect root membership/workspace dependency and the two manifests named in B04.
2. Inspect the selected re-export modules.
3. Keep complete reverse dependencies, feature selections, and compatibility unknown.

**Stop:** a conclusion requires every consumer, a selected target, or runtime reachability. **Recovery:** obtain appropriate graph/build/runtime evidence from the responsible owner. **Evidence:** searched paths, observed relationship, and coverage limit. [Procedure index](#m02)

<a id="proc-005"></a>

### PROC-005 — Hand off scoped documentation

**Mode:** STATIC_HANDOFF. **Prerequisites:** only the four assigned ownership artifacts changed. **Predicate:** S01–S07, R01–R08, B01–B06, and M01–M06 exist; cross-links resolve; scope and whitespace checks pass.

1. Run the four S-profile documentary structural checks, one per artifact.
2. Run `git diff --check` and inspect changed paths.
3. Report documentary validation only; do not call it a build, test, runtime, or cold-review result.

**Stop:** any checker, link, scope, or whitespace predicate fails. **Recovery:** repair only an owned artifact and rerun the documentary checks. **Evidence:** commands, verdicts, revision, and changed paths. [Procedure index](#m02)


<a id="m04"></a>
## M04 — Documentary validation matrix

| Check | Expected predicate | Evidence class |
|---|---|---|
| Skill structural check (S) | target exists with S01–S07 and `Condition`/`Action`/`Evidence`/`Stop` decision form | documentary |
| Reference structural check (S) | target exists with R01–R08 | documentary |
| Blast structural check (S) | target exists with B01–B06 | documentary |
| Maintenance structural check (S) | target exists with M01–M06 | documentary |
| `git diff --check` | no whitespace error | documentary |

These checks neither compile nor execute the package. A pass is not a security, legal, compatibility, deployment, runtime, or cold-review approval.

<a id="m05"></a>
## M05 — Recovery limits

| Boundary | Recovery action | Do not assume |
|---|---|---|
| SHA-256 helper contract | Pause for representation/consumer review | Existing stored or transmitted values remain compatible |
| Marker construction | Restore or revise the in-process shape with source review | A backend marker or PII change is rolled back |
| `ct_eq` call | Preserve or deliberately revise the source relation | A timing property is measured or a request is authorized |
| Salt/KMS/provider request | Escalate to its security/integration owner | This crate can obtain, rotate, or expose salt material |
| Ownership documents | Amend only owned docs and rerun M04 | Documentary checks approve a privacy operation |

<a id="m06"></a>
## M06 — Handoff and explicit unknowns

Handoff includes baseline, source and manifest paths, symbols/relations reviewed, changed paths, four S-check verdicts, `git diff --check`, and explicit unknowns. Required unknowns include salt generation/custody, secret handling, KMS/vault, consumer inputs, complete reverse graph, serialization/storage compatibility, marker persistence, any real pseudonymization/redaction/erasure action, authorization, timing measurements, deployment, publication, runtime reachability, and cold review.

[Reference](REFERENCE.md#r01) · [Blast radius](BLAST_RADIUS.md#b01) · [Ownership guide](../../../../.claude/skills/own-corelink-privacy-pseudonymize/SKILL.md#s01)
