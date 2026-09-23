---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-privacy
manifest: crates/corelink-privacy/Cargo.toml
source_commit: 8b800acd3ffb042e5f68bedbb989415a5c5b0bbe
profile: H
state: draft
evidence_set: privacy-source-static-20260920
---

# corelink-privacy — maintenance

Source/static procedures for the hybrid privacy umbrella. They authorize neither
Cargo execution nor tests, network access, deployment, credentials, data access,
or independent-review claims.

[Baseline](#m01) · [Classification](#m02) · [Local modules](#m03) ·
[Facades](#m04) · [Consumers](#m05) · [Handoff](#m06)

<a id="m01"></a>
## M01 — Confirm the baseline

**Mode:** STATIC_SOURCE. **Prerequisite:** requested revision and changed paths
are known. **Predicate:** the manifest and root agree with the recorded package
boundary. **Procedure:** record the SHA, inspect `Cargo.toml`, `src/lib.rs`, and
the changed family. **Stop/recovery:** stop on a different or ambiguous tree;
recover by obtaining the intended source snapshot. **Evidence:** SHA and paths read.

<a id="m02"></a>
## M02 — Classify local versus facade paths

**Mode:** STATIC_SOURCE. **Prerequisite:** a public module or dependency changes.
**Predicate:** every affected path is marked local or external before ownership is
reported. **Procedure:** compare R02 with the root declarations and `pub use`
files. **Stop/recovery:** stop if a facade is treated as an implementation move;
recover by naming the defining package and retaining its behavior as unknown.
**Evidence:** [R02](REFERENCE.md#r02), `src/lib.rs`, and `Cargo.toml`.

<a id="m03"></a>
## M03 — Review absorbed local modules

**Mode:** STATIC_SOURCE. **Prerequisite:** the changed local family is known.
**Predicate:** changed root, contract anchor, local submodule, public export, and
direct local relation are traceable.

**Procedure:** trace the relevant family among `breach`, `consent`, `notice`,
`residency`, `sub_processor`, and `dpa::versioning`; record the changed trait,
schema, state/decision function, its R03 invariant row, and the affected B01/B02
relation. Check audit-before-mutation and the family-specific exception (notably
breach dispatch behavior) at source.

**Stop/recovery:** stop if a conclusion needs an external component or policy
decision; recover by handing off that exact gap.
**Evidence:** [R03](REFERENCE.md#r03); [B01](BLAST_RADIUS.md#b01);
[B02](BLAST_RADIUS.md#b02).

<a id="m04"></a>
## M04 — Review facade paths

**Mode:** STATIC_SOURCE. **Prerequisite:** `dsr`, `dsr::statuspage`, `erasure`,
`pseudonymize`, or `dpa::acceptance` changes. **Predicate:** the module's `pub use`
and manifest edge agree with the reported external owner. **Procedure:** inspect
the matching facade file and dependency declaration; trace the corresponding
B03–B05 relation. **Stop/recovery:** stop if the requested conclusion concerns the
defining package's implementation or execution; recover by routing to that owner.
**Evidence:** [R04](REFERENCE.md#r04); [B03](BLAST_RADIUS.md#b03)–[B05](BLAST_RADIUS.md#b05).

<a id="m05"></a>
## M05 — Reconcile static consumers

**Mode:** STATIC_GRAPH. **Prerequisite:** a public path, export, or dependency
changes. **Predicate:** direct static references are distinguished from unproven
consumer categories. **Procedure:** inspect root manifest declarations and source
imports, then record every located relation and the incomplete census boundary.
For a local trait/type/schema change, use the R03/B02 family inventory to select
the contract anchor and relevant source-adjacent test file.
**Stop/recovery:** stop where a complete, generated, feature-selected, or external
consumer graph is needed; recover with separately recorded graph evidence.
**Evidence:** [R07](REFERENCE.md#r07); [B06](BLAST_RADIUS.md#b06).

<a id="m06"></a>
## M06 — Record and hand off

**Mode:** STATIC_HANDOFF. **Prerequisite:** source review or ownership-document
change is complete. **Predicate:** baseline, classifications, paths, relations,
documentary checks, and unknowns are recorded. **Procedure:** report R/B/M IDs,
the source files inspected, the four structural check results, `git diff --check`,
and unresolved boundaries. **Stop/recovery:** stop if a structural result is being
presented as execution or cold review; recover by restating it as documentary
evidence only. **Evidence:** final diff, checker output, SHA, and this record.

[Reference](REFERENCE.md#r01) · [Blast radius](BLAST_RADIUS.md#b01) ·
[Guide](../../../../.claude/skills/own-corelink-privacy/SKILL.md#s01)
