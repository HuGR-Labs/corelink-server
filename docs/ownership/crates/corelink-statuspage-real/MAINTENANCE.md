---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-statuspage-real
manifest: crates/corelink-statuspage-real/Cargo.toml
source_commit: 6be030999de1f0e0fe62d3a9abb04ec2a4fefde6
profile: S
state: author_validated
evidence_set: w010-statuspage-real-source-static-20260920
---

# corelink-statuspage-real — maintenance

These are SOURCE-static, evidence-aware procedures. They do not run or
authorize Cargo, tests, networking, Statuspage API activity, credentials,
deployment, scheduler execution, or runtime recovery.

[Baseline](#m01) · [Targets](#m02) · [Contracts](#m03) · [Adapters/fakes](#m04) · [Relations](#m05) · [Handoff](#m06)

<a id="m01"></a>
## M01 — Fix the static baseline

**Mode:** SOURCE_STATIC. **Prerequisite:** checkout is at `6be030999de1f0e0fe62d3a9abb04ec2a4fefde6` and the package manifest is the stated target. **Predicate:** package identity, manifest path, and source boundary match [R01](REFERENCE.md#r01). **Action:** record the revision and inspect the manifest plus named local source only. **Stop/recovery:** stop on another revision or manifest; recover by requesting the correct scoped baseline. **Evidence:** `git rev-parse HEAD`; `crates/corelink-statuspage-real/Cargo.toml`.

<a id="m02"></a>
## M02 — Reconcile a target split

**Mode:** SOURCE_TARGET. **Prerequisite:** a target predicate, target dependency, or gated export changed. **Predicate:** [R02](REFERENCE.md#r02), [AX-SP-01](REFERENCE.md#r05), [B01](BLAST_RADIUS.md#b01), and [B02](BLAST_RADIUS.md#b02) record native and wasm declarations independently. **Action:** compare `Cargo.toml` target blocks with `src/lib.rs` gates; update only the affected source record. **Stop/recovery:** stop if target selection, compilation, linking, or Worker behavior is requested; route that request to separately evidenced build/runtime ownership. **Evidence:** manifest and crate-root text; SOURCE.

<a id="m03"></a>
## M03 — Reconcile a public contract or axiom

**Mode:** SOURCE_CONTRACT. **Prerequisite:** trait, report, audit, limiter, retry, redaction, or bridge text changed. **Predicate:** each changed claim names one R03–R05 predicate and one textual falsifier. **Action:** trace declarations and control-flow locally; retain all five axioms or revise their exact source wording. **Stop/recovery:** stop when a claim needs caller compatibility, an audit service, data provenance, or an external response; recover by recording it among [R08](REFERENCE.md#r08)'s unknowns. **Evidence:** `src/{backend,report,audit,rate_limit,retry,redact,dsr_bridge}.rs`; SOURCE.

<a id="m04"></a>
## M04 — Reconcile adapter or fake declarations

**Mode:** SOURCE_ADAPTER. **Prerequisite:** native HTTP, wasm backend, or in-memory fake text changed. **Predicate:** implementation/fake relations remain atomic in B03–B05 and external-operation language remains absent. **Action:** map each changed implementation to its trait or bridge and state its source limit. **Stop/recovery:** stop for credential handling, external API assertions, network outcomes, deployment, scheduler, persistence, or runtime requests; hand off the exact unknown domain. **Evidence:** `src/{http,wasm32_backend,memory,audit}.rs`; SOURCE.

<a id="m05"></a>
## M05 — Validate documentary artifacts

**Mode:** DOCUMENTARY. **Prerequisite:** only the assigned skill and three
package documents changed and the supplied profile-S checker is available.
**Predicate:** S01–S07, R01–R08, B01–B06, and M01–M06 exist; links resolve;
the four structural checks and the baseline whitespace diff succeed.

**Action:** run the supplied checker once for each kind (`skill`, `reference`,
`blast_radius`, `maintenance`) with profile `S`, then run `git diff --check
6be030999de1f0e0fe62d3a9abb04ec2a4fefde6`. **Stop/recovery:** stop on
structural, link, scope, or whitespace failure; repair only the assigned four
paths. **Evidence:** four checker verdicts and diff output; documentary only.

<a id="m06"></a>
## M06 — Handoff and five unknowns

**Mode:** STATIC_HANDOFF. **Prerequisite:** M01–M05 are satisfied. **Predicate:** handoff includes baseline, four changed paths, affected R/B/M IDs, five axioms, five unknowns, four checker results, and diff result. **Action:** state literal evidence modes and route to the verified [SRE operations hub](../../../knowledge/ops/sre-operations-hub.md) only as an OKF reference. **Stop/recovery:** stop if author checks are called execution, API, deployment, runtime, or cold-review proof; replace that wording with the corresponding [R08](REFERENCE.md#r08) unknown. **Evidence:** this artifact set and M05 outputs.

Success is a source-accurate procedure. Completeness is M01–M06 with a mode,
prerequisite, predicate, action, stop/recovery, and evidence for each. Quality
keeps external adapter declarations and local fakes distinct from external
operation. Definition of done is structural documentation validation plus a
scope-limited whitespace diff; it is not Cargo, test, network, deploy, API, or
runtime evidence.

[Reference](REFERENCE.md#r01) · [Blast radius](BLAST_RADIUS.md#b01) · [Ownership guide](../../../../.claude/skills/own-corelink-statuspage-real/SKILL.md#s01)
