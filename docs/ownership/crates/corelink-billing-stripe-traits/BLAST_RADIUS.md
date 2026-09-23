---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-billing-stripe-traits
manifest: crates/corelink-billing-stripe-traits/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-billing-stripe-traits-structural-normalization-20260921
---

# corelink-billing-stripe-traits — blast radius

Static map of a source-defined trait surface. Relations separate direct manifest declarations, source imports, and re-exports from unverified execution.

[Scope](#b01) · [Outbound](#b02) · [Ports](#b03) · [Static consumers and aliases](#b04) · [Representation effects](#b05) · [Coverage](#b06).

<a id="b01"></a>
## B01 — Scope and reading rule

Record index: [REL-001](#rel-001) · [REL-002](#rel-002) · [REL-003](#rel-003) · [REL-004](#rel-004) · [REL-005](#rel-005) · [REL-006](#rel-006) · [REL-007](#rel-007) · [REL-008](#rel-008) · [REL-009](#rel-009) · [REL-010](#rel-010) · [REL-011](#rel-011)

The package owns the abstractions in `src/lib.rs`, not concrete adapters or consumers. A change can affect Rust callers that select these traits or representations even though the crate has no declared first-party dependency. Every relation below is SOURCE evidence unless marked as a manifest declaration.

<a id="b02"></a>
## B02 — Outbound dependency boundary

<a id="rel-001"></a>
### REL-001 — External representation dependencies

**Dependency:** `corelink-billing-stripe-traits` → `serde`, `serde_json`, `blake3`, `hex`.
**Flow:** the envelope derives deserialization; the token and helper methods name the remaining dependencies.
**Impact:** a change to exposed representation or token helpers may require source-level compatibility assessment.
**Evidence:** `Cargo.toml`; `src/lib.rs`.
**Stop:** this does not identify resolved versions, enabled features, external-library behavior, or runtime use. [Relation index](#b03)

<a id="rel-002"></a>
### REL-002 — No first-party outbound edge

**Dependency:** no `corelink-*` dependency is declared by this package.
**Flow:** the package can expose shared ports without this manifest naming another CoreLink package.
**Impact:** adding a first-party dependency changes the documented leaf and cycle-risk boundary.
**Evidence:** `crates/corelink-billing-stripe-traits/Cargo.toml`.
**Stop:** the manifest does not prove whole-workspace acyclicity or enforcement. [Relation index](#b03)

<a id="b03"></a>
## B03 — Port impact relations

<a id="rel-003"></a>

### REL-003 — Idempotency port implementors and callers

**Dependency:** implementors/callers → `IdempotencyStore`, `IdempotencyToken`, `IdempotencyOutcome`.
**Flow:** callers supply a token, canonical event type, and millisecond value; implementors return a typed outcome or string error.
**Impact:** changing the supertraits or `try_insert` signature can affect implementations and call sites that select it.
**Evidence:** `src/lib.rs`.
**Stop:** no particular backend, dedup result, or delivery path is proved. [Relation index](#b03)

<a id="rel-004"></a>
### REL-004 — State materialization port implementors and callers

**Dependency:** implementors/callers → `StateMaterializer`, `StripeWebhookEnvelope`, `MaterializerError`.
**Flow:** the six named hooks take a borrowed envelope and return a materializer result.
**Impact:** renaming a hook or changing its default/result type can affect implementors and dispatching callers.
**Evidence:** `src/lib.rs`.
**Stop:** the trait does not establish concrete state mutation, D1 operation, or event delivery behavior. [Relation index](#b03)

<a id="rel-005"></a>
### REL-005 — Audit port implementors and callers

**Dependency:** implementors/callers → `AuditEmitter`, `AuditRecord`, `AuditOutcome`.
**Flow:** callers provide a borrowed record; implementors return unit or a string error.
**Impact:** record fields, outcome labels, or `emit` shape can affect Rust source consumers.
**Evidence:** `src/lib.rs`.
**Stop:** comments about audit failure are not evidence of a configured sink or durable audit event. [Relation index](#b03)

<a id="rel-006"></a>
### REL-006 — SLI port implementors and callers

**Dependency:** implementors/callers → `SliRecorder`, `SliObservation`, `SLI_BILLING_STRIPE_EVENT_SECONDS`.
**Flow:** callers pass an observation by value to `observe`.
**Impact:** changing the observation fields, constant text, or trait signature can affect source callers.
**Evidence:** `src/lib.rs`.
**Stop:** no metric backend, registration, label cardinality, or observed measurement is established. [Relation index](#b03)

<a id="b04"></a>
## B04 — Static consumers and aliases

<a id="rel-007"></a>

### REL-007 — Stripe-real compatibility re-export

**Dependency:** `corelink-stripe-real` → `corelink-billing-stripe-traits`.
**Flow:** `webhook_dispatch.rs` publicly re-exports the named surface; `lib.rs` publicly re-exports those names from its webhook-dispatch module.
**Impact:** a changed source symbol can affect callers using the retained `corelink_stripe_real::webhook_dispatch::*` path.
**Evidence:** `crates/corelink-stripe-real/{Cargo.toml,src/lib.rs,src/webhook_dispatch.rs}`.
**Stop:** this does not show that all callers build, that aliases are wire-compatible, or that concrete dispatcher implementation ownership moved. [Relation index](#b03)

<a id="rel-008"></a>
### REL-008 — Materializer direct trait-surface use

**Dependency:** `corelink-billing-stripe-materializer` → `corelink-billing-stripe-traits`.
**Flow:** `audit.rs`, `handler.rs`, and `idempotency.rs` directly import named ports and types from this crate.
**Impact:** source changes to those selected symbols may require materializer source compatibility review.
**Evidence:** `crates/corelink-billing-stripe-materializer/{Cargo.toml,src/audit.rs,src/handler.rs,src/idempotency.rs}`.
**Stop:** the materializer manifest also names other packages; this relation does not certify target selection, tests, D1, or runtime operation. [Relation index](#b03)

<a id="rel-009"></a>
### REL-009 — Billing umbrella re-export

**Dependency:** `corelink-billing` → `corelink-billing-stripe-traits`.
**Flow:** `stripe.rs` re-exports this package under `corelink_billing::stripe::traits`.
**Impact:** public representation or port changes may affect callers selecting the umbrella alias.
**Evidence:** `crates/corelink-billing/{Cargo.toml,src/stripe.rs}`.
**Stop:** this does not prove consumer migration or that all umbrella exports are compatible. [Relation index](#b03)

<a id="b05"></a>
## B05 — Representation effects

<a id="rel-010"></a>

### REL-010 — Event classification and labels

**Dependency:** Rust callers → canonical event and outcome labels.
**Flow:** classifier, label methods, and the fixed ten-element array expose named variants and string values.
**Impact:** changed names, mappings, labels, or cardinality may affect source code comparing those values.
**Evidence:** `src/lib.rs`.
**Stop:** this is not evidence of a Stripe event taxonomy, stored-data format, or external protocol contract. [Relation index](#b03)

<a id="rel-011"></a>
### REL-011 — Token, record, observation, and response shapes

**Dependency:** Rust callers → `IdempotencyToken`, `AuditRecord`, `SliObservation`, `DispatchResponse`, and `MaterializerError` APIs.
**Flow:** constructors and helper methods expose fixed argument/result shapes; public fields are readable despite non-exhaustive construction restrictions.
**Impact:** changing a field, constructor, error variant, or status-code helper can affect direct source consumers.
**Evidence:** `src/lib.rs`.
**Stop:** no persistence schema, HTTP server mapping, serialization compatibility, or external effect is inferred. [Relation index](#b03)

<a id="b06"></a>
## B06 — Coverage and explicit unknowns

This map covers the leaf manifest invariant, four port interfaces, representations defined in the sole source file, three direct consumer manifest declarations, materializer's three observed direct imports, and three observed public re-export sites: `corelink-stripe-real/src/webhook_dispatch.rs`, `corelink-stripe-real/src/lib.rs`, and `corelink-billing/src/stripe.rs`. It does not cover reverse transitive consumers, feature-gated imports, every source alias, resolved dependency selection, implementations, data migration, D1, Stripe, FFI, metrics, runtime behavior, deployment, or cold review.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#b01)
