---
schema: corelink-ownership/1.1
document: reference
package: corelink-billing-stripe-traits
manifest: crates/corelink-billing-stripe-traits/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-billing-stripe-traits-structural-normalization-20260921
---

# corelink-billing-stripe-traits — reference

Source-static reference for the one-file, leaf trait surface. Claims below describe Rust source and manifest text only.

[Scope](#r01) · [Surface](#r02) · [Ports](#r03) · [Representations](#r04) · [Invariants](#r05) · [Dependencies and consumers](#r06) · [Boundaries](#r07) · [Unknowns](#r08).

<a id="r01"></a>
## R01 — Scope and evidence

Record index: [INV-001](#inv-001) · [INV-002](#inv-002) · [INV-003](#inv-003) · [INV-004](#inv-004)

The package is `corelink-billing-stripe-traits`; `src/lib.rs` is its sole source file in the inspected package. Its manifest declares `serde`, `serde_json`, `blake3`, and `hex`, and no dependency whose name begins `corelink-`. The source forbids unsafe code and defines the shared port, identity, record, outcome, error, and response surface.

Evidence class is SOURCE: `crates/corelink-billing-stripe-traits/{Cargo.toml,src/lib.rs}`. This is not a resolved graph, build, or runtime observation.

<a id="r02"></a>
## R02 — Public-surface map

| Surface | Source-defined members |
|---|---|
| Classification and envelope | `CanonicalWebhookEventType`; `StripeWebhookEnvelope` |
| Identity and dedup outcome | `IdempotencyToken`; `IdempotencyOutcome`; `IdempotencyStore` |
| Materialization | `MaterializerError`; `StateMaterializer` |
| Audit | `AuditRecord`; `AuditOutcome`; `AuditEmitter` |
| SLI | `SLI_BILLING_STRIPE_EVENT_SECONDS`; `SliObservation`; `SliRecorder` |
| Dispatch response | `DispatchResponse` |

All listed public enums and structs except `IdempotencyToken` are marked `#[non_exhaustive]` in `src/lib.rs`. `IdempotencyToken` is a public tuple struct without that attribute; the four traits are not enums or structs and do not carry it.

<a id="r03"></a>
## R03 — Port contracts

`IdempotencyStore: fmt::Debug + Send + Sync` supplies `try_insert(&self, token: IdempotencyToken, event_type: CanonicalWebhookEventType, now_ms: u64) -> Result<IdempotencyOutcome, String>`.

`StateMaterializer: fmt::Debug + Send + Sync` supplies these methods, each returning `Result<(), MaterializerError>`: `on_subscription_deleted`, `on_subscription_updated`, `on_invoice_paid`, `on_invoice_payment_failed`, `on_charge_dispute_created`, and `on_charge_refunded`. Every method receives `&StripeWebhookEnvelope`; the last has a source-defined default returning `Ok(())`.

`AuditEmitter: fmt::Debug + Send + Sync` supplies `emit(&self, record: &AuditRecord) -> Result<(), String>`. `SliRecorder: fmt::Debug + Send + Sync` supplies `observe(&self, obs: SliObservation)`.

These signatures are Rust source APIs. Comments describe intended dispatch behavior, but this record does not establish an executing dispatcher, HTTP mapping, provider, or storage behavior.

<a id="r04"></a>
## R04 — Representations and outcomes

`CanonicalWebhookEventType::classify` maps ten listed string literals to named variants and all other strings to `Unknown`; `label`, `is_state_mutator`, and `sla_event_types` expose the corresponding source-defined classification helpers. `StripeWebhookEnvelope` derives `Deserialize` and has public `id: String`, renamed `event_type: String`, defaulted `data: serde_json::Value`, and defaulted `created: u64` fields. This does not demonstrate compatibility with any external event format.

`IdempotencyToken` wraps `[u8; 32]`; `from_event_id`, `as_bytes`, and `to_hex` are public. Its custom debug formatter emits an eight-character hexadecimal prefix followed by `...`, according to the source.

`AuditRecord::new` constructs its seven public fields: event name, Stripe event id, canonical type, audit outcome, optional token hex, timestamp, and optional detail. `SliObservation::new` constructs metric name, seconds, event type, and outcome. `AuditOutcome::label` maps its seven variants to lowercase underscore labels. `DispatchResponse::status_code` maps its five variants to 200, 400, 401, 422, or 500. `MaterializerError` has `Transient(String)` and `InvalidPayload(String)` variants and implements `Display` and `std::error::Error`.

<a id="r05"></a>
## R05 — Falsifiable invariants

<a id="inv-001"></a>
### INV-001 — First-party leaf invariant

`Cargo.toml` declares no dependency entry whose package name begins `corelink-`. This is falsified by adding such an entry to any dependency section of this package manifest.

<a id="inv-002"></a>
[↩](#r01)
### INV-002 — Taxonomy cardinality and fallback

`sla_event_types()` returns ten non-`Unknown` values, and `classify` maps an unmatched string to `Unknown`. This is falsified by a differing array cardinality, mapping, or fallback in `src/lib.rs`.

<a id="inv-003"></a>
[↩](#r01)
### INV-003 — Port supertraits and signatures

The four trait supertrait bounds and method signatures stated in R03 are present. This is falsified by a changed bound, method name, argument type/order, or result type.

<a id="inv-004"></a>
[↩](#r01)
### INV-004 — Explicit constructor seams

`AuditRecord` and `SliObservation` are non-exhaustive public structs with public `new` constructors carrying the source-defined field sets. This is falsified by removing non-exhaustive marking, a constructor, or a stated constructor parameter.
[↩](#r01)

<a id="r06"></a>
## R06 — Dependency and consumer evidence

The manifest invariant in INV-001 is an outbound dependency fact. Static manifests declare direct package edges from `corelink-stripe-real`, `corelink-billing-stripe-materializer`, and `corelink-billing` to this package. In source, `corelink-stripe-real/src/webhook_dispatch.rs` publicly re-exports the trait surface at the compatibility module path, and `corelink-billing/src/stripe.rs` publicly re-exports it under `stripe::traits`. Materializer production modules `audit.rs`, `handler.rs`, and `idempotency.rs` import named symbols from this crate.

This is not a complete reverse graph, selected-feature result, or proof that every manifest edge imports the surface.

<a id="r07"></a>
## R07 — Failure and ownership boundaries

| Boundary | Source-defined observation | Not established here |
|---|---|---|
| Dedup port | `try_insert` returns `Result<IdempotencyOutcome, String>` | A backend, durable row, retry, or delivery outcome |
| Materializer port | Six named hooks return `MaterializerError` | Provider implementation, D1 behavior, or state transition |
| Audit and SLI ports | `emit` can return `String`; `observe` returns unit | Sink delivery, metric registration, or observability operation |
| Envelope and responses | Rust fields, enums, helpers, and numeric status mapping | External wire, Stripe, HTTP route, or protocol compatibility |
| Re-export paths | Source aliases in Stripe-real and billing | Ownership transfer, universal consumer compatibility, or migration completion |

<a id="r08"></a>
## R08 — Explicit unknowns

This work does not establish selected Cargo features or targets, a full consumer graph, whether every direct dependency imports this surface, API compatibility for existing callers, concrete trait implementations, storage schemas, D1 behavior, Stripe delivery, serialization/wire compatibility, FFI, metric operation, route mounting, secrets, runtime reachability, deployment, or cold review.

[Relations](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#r01)
