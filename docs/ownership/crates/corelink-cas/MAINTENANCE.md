---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-cas
manifest: crates/corelink-cas/Cargo.toml
source_commit: 12ca4a8d1afed61c7fdd3312ede3dffc17665c47
profile: H
state: author_validated
evidence_set: cas-static-graph-20260920
---

# corelink-cas — maintenance

Source-guided maintenance modes for a hybrid facade. They support SOURCE and
static document-check evidence only; they neither prescribe nor report Cargo,
test, provider, storage, worker, runtime, deployment, or publication activity.

[Baseline](#m01) · [Classification](#m02) · [Local change](#m03) · [Facade change](#m04) · [Runtime escalation](#m05) · [Record](#m06)

<a id="m01"></a>
## M01 — Static baseline mode

**Mode:** source inspection. **Prerequisites:** fixed revision, manifest,
`src/lib.rs`, and changed paths. **Expected predicate:** every affected public
path has a recorded classification. **Stop:** package identity, revision, or
path inventory differs. **Recovery:** refresh the map at the selected baseline.
**Evidence:** SHA and exact repository paths; no execution result is implied.

<a id="m02"></a>
## M02 — Ownership classification mode

**Mode:** public-surface analysis. **Prerequisites:** affected path and its
route module. **Expected predicate:** it is classified as local implementation,
dependency facade, or worker facade. **Stop:** the review assumes a re-export
transfers implementation ownership or proves migration. **Recovery:** trace the
defining package/source and keep facade and provider conclusions separate.
**Evidence:** `src/lib.rs`, forwarding module, and manifest dependency.

<a id="m03"></a>
## M03 — Local-contract change mode

**Mode:** compatibility and invariant analysis. **Prerequisites:** a change in
one of the six local module families. **Expected predicate:** the top-level
module, impacted submodules, public exports, tests, and examples have been
identified. **Stop:** compatibility needs historical persisted data, a selected
target, or a live D1/R2/Worker result. **Recovery:** retain the source findings
and request the missing consumer or runtime evidence. **Evidence:** changed
local source plus related `tests/` or `examples/` paths; they are unrun unless
separately authorized and executed.

<a id="m04"></a>
## M04 — Facade-contract change mode

**Mode:** forwarding-boundary analysis. **Prerequisites:** an affected
`eviction`, `r2_multipart`, `meta`, `handler`, `r2_storage`, or `cache` path.
**Expected predicate:** the canonical path and its defining provider are both
recorded. **Stop:** the requested change needs provider behavior, feature
resolution, storage semantics, or consumer compatibility without its owner.
**Recovery:** hand off the provider-specific question while preserving the
facade/API impact. **Evidence:** route source and direct manifest dependency.

<a id="m05"></a>
## M05 — Composition and external-effect mode

**Mode:** boundary escalation. **Prerequisites:** a request involving R2,
storage, cache, worker execution, remote data, credentials, deployment, or
provider operation. **Expected predicate:** the required composition root and
runtime/provider owner are named. **Stop:** runtime or durable-state proof is
required. **Recovery:** use the responsible owner’s approved procedure; a
source revert is not an external rollback. **Evidence:** separately supplied
operator/runtime record, absent from this artifact.

<a id="m06"></a>
## M06 — Handoff record mode

**Mode:** reporting. **Prerequisites:** static work is ready for another owner.
**Expected predicate:** report baseline, classification, source paths/symbols,
affected canonical paths, static checks actually run, and explicit unknowns.
**Stop:** a conclusion exceeds source/static evidence. **Recovery:** restate it
at the correct ownership boundary and request missing proof. **Evidence:**
repository paths and SHA; author validation is not cold review.

[Reference](REFERENCE.md#r01) · [Blast radius](BLAST_RADIUS.md#b01) · [Back to baseline](#m01)
