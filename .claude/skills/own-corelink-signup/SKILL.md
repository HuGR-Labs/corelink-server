---
name: own-corelink-signup
description: Review source-static contracts for the corelink-signup orchestrator, its traits, fakes, and chaos-test source.
metadata:
  schema: "corelink-ownership/1.1"
  package: "corelink-signup"
  manifest: "crates/corelink-signup/Cargo.toml"
  source-commit: "6be030999de1f0e0fe62d3a9abb04ec2a4fefde6"
  profile: S
  evidence-set: "signup-static-source-20260920"
---

# Ownership — corelink-signup

This skill governs static Rust contracts, in-memory fakes, and chaos-test source in `corelink-signup`. It is not evidence of an identity, signup, billing, provider, network, persistence, deployment, or runtime result. The verified OKF route is [Signup auto-provision](../../../docs/knowledge/flows/signup-auto-provision.md); that separate concept is a routing destination, not evidence for claims below.

[Baseline](#s01) · [Boundary](#s02) · [Surface](#s03) · [Axioms](#s04) · [Relations](#s05) · [Evidence](#s06) · [Handoff](#s07).

<a id="s01"></a>
## S01 — Fix the static baseline

**Condition:** an ownership or source-contract review is requested. **Action:** record commit `6be030999de1f0e0fe62d3a9abb04ec2a4fefde6`, inspect the manifest, `src/lib.rs`, the relevant local module, and applicable test source. **Evidence/mode:** SOURCE_STATIC; inspected text only. **Stop:** the request needs a compiled target, an adapter, an external system, or a runtime observation.

<a id="s02"></a>
## S02 — Preserve the ownership boundary

**Condition:** a change crosses a collaborator trait or fixture. **Action:** keep `SignupOrchestrator` and the `AtomicSignupStore`, `BillingClient`, `SignupAuditSink`, and `ProvisionRecord` contracts distinct from implementations outside this package. **Evidence/mode:** [R02](../../../docs/ownership/crates/corelink-signup/REFERENCE.md#r02), SOURCE_STATIC. **Stop:** do not infer that a trait is implemented, selected, called, or backed by any service.

<a id="s03"></a>
## S03 — Route a contract change

**Condition:** request, outcome, store, audit, billing-client, or orchestration source changes. **Action:** identify the exported symbol, one falsifiable axiom in [R05](../../../docs/ownership/crates/corelink-signup/REFERENCE.md#r05), and its atomic relation in [B01–B06](../../../docs/ownership/crates/corelink-signup/BLAST_RADIUS.md#b01). **Evidence/mode:** source and unexecuted test-source text. **Stop:** a comment or fake cannot be promoted to behavior evidence.

<a id="s04"></a>
## S04 — Keep representation contracts explicit

**Condition:** a public type, trait method, enum variant, label, or constructor changes. **Action:** enumerate the exact Rust shape, including `#[non_exhaustive]` where present, and update [R03](../../../docs/ownership/crates/corelink-signup/REFERENCE.md#r03) and its B relation. **Evidence/mode:** SOURCE_STATIC. **Stop:** compatibility for callers, wire data, stored data, or any external protocol requires separate evidence.

<a id="s05"></a>
## S05 — Separate fake and chaos evidence

**Condition:** a source-local fake or chaos test is cited. **Action:** name the fixture and assertion as test source, retaining its unexecuted status. **Evidence/mode:** `InMemoryAtomicSignupStore`, `FailingAtomicSignupStore`, `InMemorySignupAuditSink`, `FailingSignupAuditSink`, `InMemoryBillingClient`, `StripeOutageBillingClient`, and package test files; SOURCE_STATIC. **Stop:** fixtures and assertions do not prove concurrency, persistence, availability, or a provider outcome.

<a id="s06"></a>
## S06 — Use evidence modes honestly

**Condition:** a conclusion is written or handed off. **Action:** label manifest/module/fixture/test text as SOURCE_STATIC and label all execution, resolved-graph, integration, provider, and runtime matters UNKNOWN. **Evidence/mode:** [R06](../../../docs/ownership/crates/corelink-signup/REFERENCE.md#r06) and [B06](../../../docs/ownership/crates/corelink-signup/BLAST_RADIUS.md#b06). **Stop:** route verified architecture material through the linked OKF concept only; do not reproduce or reinterpret it here.

<a id="s07"></a>
## S07 — Deliver bounded documentation

**Condition:** the four ownership artifacts are ready. **Action:** report baseline, paths, affected symbols, axiom/relation IDs, evidence mode, five unknown groups, four profile-S checker verdicts, and `git diff --check`. **Evidence/mode:** DOCUMENTARY. **Stop:** a documentary pass is not a build, test, runtime result, cold review, deployment, or approval.
