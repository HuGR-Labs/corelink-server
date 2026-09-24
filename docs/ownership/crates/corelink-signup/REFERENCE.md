---
schema: corelink-ownership/1.1
document: reference
package: corelink-signup
manifest: crates/corelink-signup/Cargo.toml
source_commit: 6be030999de1f0e0fe62d3a9abb04ec2a4fefde6
profile: S
state: author_validated
evidence_set: signup-static-source-20260920
---

# corelink-signup — reference

SOURCE_STATIC reference for Rust source, fixtures, and test source. It makes no runtime claim. The verified OKF route is [Signup auto-provision](../../../knowledge/flows/signup-auto-provision.md); its independently scoped app-source evidence is not imported or revalidated here.

[Identity](#r01) · [Boundary](#r02) · [Surface](#r03) · [Source map](#r04) · [Axioms](#r05) · [Evidence](#r06) · [Relations](#r07) · [Unknowns](#r08).

<a id="r01"></a>
## R01 — Package identity and manifest

`crates/corelink-signup/Cargo.toml` names package `corelink-signup`. It declares `thiserror` as a normal dependency, `proptest` and `criterion` as development dependencies, three named test targets, and one benchmark target. This is a manifest-text observation, not a resolved dependency graph, build, or execution result.

<a id="r02"></a>
## R02 — Source-owned boundary

`src/lib.rs` exports source-local request, outcome, audit, store, billing, orchestration, identity-wrapper, region, and error modules. `SignupOrchestrator<A, S, B, P>` is parameterized by `SignupAuditSink`, `AtomicSignupStore`, `BillingClient`, and `ProvisionRecord`. The boundary recorded here is the Rust API plus fakes in this package; it does not establish any concrete implementation, dispatch, identity, signup, billing, provider, persistence, or runtime path.

<a id="r03"></a>
## R03 — Public source-contract map

| Area | Source-defined contract |
|---|---|
| Input and response | `SignupRequest`, `SignupResponse`, correlation/idempotency wrappers |
| Outcome vocabulary | `SignupOutcome`, `BillingIntent`, `OrchestrationStep`, their canonical helper lists |
| Orchestration | `SignupOrchestrator::new` and `provision`; generic collaborator bounds |
| Store port | `AtomicSignupStore`, `SignupTx`, row types, `StorageError` |
| Audit port | `SignupAuditSink`, record/event/error types |
| Client port | `BillingClient`, customer-id/error types |
| Fixtures | in-memory and failing store/audit/client fixtures plus `InMemoryProvisionRecord` |

`SignupRequest`, `SignupResponse`, `SignupOutcome`, `BillingIntent`, `OrchestrationStep`, and the listed error/event taxonomies are source representations. Their Rust text does not establish external compatibility.

<a id="r04"></a>
## R04 — Source and test-source map

| Path family | Static responsibility | Evidence mode |
|---|---|---|
| `src/lib.rs`, `request.rs`, `outcome.rs`, `error.rs` | exports and source-visible representations | SOURCE_STATIC |
| `src/orchestrator.rs` | generic orchestration algorithm and provision-record fake | SOURCE_STATIC |
| `src/store.rs`, `audit.rs`, `billing.rs` | collaborator ports and local fake/adversarial implementations | SOURCE_STATIC |
| `src/{correlation,idempotency,tenant,pat,region}.rs` | source-local wrapper and region representations | SOURCE_STATIC |
| `tests/prop_signup_orchestration.rs`, `tests/chaos_stripe_outage.rs`, `tests/mutation_kills.rs` | assertions expressed as test source | SOURCE_STATIC, NOT_EXECUTED |

<a id="r05"></a>
## R05 — Five falsifiable source axioms

| Axiom | Falsifiable observation | Source evidence |
|---|---|---|
| AX-01 collaborator seam | `SignupOrchestrator` retains its four generic collaborator bounds and `new` receives all four values | `src/orchestrator.rs` |
| AX-02 transaction vocabulary | `AtomicSignupStore` declares begin, four insert methods, commit, rollback, idempotency lookup, and committed-count inspection | `src/store.rs` |
| AX-03 idempotency representation | the source defines `Duplicate` with `signup_id` and `tenant_id`, and the store lookup returns `Option<(SignupId, TenantId)>` | `src/outcome.rs`, `src/store.rs` |
| AX-04 audit vocabulary | the audit event helper exposes exactly four listed `corelink.signup.*` strings and `SignupAuditSink::emit` receives a record reference | `src/audit.rs` |
| AX-05 fake/chaos separateness | source defines both in-memory/failing collaborators and the outage fixture; the chaos test source selects the outage fixture | `src/{store,audit,billing}.rs`, `tests/chaos_stripe_outage.rs` |

Each axiom is falsified by removal or alteration of the named declaration, signature, field, cardinality, or fixture/test-source selection. It is not a claim that the asserted behavior was executed.

<a id="r06"></a>
## R06 — Evidence modes

**SOURCE_STATIC** means manifest, Rust module, fixture, or test text was inspected at the recorded commit. **NOT_EXECUTED** applies to all test-source assertions. **UNKNOWN** covers resolved features and targets, complete consumers, implementations, compatibility, external systems, persistence, identity, signup, billing, provider outcomes, network, deployment, and runtime state. No stronger mode was collected.

<a id="r07"></a>
## R07 — Relation route

Read [B01](BLAST_RADIUS.md#b01) for manifest and facade scope; [B02](BLAST_RADIUS.md#b02) for request/outcome representations; [B03](BLAST_RADIUS.md#b03) for the collaborator seams; [B04](BLAST_RADIUS.md#b04) for store/fake relations; [B05](BLAST_RADIUS.md#b05) for audit/client/fake relations; and [B06](BLAST_RADIUS.md#b06) for coverage limits. Procedures are in [M01–M06](MAINTENANCE.md#m01).

<a id="r08"></a>
## R08 — Five explicit unknown groups

1. **Graph:** selected features/targets, reverse dependencies, and all callers.
2. **Implementations:** concrete port implementations and their compatibility with the traits.
3. **Data:** schemas, migrations, persistence, transactions, and rollback semantics outside fakes.
4. **Integration:** identity, signup, billing, provider, network, secrets, routing, and external formats.
5. **Operation:** execution results, concurrency, availability, observability delivery, deployment, and cold review.

[Blast radius](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01) · [Skill](../../../../.claude/skills/own-corelink-signup/SKILL.md#s01)
