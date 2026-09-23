---
schema: corelink-ownership/1.1
document: reference
package: corelink-billing-stripe
manifest: crates/corelink-billing-stripe/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-billing-stripe-structural-normalization-20260921
---

# corelink-billing-stripe — ownership reference

[R01](#r01) · [R02](#r02) · [R03](#r03) · [R04](#r04) · [R05](#r05) · [R06](#r06) · [R07](#r07) · [R08](#r08)

Source/static reference. It does not establish a real Stripe API request,
webhook delivery, HTTP route binding, secret availability, receiver-clock
observation, production audit persistence, deployment, or a complete consumer
graph.

<a id="r01"></a>
## R01 — Package identity and evidence boundary

`crates/corelink-billing-stripe/Cargo.toml` declares package
`corelink-billing-stripe`. `src/lib.rs` declares and re-exports nine local
modules: adapter, audit, error, event, idempotency, ledger, signature, webhook,
and webhook_log. This document uses SOURCE evidence for code and static
manifest evidence for direct dependencies; it contains no EXECUTED_LOCAL,
DEPLOYMENT, or OBSERVED_RUNTIME claim.

<a id="r02"></a>
## R02 — Ownership split

| Surface | Source-visible owner | Boundary |
|---|---|---|
| Adapter traits, in-memory orchestration, local types/errors, signature primitive, audit/ledger/log seams | this crate | implementations appear under this crate’s `src/` |
| `AggregatedCounter` and its package implementation | `corelink-billing-aggregator` | this crate consumes it in `idempotency.rs` and `adapter.rs` |
| emitted billing event types | `corelink-billing-emit` | direct manifest dependency; `IdemKey` and `UsageEventKind` imports occur in `#[cfg(test)]` modules in `adapter.rs`/`idempotency.rs` and the `prop_billing_stripe` integration-test target |
| property/analytics target | test/development-only static edge | `corelink-analytics` is a dev-dependency, not evidence of an export or runtime flow |

The direct dependency declaration is a static edge, not a transfer of
implementation ownership and not proof that either package executes.

<a id="r03"></a>
## R03 — Public source surface

`src/lib.rs` re-exports `StripeBillingAdapter`, `StripeWebhookHandler`, the
in-memory adapter/handler/sinks/logs/ledger, key/event/request/decision types,
idempotency functions, signature functions and `REPLAY_WINDOW_MS`. It exposes
`stripe_schema_version() -> u32`, returning `18`. `lib.rs` forbids unsafe code
and denies missing docs. Public re-export availability is source-visible; it
does not prove a package consumer selects any symbol.

<a id="r04"></a>
## R04 — Canonical aggregate and idempotency contract

`compute_canonical_aggregate_bytes` calls `serde_jcs::to_vec` on
`corelink_billing_aggregator::AggregatedCounter`. `derive_idempotency_key`
passes those bytes to `derive_idempotency_key_from_canonical`, which BLAKE3
hashes into `IdempotencyKey([u8; 32])` (`src/idempotency.rs`).

`IdempotencyKey::to_hex` hex-encodes that array and `hex_len()` returns `64`
(`src/event.rs`). The local in-memory ledger uses the key as its `HashMap` key;
the exact same `UsageRecordRequest` returns `AlreadyExistsIdempotent`, while a
different request at that key returns `IdempotencyKeyReuse`
(`src/ledger.rs`). This is an implementation invariant of the local fake, not
evidence of a Stripe idempotency window or charge result.

<a id="r05"></a>
## R05 — Exact signature parser and verifier

`StripeSignatureHeader::parse` rejects an empty header, a missing `t=`, no
parsed `v1=`, non-`u64` timestamp, invalid hex, and a `v1` value not exactly 32
bytes. It trims comma-separated fields, keeps every valid `v1=` candidate, and
otherwise ignores unrecognized fields (`src/signature.rs`).

`compute_signature` initializes `Hmac<Sha256>` with the supplied bytes and updates it with the decimal timestamp bytes, `b"."`, then the supplied raw payload bytes. `verify_stripe_signature` parses first; calculates the expected tag; ORs `subtle::ConstantTimeEq::ct_eq` results across every candidate; rejects when no candidate matched; then computes `|now_ms - timestamp_seconds * 1000|` with saturating arithmetic. It returns `SignatureSkewRejected` only when that delta is strictly greater than `REPLAY_WINDOW_MS`, whose source value is `300_000` (`src/signature.rs`). Source verifies

the primitive’s code path, not external webhook semantics, clock correctness, secret provenance, or timing behavior of a deployed endpoint.

<a id="r06"></a>
## R06 — Adapter, ledger, and webhook-log contracts

`StripeBillingAdapter::record_usage` accepts an aggregate, subscription-item
identifier, and caller-supplied `now_ms`. `InMemoryStripeBillingAdapter` derives
the canonical key, constructs `UsageRecordRequest`, emits local audit before a
ledger record, and returns local decision variants (`src/adapter.rs`).

`StripeWebhookLog` inserts and reads a `WebhookEvent`; its in-memory
implementation maps `stripe_event_id` to `WebhookEvent` and reports either
`Inserted` or `AlreadyExists` (`src/webhook_log.rs`). `WebhookEvent` has a typed
kind, event id/timestamp, and `payload_redacted` bytes (`src/event.rs`). These
are trait/fake contracts, not proof of a database schema, event deserializer,
or remote delivery behavior.

<a id="r07"></a>
## R07 — Audit and webhook ordering

`StripeAuditSink::emit` is a local trait. `InMemoryStripeAuditSink` appends to
an `Arc<Mutex<Vec<StripeAuditRecord>>>`; `FailingStripeAuditSink` returns an
error (`src/audit.rs`). In `adapter.rs`, calls to `audit.emit` precede the local
ledger mutation paths. In `webhook.rs`, `WebhookReceived` is emitted before
verification; on success `SignatureVerified` is emitted before `log.insert`;
signature error arms emit their matching audit record before returning. Error
propagation is source-visible. There is no observed durable audit sink,
transaction, alert, external state, or real webhook route.

<a id="r08"></a>
## R08 — Explicit unknowns and handoff

Unknown: real Stripe endpoints and responses; HTTP client behavior; HTTP route
mounting; raw request-body capture; configured/provisioned secrets; trusted or
observed clock; key rotation in production; durable ledger/log/audit backends;
production error mapping/retry; deployment; and actual consumers. Requests for
those properties belong to the relevant integration, server composition,
secrets, observability, or runtime owner and need direct evidence beyond this
crate.

[Ownership guide](../../../../.claude/skills/own-corelink-billing-stripe/SKILL.md#s01) ·
[Blast radius](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01)
