---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-signup
manifest: crates/corelink-signup/Cargo.toml
source_commit: 6be030999de1f0e0fe62d3a9abb04ec2a4fefde6
profile: S
state: author_validated
evidence_set: signup-static-source-20260920
---

# corelink-signup — blast radius

Atomic SOURCE_STATIC relations for the package’s Rust declarations, fakes, and chaos-test text. None proves execution or an external behavior.

[Facade](#b01) · [Representations](#b02) · [Orchestrator](#b03) · [Store](#b04) · [Audit/client](#b05) · [Limits](#b06).

<a id="b01"></a>
## B01 — Manifest-to-facade relation

**Relation:** the package manifest names the crate and its direct dependencies; `src/lib.rs` publicly exposes the module surface. **Impact:** a manifest or re-export change can affect static Rust consumers selecting those names. **Falsifier:** remove or rename a declared dependency, module, or re-export. **Evidence/mode:** `Cargo.toml`, `src/lib.rs`; SOURCE_STATIC. **Unknown:** feature selection and all consumers.

<a id="b02"></a>
## B02 — Request-to-outcome representation relation

**Relation:** `SignupRequest` supplies wrappers and input fields to `SignupOrchestrator::provision`, which returns `SignupResponse` containing a `SignupOutcome`; `outcome.rs` defines outcome and step representations. **Impact:** changed fields, variants, or constructors can affect direct Rust callers. **Falsifier:** change the named parameter/result shape or an outcome field/variant. **Evidence/mode:** `src/{request,orchestrator,outcome}.rs`; SOURCE_STATIC. **Unknown:** caller compatibility, formats, and external input/output.

<a id="b03"></a>
## B03 — Orchestrator-to-collaborator relation

**Relation:** `SignupOrchestrator<A, S, B, P>` is bounded by the audit, store, client, and provision-record traits; `provision` selects their methods. **Impact:** a trait bound or method-signature change can affect static implementors and callers. **Falsifier:** alter a bound, selected method name, parameter, or result type. **Evidence/mode:** `src/orchestrator.rs`, `src/{audit,store,billing}.rs`; SOURCE_STATIC. **Unknown:** implementations, ordering under execution, and real effects.

<a id="b04"></a>
## B04 — Store-to-fake relation

**Relation:** `AtomicSignupStore` declares a transaction-shaped API; `InMemoryAtomicSignupStore` and `FailingAtomicSignupStore` are source-local implementations, and property-test source selects them. **Impact:** a changed row, step, store method, or failure-injection seam changes the static fake/test contract. **Falsifier:** remove a method, alter a row/step shape, or change a fixture selection in `prop_signup_orchestration.rs`. **Evidence/mode:** `src/store.rs`, `tests/prop_signup_orchestration.rs`; SOURCE_STATIC, NOT_EXECUTED. **Unknown:** durability, transaction semantics, and concurrency.

<a id="b05"></a>
## B05 — Audit/client-to-adversarial-fixture relation

**Relation:** `SignupAuditSink` and `BillingClient` are source ports; their in-memory/failing/outage fixtures are selected by package test source. **Impact:** changing record/client signatures, error variants, or fixture behavior can affect source consumers and assertions. **Falsifier:** alter `emit`, `create_customer`, a fixture result, or the fixture selected in chaos source. **Evidence/mode:** `src/{audit,billing}.rs`, `tests/{prop_signup_orchestration,chaos_stripe_outage}.rs`; SOURCE_STATIC, NOT_EXECUTED. **Unknown:** any audit delivery, provider operation, or outage behavior outside the fixture.

<a id="b06"></a>
## B06 — Coverage and non-relations

**Coverage:** B01–B05 cover manifest/facade declarations, representations, generic collaborator seams, source-local fakes, and named property/chaos test source. **Completion predicate:** every changed source contract identifies one B relation and one R05 axiom; otherwise the review is incomplete. **Quality rule:** source/fake/test text remains SOURCE_STATIC or NOT_EXECUTED.

The five unknown groups in [R08](REFERENCE.md#r08) are excluded: graph, implementations, data, integration, and operation. In particular, no relation certifies identity, signup, billing, provider, network, persistence, runtime, deployment, or cold review.

### Known reverse Cargo consumers

| Relation | Arrow and activation | Evidence | Unknown / limit |
|---|---|---|---|
| RC-001 | `e2e-signup-flow` manifest → this crate; `src/helpers.rs` imports signup types and uses `SignupOrchestrator` while building its harness. | `tests/e2e-signup-flow/Cargo.toml`; `tests/e2e-signup-flow/src/helpers.rs` | Harness construction is source evidence only; no test execution, production caller, or external effect is established. |
| RC-002 | `e2e-billing-flow` manifest → this crate; `src/harness.rs` imports signup types and composes its harness. | `tests/e2e-billing-flow/Cargo.toml`; `tests/e2e-billing-flow/src/harness.rs` | Harness construction is source evidence only; no test execution, production caller, or external effect is established. |

These are known direct manifest edges, not a complete reverse graph. Cargo
target/feature resolution and runtime activation remain unverified.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Skill](../../../../.claude/skills/own-corelink-signup/SKILL.md#s01)
