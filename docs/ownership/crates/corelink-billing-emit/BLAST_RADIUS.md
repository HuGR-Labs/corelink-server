---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-billing-emit
manifest: crates/corelink-billing-emit/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-billing-emit-structural-normalization-20260921
---

# `corelink-billing-emit` blast radius

[B01](#b01) · [B02](#b02) · [B03](#b03) · [B04](#b04) · [B05](#b05) · [B06](#b06)

Static relationship map at baseline `9f372cc1f`. Relations below are SOURCE
evidence, not runtime reachability.

<a id="b01"></a>
## B01 — Internal flow

Dependency/flow: `event` supplies `UsageEvent` to idempotency and emitter;
emitter derives canonical bytes/key, calls the tracker, then branches to audit
and the in-memory sink. Direction matters: an accepted path audits before its
append, while duplicate/collision audit occurs after tracker mutation/decision.
Impact: changing event bytes, key derivation, tracker outcome, audit record, or
sink key changes adjacent stages. No atomic transaction spans those stages.

Evidence: `src/{event,idempotency,emitter,audit,sink}.rs`.

<a id="b02"></a>
## B02 — Idempotency relation

Dependency: the in-memory tracker depends on `UsageEvent` canonical bytes and
tenant UUID. Flow: `(tenant, idem_key, bytes)` enters the tracker; first sight
records it, same bytes reject as duplicate, divergent bytes error as collision.
Impact: changing canonical serialization or tenant/key identity can alter
replay classification. Unknown: durable, distributed, and rollback semantics.

Evidence: `src/idempotency.rs`.

<a id="b03"></a>
## B03 — Port/provider relation

Dependency: the package defines `R2UsageSink`, `BillingAuditSink`, and
`IdempotencyTracker` traits. Flow: only audit/idempotency are generic inputs to
the checked emitter; the sink concrete field is `InMemoryR2UsageSink`.
Impact: port changes can affect future adapters, but no real R2, audit, D1,
Queue, Cron, or provider is demonstrated by this relation. Provider ownership
remains with whichever adapter/composition supplies it.

Evidence: `src/{sink,audit,idempotency,emitter}.rs`.

<a id="b04"></a>
## B04 — Known static consumers

Direct manifest consumers found by exact `corelink-billing-emit =` search are
`corelink-billing`, `corelink-billing-aggregator`,
`corelink-billing-reconcile`, `corelink-billing-stripe`,
`corelink-runner-aggregate`, and `corelink-container`. Source samples show:
`corelink-billing` re-exports the full surface; aggregator and runner aggregate
import event types; container imports period/kind/type constants. The remaining
manifest edges are dependencies, not proof that each public API is used or
executed.

Evidence: those `Cargo.toml` files; `crates/corelink-billing/src/emit.rs`;
`crates/corelink-{billing-aggregator,runner-aggregate,container}/src/**`.

<a id="b05"></a>
## B05 — Analytics relationship

Dependency: `corelink-analytics` supplies `Region` used in `UsageEvent`.
Flow/impact: a `Region` type or serialization change can affect this event
shape. Unknown: metrics export, collector configuration, and external
analytics delivery; none follows from this direct dependency.

Evidence: `Cargo.toml`; `src/event.rs`.

<a id="b06"></a>
## B06 — Boundaries and unknowns

Static search is not a complete reverse graph. Unknowns include feature-gated
or generated consumers, consumer format compatibility, real provider behavior,
transactionality, route mounting, credentials, R2/Queue/Cron/D1 delivery, and
all runtime effects. Changes that need any unknown must be routed to the
consumer, provider, or composition owner with corresponding evidence.

Continue with [reference](REFERENCE.md#r01) and [maintenance](MAINTENANCE.md#m01).
