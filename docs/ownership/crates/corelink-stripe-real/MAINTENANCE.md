---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-stripe-real
manifest: crates/corelink-stripe-real/Cargo.toml
source_commit: 16d9f0303d849a1ab3df14688bd2c7cdbfee8140
profile: S
state: author_validated
evidence_set: w011-stripe-real-source-static-20260920
---

# corelink-stripe-real — maintenance

These are SOURCE-static, evidence-aware procedures. They do not run or
authorize Cargo, tests, networking, credential handling, external effects,
deployment, or runtime recovery.

[Baseline](#m01) · [Target gates](#m02) · [Contracts](#m03) · [Fakes](#m04) · [Validation](#m05) · [Handoff](#m06)

<a id="m01"></a>
## M01 — Fix the static baseline

**Mode:** SOURCE_STATIC. **Prerequisite:** checkout is at
`16d9f0303d849a1ab3df14688bd2c7cdbfee8140` and the package manifest is the
stated target. **Predicate:** package identity, manifest path, and source
boundary match [R01](REFERENCE.md#r01). **Action:** record revision and inspect
the manifest plus named local source only. **Stop/recovery:** stop on another
revision or manifest; request the correct scoped baseline. **Evidence:**
`git rev-parse HEAD`; `crates/corelink-stripe-real/Cargo.toml`.

<a id="m02"></a>
## M02 — Reconcile target gates

**Mode:** SOURCE_TARGET. **Prerequisite:** a target predicate, target
dependency, or gated export changed. **Predicate:** [R02](REFERENCE.md#r02),
[AX-SR-01](REFERENCE.md#r05), and [B01](BLAST_RADIUS.md#b01) remain separate
native and wasm declarations. **Action:** compare manifest target blocks to
crate-root gates; update only the affected source record. **Stop/recovery:**
stop if target selection, compilation, linking, or execution is requested;
route that request to separately evidenced build ownership. **Evidence:**
manifest and crate-root text; SOURCE.

<a id="m03"></a>
## M03 — Reconcile a contract or axiom

**Mode:** SOURCE_CONTRACT. **Prerequisite:** a retry, verifier, dispatcher,
clock, portal, error, DLQ, port, or local fake changed. **Predicate:** each
changed claim names an R03–R07 predicate, an AX-SR axiom when applicable, and
one textual falsifier. **Action:** trace local declarations and control flow;
retain all five axioms or revise their exact source wording. **Stop/recovery:**
stop when a claim needs a caller, credential, audit service, persistence,
external response, or compatibility result; record it among
[R08](REFERENCE.md#r08)'s unknowns. **Evidence:** named `src/*.rs`; SOURCE.

<a id="m04"></a>
## M04 — Reconcile ports and fakes

**Mode:** SOURCE_ADAPTER. **Prerequisite:** an imported port, local trait,
injected dependency, or in-memory store/fake changed. **Predicate:** each of
the ten B05 rows remains atomic in [B05](BLAST_RADIUS.md#b05).

For B05, ten rows form five use/implementation pairs: each port class has one
dispatcher-use arrow and one separate local-implementation arrow, each with
its own boundary. Local state is not described as external persistence or
operation.

**Action:** map each changed declaration to its immediate implementation or
re-export and state the source limit. **Stop/recovery:** stop for credentials, network/external claims,
configuration, deployment, audit durability, or runtime requests; hand off the
exact unknown domain. **Evidence:** `src/{webhook_dispatch,dlq,portal,clock}.rs`;
SOURCE.

<a id="m05"></a>
## M05 — Validate documentary artifacts

**Mode:** DOCUMENTARY. **Prerequisite:** only the assigned skill and three
package documents changed and the supplied profile-S checker is available.
**Predicate:** S01–S07, R01–R08, B01–B06, and M01–M06 exist; links resolve;
the four structural checks and baseline whitespace diff succeed.

**Action:** run the supplied checker once for each kind (`skill`, `reference`,
`blast_radius`, `maintenance`) with profile `S`, then run `git diff --check
16d9f0303d849a1ab3df14688bd2c7cdbfee8140`. **Stop/recovery:** stop on
structural, link, scope, or whitespace failure; repair only the assigned four
paths. **Evidence:** four checker verdicts and diff output; documentary only.

<a id="m06"></a>
## M06 — Handoff and five unknowns

**Mode:** STATIC_HANDOFF. **Prerequisite:** M01–M05 are satisfied.
**Predicate:** handoff includes baseline, four changed paths, affected R/B/M
IDs, five axioms, five unknowns, four checker results, and diff result.
**Action:** state literal evidence modes and route to the verified
[SRE operations hub](../../../knowledge/ops/sre-operations-hub.md) only as an
OKF reference. **Stop/recovery:** stop if author checks are called execution,
external-effect, deployment, runtime, or cold-review proof; replace that
wording with the corresponding [R08](REFERENCE.md#r08) unknown. **Evidence:**
this artifact set and M05 outputs.

Success is a source-accurate procedure. Completeness is M01–M06 with a mode,
prerequisite, predicate, action, stop/recovery, and evidence for each. Quality
keeps local declarations/fakes distinct from external operation. Definition of
done is structural documentation validation plus a scope-limited whitespace
diff; it is not Cargo, test, network, deploy, external-effect, or runtime
evidence.

[Reference](REFERENCE.md#r01) · [Blast radius](BLAST_RADIUS.md#b01) · [Ownership guide](../../../../.claude/skills/own-corelink-stripe-real/SKILL.md#s01)
