---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-adapters-cloud
manifest: crates/corelink-adapters-cloud/Cargo.toml
source_commit: 1c99a5b8ce1db7b622db7811512432a5d070bbbe
profile: S
state: author_validated
evidence_set: w005-cloud-source-static-20260920
---

# corelink-adapters-cloud — maintenance guide

These procedures are bounded to source inspection and ownership-document validation. They do not authorize Cargo compilation, tests, wasm builds, Cloudflare actions, HTTPS/provider calls, credentials, migration, deployment, publication, or runtime operation. Every result must name its evidence class; this artifact currently supports SOURCE and static document-check evidence only.

[Mode](#m01) · [Intake](#m02) · [Path change](#m03) · [Boundary analysis](#m04) · [Validation](#m05) · [Handoff](#m06).

<a id="m01"></a>
## M01 — Mode, prerequisites, and safeguards

| Item | Requirement |
|---|---|
| Mode | Read source and validate documentation only |
| Baseline | Confirm the fixed commit and package manifest before interpreting evidence |
| Inputs | Requested facade change, exact local module paths, and companion ownership documents |
| Expected predicate | Every claim is SOURCE, static check result, or explicit unknown |
| Stop | Provider, target, build, migration, reverse-consumer, or runtime evidence is required |
| Recovery | Preserve the baseline and report the unmet evidence requirement |

Do not treat an upstream description, manifest dependency, `pub use`, or local test declaration as provider, target, build, or runtime evidence. The package’s owner artifacts also do not make the author an independent reviewer.

<a id="m02"></a>
## M02 — Intake and scope procedure

[PROC-001](#proc-001).

<a id="proc-001"></a>
### PROC-001 — Classify a facade change

**Mode:** SOURCE inspection. **Prerequisites:** fixed baseline, manifest, `src/lib.rs`, affected facade module, and requested target. **Expected predicate:** the change is classified as root module naming, direct re-export mapping, or upstream/selection work.

1. Confirm package identity and the fixed baseline.
2. Read [R03–R06](REFERENCE.md#r03) and the matching relation in [B02–B05](BLAST_RADIUS.md#b02).
3. Record the canonical facade path and matched direct dependency.
4. State separately the facade owner, upstream implementation owner, and target-selection boundary.
5. Mark all consumers and selected contexts not independently resolved as unknown.
6. Stop if the requested outcome needs an upstream behavior, target choice, build, migration, provider, or runtime result.

**Evidence:** manifest field and local source path. **Recovery:** classify the item as unknown and route it to the needed owner/evidence class. [Index](#m02)


<a id="m03"></a>
## M03 — Canonical path procedure

[PROC-002](#proc-002).

<a id="proc-002"></a>
### PROC-002 — Review a re-export path change

**Mode:** SOURCE inspection. **Prerequisites:** scoped diff, affected root declaration, affected local facade module, and known direct target. **Expected predicate:** the source path mapping and compatibility impact are explicit before a change is accepted.

1. Identify whether `cf`, `clerk`, `stripe`, `statuspage`, or `slack` changes.
2. Verify the root declaration and the module’s one direct glob re-export agree.
3. Trace the matching REL-001–005 and REL-006/007 records.
4. Treat module rename, removal, visibility reduction, or target-package swap as source compatibility risk.
5. Ask the upstream implementation owner to assess changed symbol behavior or contract semantics.
6. Require separately resolved consumer/migration evidence before claiming compatibility outcome.

**Evidence:** source declarations, manifest edge, and scoped diff. **Stop:** a target-specific, build, provider, migration, or runtime conclusion is requested. **Recovery:** preserve the existing facade mapping and report the unresolved compatibility question. [Index](#m03)


<a id="m04"></a>
## M04 — Boundary-analysis procedure

[PROC-003](#proc-003).

<a id="proc-003"></a>
### PROC-003 — Analyze an implementation or target request

**Mode:** source-boundary reasoning only. **Prerequisites:** request and affected facade path. **Expected predicate:** the request is routed without projecting upstream behavior onto the facade.

1. Confirm whether the request changes only the local import path. 2. If it concerns implementation semantics, identify the direct re-export target as the implementation boundary. 3. If it concerns feature or target availability, identify build/workspace plus consumer context as the selection boundary. 4. State that the facade does not choose a target or implement provider behavior. 5. Do not infer build success, migration completion, provider request, or runtime reachability.

6. Stop and request the correct owner or separately authorized evidence.

**Evidence:** R02, API-003, and REL-009–011. **Recovery:** retain the request as an unresolved cross-package/selection item; make no local operational claim. [Index](#m04)


<a id="m05"></a>
## M05 — Documentation validation procedure

[PROC-004](#proc-004).

<a id="proc-004"></a>
### PROC-004 — Validate the ownership artifacts

**Mode:** static local documentation validation. **Prerequisites:** exactly the four scoped ownership artifacts, an integration-supplied candidate checker, and a clean target scope. **Expected predicate:** each artifact passes its declared S-profile structural check and the diff has no whitespace or scope error.

1. Run the candidate checker once for the skill with kind `skill`, profile `S`, and repository root.
2. Run it once each for reference, blast radius, and maintenance with matching kinds/profile/root.
3. Run `git diff --check` on the scoped worktree.
4. Inspect the changed-path list for exactly the four authorized artifacts.
5. Correct only an owned artifact, then rerun its failed documentary check.
6. Stop if a required correction touches source, shared state, or an external operation.

**Evidence:** checker output, command exit statuses, and changed-path list. **Recovery:** hand off the exact structural failure; a checker pass is not semantic approval, an upstream build, or runtime proof. [Index](#m05)


<a id="m06"></a>
## M06 — Handoff, quality, and definition of done

The handoff must include baseline SHA, changed paths, affected API/invariant/relation/procedure IDs, facade-to-implementation mapping, target-selection boundary, checker commands and exit statuses, and explicit unknowns. Success means the package’s canonical import surface is documented from bounded source evidence. Completeness means M01–M05 provide mode, prerequisites, predicate, stop/recovery, and evidence for a facade maintenance decision. Quality means the facade, implementation owner, and target-selection boundary remain distinct.

Definition of done: all four artifacts exist in their assigned paths; each passes the candidate structural checker; `git diff --check` passes; and a scope-only diff shows no unauthorized path. These checks do not execute Rust, establish target selection or build output, prove consumer migration, perform Cloudflare or HTTPS/provider operations, or certify runtime behavior. Independent cold review remains required.

[Back to start](#m01)
