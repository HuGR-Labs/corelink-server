---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-handler-cas
manifest: crates/corelink-handler-cas/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-handler-cas-structural-normalization-20260921
---

# corelink-handler-cas — maintenance

These procedures are source/static-only. They do not authorize Cargo execution,
network/provider calls, storage mutation, secrets, runtime testing, deployment,
publication, or independent-review approval.

[Mode](#m01) · [Intake](#m02) · [Contract](#m03) · [Fake](#m04) ·
[Validation](#m05) · [Handoff](#m06).

<a id="m01"></a>
## M01 — Mode and evidence boundary

| Item | Requirement |
|---|---|
| Mode | Inspect pinned manifest/source and validate ownership documents only |
| Prerequisites | Requested change, baseline, affected local paths, and companion ownership artifacts |
| Predicate | Each material statement is SOURCE evidence, a documentary-check result, or an explicit unknown |
| Stop | A request needs provider, HTTP, REAPI, wasm, audit-delivery, metric-delivery, or runtime proof |
| Recovery | Preserve scope and request evidence from the concrete provider/composition/operator owner |

Do not convert trait prose, comments, an in-memory fake, or a successful
document checker into operational evidence.

<a id="m02"></a>
## M02 — Intake and scope procedure

**Mode:** static intake. **Prerequisites:** expected baseline and assigned
package boundary. **Expected predicate:** `Cargo.toml` names
`corelink-handler-cas`; only its requested local surface and authorized
ownership artifacts are in scope.

1. Confirm baseline, branch, manifest, and changed-path list.
2. Read `lib.rs` and the affected modules before classifying a claim.
3. Record public trait/envelope changes separately from fake control-flow
   changes and external composition.
4. Stop if resolving the request needs a caller-wide compatibility decision or
   an external implementation.

**Evidence:** manifest, source paths, baseline, and scope-only diff. **Recovery:**
return to the assigned checkout or escalate the external boundary; do not edit
source or a shared registry to make scope appear complete.

<a id="m03"></a>
## M03 — Trait, envelope, and ordering procedure

**Mode:** static contract review. **Prerequisites:** affected request/error/
trait/audit/observer symbols and known direct relations. **Expected predicate:**
explicit namespace authorization, digest tag, response shape, probe fallback,
and claimed ordering are traced to code.

1. For a trait or envelope change, inspect `request.rs`, `error.rs`,
   `handler.rs`, and B01–B05.
2. Preserve the distinction between `exists` fallback and a provider override;
   retain batch order/cardinality requirements where applicable.
3. Trace every early return when changing audit or SLI control flow. Treat
   attempted/denied-before-mutation and committed-after-mutation separately.
4. Stop when compatibility, HTTP status, storage transactionality, or audit
   durability must be approved.

**Evidence:** named source branches and [R02–R04](REFERENCE.md#r02). **Recovery:**
freeze the public contract and hand the provider/composition question to its
owner; do not infer a successful remote rollback from source.

<a id="m04"></a>
## M04 — Digest and fake procedure

**Mode:** static fake/digest review. **Prerequisites:** `DigestAlgo` and any
affected helper or fake operation. **Expected predicate:** SHA-256 helper and
synthetic fake behavior stay explicitly distinguished.

1. Trace `expected_content_hash` for both tags and label `fake_hash` as
   length/first-byte/padding, not BLAKE3.
2. For reads, inspect injection, map lookup, `max_bytes`, audit, and SLI paths;
   do not assume a normal read rehashes stored bytes.
3. For writes/deletes/lists, trace mutation versus post-mutation audit failure
   independently.
4. Stop if a requested conclusion concerns R2/S3/KV, digest interoperability,
   REAPI, object metadata, or target execution.

**Evidence:** `src/{digest_algo,handler}.rs`, `src/lib.rs`, and
[R05–R07](REFERENCE.md#r05).
**Recovery:** request concrete implementation or protocol evidence; preserve
the source-defined in-memory-fake and temporary native wire-up boundary.

<a id="m05"></a>
## M05 — Documentary validation procedure

**Mode:** local documentation validation. **Prerequisites:** exactly the four
assigned ownership artifacts and the integration-supplied S-profile checker.
**Expected predicate:** one structural pass each for skill, reference, blast
radius, and maintenance; `git diff --check` passes; changed paths are only the
four assigned files.

1. Run the declared checker once for each artifact with its matching kind and
   profile `S`.
2. Run `git diff --check` and inspect the changed-path list.
3. Treat a checker failure as structural; repair only the authorized artifact.
4. Stop if repair requires source, manifest, shared index, status, or runtime
   work.

**Evidence:** actual checker output, exit statuses, and scope-only diff.
**Recovery:** retain the exact failure for handoff; a pass is not a build,
semantic approval, runtime observation, or cold review.

<a id="m06"></a>
## M06 — Handoff and definition of done

Handoff must state baseline, changed paths, affected public symbols and
reference/relation/procedure identifiers, four structural-check outcomes,
`git diff --check`, and explicit unknowns. Definition of done is the four
assigned artifacts with a scope-only, whitespace-clean diff and only actual
documentary results reported. Required unknowns include concrete handler
implementations, full consumer graph, compatibility, HTTP/REAPI, wasm, storage,
audit/SLI delivery, runtime, deployment, and cold review.

[Reference](REFERENCE.md#r01) · [Blast radius](BLAST_RADIUS.md#b01) ·
[Ownership guide](../../../../.claude/skills/own-corelink-handler-cas/SKILL.md#s01).
