---
name: own-corelink-rate-headers
description: Maintain the source-scoped ownership record for corelink-rate-headers without representing its contracts as executed HTTP, provider, D1, or runtime evidence.
metadata:
  evidence-set: rate-headers-source-static-20260920
  source-commit: 38f43b6d6b7f461edbd3a9bdd5cff34f2fd64018
  manifest: crates/corelink-rate-headers/Cargo.toml
  package: corelink-rate-headers
  profile: S
  evidence: source-static
---

# Own corelink-rate-headers

Use this S-profile skill only for the static ownership boundary of
`corelink-rate-headers`. It records source contracts and declared test
surfaces; it neither runs nor certifies Cargo, HTTP, D1, provider, deployment,
or production behavior.

[Scope](#s01) · [Evidence](#s02) · [Contracts](#s03) · [Invariants](#s04) ·
[Relations](#s05) · [Quality](#s06) · [Handoff](#s07)

<a id="s01"></a>
## S01 — Activation

Own only `crates/corelink-rate-headers/Cargo.toml`, its local `src/` and
checked-in test sources, plus the four ownership artifacts. The crate root
declares modules and public re-exports; it is not evidence of an HTTP response,
provider, D1 operation, routing, deployment, or a running circuit.

<a id="s02"></a>
## S02 — Territory and authority

Own only the package manifest, root/modules, migration include declaration, and checked-in tests. Contract authority remains with this package's source; the verified OKF profile remains canonical for cross-cutting policy. Class claims SOURCE, DOCUMENTARY, or UNKNOWN. No test, deployment, or runtime evidence is implied.

<a id="s03"></a>
## S03 — Reading and routing

Read `REFERENCE.md` for the RFC/header surface (`RateLimitHeaders`, `RateLimitPolicy`, builder,
error body, five-arm kind) and circuit/fake surface
(`GlobalCircuitBreaker`, states, audit and metric traits, and in-memory
implementations). Do not turn source comments or types into a claim that a
middleware, HTTP service, storage provider, or migration execution exists.

<a id="s04"></a>
## S04 — Decisions and invariants

Preserve falsifiable source predicates: five kind literals, builder clamps for
remaining and retry-after, three state variants, at-least-two signal trip
predicate, deterministic half-open sampling, and trait-plus-in-memory test
seams. A predicate is reported only with its named source location and a
concrete textual falsifier.

<a id="s05"></a>
## S05 — Workflow

Describe each relation as a single source arrow with its evidence and limit.
The local root-to-module route, billing re-export, workspace declaration, and
resilience-harness reference are static relations. Do not infer caller
compilation, invocation, HTTP emission, or a cross-package runtime graph.

<a id="s06"></a>
## S06 — Stop conditions

Stop when the claim requires resolved features, compilation, executed tests, emitted HTTP, provider/D1 activity, runtime or deployment evidence; record UNKNOWN and route to the responsible operator.

| Condition | Action | Required evidence | Stop when |
|---|---|---|---|
| Header source or root export changes | Re-record the exact local type, literal, bound, renderer, and root path. | Named `headers.rs` and `lib.rs` text. | The requested conclusion needs an emitted HTTP response or client result. |
| Circuit, audit, or metric source changes | Re-record the state/trait/helper arrow and its falsifier. | Named local module text and import/re-export text. | The requested conclusion needs a provider, D1 action, persisted audit, metric backend, or runtime transition. |
| Consumer reference changes | Make one directed relation per manifest dependency, re-export, or source mention. | Exact consumer manifest or source path. | Compilation, invocation, compatibility, or reachability is required. |
| Artifact validation is requested | Run only the supplied document checker and whitespace check, then state their documentary result. | Checker JSON and `git diff --check` output. | A semantic approval, test, network, deploy, or independent-review claim is requested. |

<a id="s07"></a>
## S07 — Evidence and output

Success: changed claims are falsifiable and evidence-linked. Completeness: inventory includes manifest targets, local modules, test targets, migration and material consumer references. Quality: no invented runtime claim or copied OKF policy. DoD: all four artifacts and documentary gates reconcile; independent review remains a separate verdict. Output source pin, paths, API/INV/REL/PROC IDs, checks and unknowns.
