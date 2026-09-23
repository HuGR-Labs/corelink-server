---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-billing-stripe
manifest: crates/corelink-billing-stripe/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-billing-stripe-structural-normalization-20260921
---

# corelink-billing-stripe — blast radius

[B01](#b01) · [B02](#b02) · [B03](#b03) · [B04](#b04) · [B05](#b05) · [B06](#b06)

Relations below are source/static boundaries. They do not prove runtime calls,
webhook routes, Stripe traffic, persistence, secrets, or a complete reverse
dependency graph.

<a id="b01"></a>
## B01 — Relation index

| Relation | Dependency direction | Source flow | Likely change impact |
|---|---|---|---|
| B02 aggregate input | stripe adapter → billing aggregator | `AggregatedCounter` → JCS bytes → local key | key/usage-request behavior |
| B03 emitted-type edge | stripe adapter → billing emit | imported upstream event types → aggregate-derived request | compile/type compatibility |
| B04 webhook verification | handler input → parser → HMAC/`ct_eq` → log | input rejection or local log outcome | signature/duplicate behavior |
| B05 audit ordering | adapter/handler → audit seam before local mutation | audit error → no following local mutation in that path | local fail-closed ordering |
| B06 public/fake surface | crate exports → static consumers (unknown set) | re-exports and traits → consumers | API compatibility |

<a id="b02"></a>
## B02 — Aggregator dependency and canonical-byte flow

**Direction:** this crate depends directly on `corelink-billing-aggregator`
according to its manifest. **Flow:** `AggregatedCounter` enters
`compute_canonical_aggregate_bytes`, then its JCS byte vector enters BLAKE3
derivation; `adapter.rs` consumes the aggregate to build a local request.
**Impact:** changing aggregate serialization, the type shape, or this
derivation can change local key outputs and ledger lookup behavior.
**Containment:** aggregator implementation ownership remains with its package;
the manifest and imports do not prove a scheduled aggregation, Stripe request,
or all reverse dependents.

<a id="b03"></a>
## B03 — Emitter manifest dependency and test-only imports

**Direction:** this crate directly declares `corelink-billing-emit` in its manifest. **Source classification:** the production adapter imports `AggregatedCounter` and accesses its event-kind data; it does not directly import emitter types. `IdemKey` and `UsageEventKind` imports appear in the `#[cfg(test)]` modules of `adapter.rs` and `idempotency.rs`, and the `prop_billing_stripe` integration-test target imports those types directly. **Impact:** changing those test fixture imports can affect the local test source, while a manifest or upstream type change can

affect package resolution or compilation when selected. **Containment:** this static edge and test-only imports do not prove an emitted-event flow, event emission, an HTTP request, or emitter implementation ownership. `corelink-analytics` is dev-dependency-only evidence and does not add a production flow.

<a id="b04"></a>
## B04 — Webhook input-to-log flow

**Direction:** `WebhookHandleRequest` feeds `StripeWebhookHandler`; the handler
calls the signature primitive and, after source-visible audit emissions,
`StripeWebhookLog::insert`. **Flow:** header/raw bytes/secret/caller-supplied
`now_ms` → parse → expected HMAC → candidate `ct_eq` accumulation → strict skew
test → in-memory or injected log outcome. **Impact:** changing parser acceptance,
HMAC concatenation, comparison, skew predicate, event-id key, or error shape
can alter rejection and duplicate behavior. **Containment:** a trait request
shape is not proof that any HTTP route supplies raw bytes, a secret, or a real
clock.

<a id="b05"></a>
## B05 — Audit-before-local-mutation relation

**Direction:** adapter and handler call `StripeAuditSink::emit` before their
respective local ledger/log mutation points. **Flow:** sink error propagates as
`StripeError::Audit`; a successful call permits the following source-local
operation. **Impact:** moving/replacing these calls can alter the code’s local
ordering and error propagation. **Containment:** the source does not establish
atomicity across any remote system, durable audit persistence, SIEM delivery,
or alert operation.

<a id="b06"></a>
## B06 — Public surface and unknown consumers

**Direction:** `src/lib.rs` re-exports local traits, fakes, types, helpers, and
the replay-window constant. **Flow:** a compile-time consumer may use those
paths. **Impact:** renaming, removing, or changing trait signatures, enum
variants, serialization, or error taxonomy may be source-compatible only after
consumer coordination. **Containment:** this review did not resolve a complete
reverse Cargo graph or execute consumer builds; do not name a runtime consumer
from textual mentions or comments alone.

[Reference](REFERENCE.md#r01) · [Ownership guide](../../../../.claude/skills/own-corelink-billing-stripe/SKILL.md#s01) ·
[Maintenance](MAINTENANCE.md#m01)
