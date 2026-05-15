---
id: "AUDIT-2026-05-15-STRIPE-WEBHOOK-PRODUCTION"
type: "audit_report"
doc_status: "FROZEN"
audit_status: "AUDITED"
version: "1.1.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "s10", "billing", "stripe", "webhook", "production", "wave-15", "wave-16", "unification"]
---

# Stripe webhook production handler wire-up

> **Sprint:** GA wave 15 (R-prep) · **Sprint origin:** S-10 (Billing Pipeline)
> **Date:** 2026-05-15
> **Crate:** `corelink-stripe-real` (new `webhook_dispatch` module)
> **Integration test:** `crates/corelink-stripe-real/tests/webhook_e2e.rs`
> **Branch:** `wt/r-prep-stripe-webhook-prod`
> **Disposition:** Replaces the partially-fake dispatch arm wired in the
> `apps/server/src/webhook.rs` route. The route trait surface there is
> retained as the HTTP plumbing for axum; the production
> classification + idempotency + audit + SLI primitives now live in
> `corelink-stripe-real::webhook_dispatch` so non-axum callers
> (Cloudflare Worker, cron-triggered replay, scripts) share one
> canonical pipeline.

---

## Scope

S-10 (Billing Pipeline) shipped WI-S10-003 with signature verification
(constant-time, 5-min replay window) and the idempotency-key
primitives. The follow-on wave-15 task is to materialise the **real
production handler** that:

1. Verifies the Stripe signature over exact raw POST bytes.
2. Derives a stable BLAKE3 idempotency token from the Stripe event id.
3. Persists the token via INSERT-OR-IGNORE; second sight short-circuits
   with HTTP 200 and zero state mutation (idempotent retry safety).
4. Classifies the event into the canonical 10-element SLA-required
   taxonomy and dispatches to a typed `StateMaterializer`.
5. Emits `corelink.billing.stripe_event_processed.v1` audit row.
6. Records one `corelink_billing_stripe_event_seconds` SLI observation
   per dispatch (happy and sad paths both observed for SLO dashboards).
7. Returns canonical PCI DSS SAQ-A status codes
   (200 / 400 / 401 / 422 / 500) per `compliance_matrix.md`.

---

## Per-event-type dispatch table

The dispatcher recognises ten canonical Stripe event types per
S-10 sprint contract + WI-S10-003 §6.1.6 + `0018_stripe_idem_keys.sql`
event-type CHECK constraint. Five are **state mutators**; five are
**observability echoes** (acked + audited but reserved for S-13+ state
materialisation).

| # | Stripe `type` string                       | Canonical variant            | Materialiser method                  | Mutator? |
|---|--------------------------------------------|------------------------------|--------------------------------------|----------|
| 1 | `customer.subscription.deleted`            | `SubscriptionDeleted`        | `on_subscription_deleted`            | yes      |
| 2 | `customer.subscription.updated`            | `SubscriptionUpdated`        | `on_subscription_updated`            | yes      |
| 3 | `invoice.paid`                             | `InvoicePaid`                | `on_invoice_paid`                    | yes      |
| 4 | `invoice.payment_failed`                   | `InvoicePaymentFailed`       | `on_invoice_payment_failed`          | yes      |
| 5 | `charge.dispute.created`                   | `ChargeDisputeCreated`       | `on_charge_dispute_created`          | yes      |
| 6 | `customer.subscription.created`            | `SubscriptionCreated`        | (echo: audit only)                   | no       |
| 7 | `customer.subscription.trial_will_end`     | `SubscriptionTrialWillEnd`   | (echo: audit only)                   | no       |
| 8 | `charge.refunded`                          | `ChargeRefunded`             | (echo: audit only)                   | no       |
| 9 | `customer.created`                         | `CustomerCreated`            | (echo: audit only)                   | no       |
| 10 | `invoice.created`                         | `InvoiceCreated`             | (echo: audit only)                   | no       |
| —  | _any other string_                        | `Unknown`                    | acked 200 + audit `UnknownEventType` | no       |

The full 10-element list is exposed as
`CanonicalWebhookEventType::sla_event_types()` for SLO dashboards and
the e2e fixture.

---

## Pipeline (canonical sequence)

```text
POST /v1/billing/stripe-webhook
  1. Read Stripe-Signature header.                 -> 400 if missing.
  2. verify_webhook_signature(body, header, secret,
                              now_seconds, 300s).  -> 401 on failure.
  3. JSON parse envelope (id, type, data).         -> 422 on malformed.
  4. BLAKE3 token = blake3("stripe-event-id:" || id).
  5. INSERT OR IGNORE token into stripe_event_log.
       - AlreadyProcessed -> 200 + audit Duplicate. NO dispatch.
       - Transient error  -> 500 + audit MaterializerFailed.
       - FirstSight       -> proceed.
  6. Classify event_type; dispatch to materializer (state mutator) OR
     short-circuit (observability echo / Unknown).
  7. Emit corelink.billing.stripe_event_processed.v1 audit row.
       - Audit emit failure -> 500 (FAIL-CLOSED).
  8. Observe corelink_billing_stripe_event_seconds (every path).
  9. Return DispatchResponse -> HTTP status.
```

---

## Idempotency mechanism

- **Token derivation:** `BLAKE3-256(b"stripe-event-id:" || event_id)`
  with a domain-prefix so the hash family cannot collide with any
  other corelink BLAKE3 use (CAS, AC, audit chain).
- **Stable per event id:** identical event ids always yield identical
  32-byte digests (collision probability < 2^-128).
- **Storage:** `INSERT OR IGNORE` semantics — second sight returns
  `IdempotencyOutcome::AlreadyProcessed` and the dispatcher short-
  circuits with 200 OK and **zero** materialiser dispatch. Mirrors the
  D1 `stripe_event_log` PRIMARY KEY (stripe_event_id) UNIQUE
  constraint from migration `0018_stripe_idem_keys.sql`.
- **24h window:** Stripe holds retries for up to 3 days; the
  in-memory store keeps tokens for the process lifetime. Production
  wiring binds to the D1 table whose row TTL is 30 days (well past
  Stripe's retry horizon).

---

## Audit emission

- **One row per dispatch path** (Dispatched / Duplicate /
  SignatureInvalid / EnvelopeInvalid / MaterializerFailed /
  MaterializerInvalid / UnknownEventType).
- **Event name:** `corelink.billing.stripe_event_processed.v1`
  (CloudEvents-style). Stable for the v1 contract; any new fields
  ship as additive `.v2`.
- **Fail-CLOSED:** if the audit sink returns `Err`, the dispatcher
  flips its response to HTTP 500 so Stripe retries → next delivery
  hits the dedup row → resolves without re-dispatch. No orphan state.
- **Emit point:** exactly **once** per request, on every outcome
  arm. The integration test pins this with seven outcome variants
  covered.

---

## SLI

- **Metric:** `corelink_billing_stripe_event_seconds` (Prom histogram).
- **Labels:** `event_type` (canonical label string) and `outcome`
  (audit-outcome label string). Wired through the
  `SliRecorder::observe` trait so the production binder can target
  any registry without runtime label-map coercion.
- **Coverage:** emitted on every dispatch path — happy, duplicate,
  401, 400, 422, 500 — so SLO dashboards see latency for the
  sad-path arms too. Test
  `tests/webhook_e2e.rs::sli_emitted_on_every_path` pins five
  representative paths; the per-event-type round-trip test pins
  ten more.

---

## PCI DSS SAQ-A error envelope

Status-code shape per `compliance_matrix.md` PCI-DSS v4.0 SAQ-A row
(card-not-present merchant, all CHD outsourced to Stripe):

| Outcome                          | HTTP status |
|----------------------------------|-------------|
| Successful dispatch              | `200 OK`    |
| Duplicate (Stripe retry)         | `200 OK`    |
| Unknown event type (forward-compat) | `200 OK` |
| Missing `Stripe-Signature` header | `400 Bad Request` |
| HMAC mismatch / replay / future-dated | `401 Unauthorized` |
| Envelope JSON malformed          | `422 Unprocessable Entity` |
| Materialiser InvalidPayload      | `422 Unprocessable Entity` |
| Idempotency-store transient      | `500 Internal Server Error` |
| Materialiser Transient           | `500 Internal Server Error` |
| Audit sink failure (fail-CLOSED) | `500 Internal Server Error` |

---

## Charter compliance

- `#![forbid(unsafe_code)]` (crate-level, inherited).
- No `unwrap` / `expect` / `panic` / `indexing_slicing` in lib code
  (clippy `-D warnings` green).
- No `tokio` in `src/`; sync trait surface — async callers schedule
  body read and pass bytes in.
- All public enums (`CanonicalWebhookEventType`, `AuditOutcome`,
  `DispatchResponse`, `IdempotencyOutcome`, `MaterializerError`,
  `AuditRecord`, `SliObservation`) are `#[non_exhaustive]`.
- HMAC signature compare is constant-time via
  `subtle::ConstantTimeEq` (inherited from
  `webhook::verify_webhook_signature`, S-10 PRINC).
- Audit fail-CLOSED: emit BEFORE returning success; emit failure
  yields HTTP 500.
- Secret bytes stored in `Vec<u8>`, redacted in `Debug` impl.
- `wasm32`-clean: module lives behind the crate-level
  `cfg(not(target_arch = "wasm32"))` guard already in
  `corelink-stripe-real::lib`.

---

## Quality gates

| Gate | Command | Result |
|------|---------|--------|
| stripe-real build | `cargo build -p corelink-stripe-real` | green |
| server build | `cargo build -p corelink-server` | green |
| stripe-real clippy | `cargo clippy -p corelink-stripe-real --tests -- -D warnings` | green |
| stripe-real tests | `cargo test -p corelink-stripe-real` | 73 / 73 (51 lib unit + 4 webhook prop + 4 dlq prop + 2 portal prop + 4 webhook unit + 12 e2e) |
| spec validate | `python3 scripts/validate_specs.py` | 430 / 430 |
| migrations additive | `python3 scripts/check_migrations_additive.py` | 51 / 51 |

---

## Invariants pinned

- **INV-BILLING-NO-DUP** (HIGH; `invariant_registry §3.9`) — BLAKE3
  idempotency token + INSERT-OR-IGNORE dedup at the dispatcher gate.
  Property pinned by
  `idempotency_redelivery_is_no_op` and
  `idempotency_token_is_blake3_of_event_id`.
- **INV-BILLING-NO-LOSS** (HIGH; idem.) — every event id that
  signature-verifies and envelope-parses inserts exactly one dedup
  row; transient errors return HTTP 500 to drive Stripe retry.
- **INV-AUDIT-APPEND-ONLY** (CRITICAL, TLA+ proven Lote 6.2) — audit
  rows are emitted exactly once per dispatch path; emit failure
  fails the request CLOSED.

---

## What deliberately did NOT change (wave 15 baseline)

- No D1 migration changes. The dedup table from
  `0018_stripe_idem_keys.sql` already pins INV-BILLING-NO-DUP at the
  storage layer; this module mirrors the same `INSERT OR IGNORE`
  semantics in the in-memory test fake.

---

## Unification (wave 16 — v1.1.0 follow-on)

> **Branch:** `wt/r-prep-stripe-dispatcher-unify`
> **Disposition:** the axum HTTP shell in `apps/server/src/webhook.rs`
> now binds the canonical `WebhookDispatcher` end-to-end. The parallel
> trait surface previously retained for S-10 backward compatibility
> has been removed; there is exactly one canonical business pipeline
> in the codebase.

### Trait surface removed from `apps/server/src/webhook.rs`

The following types and traits — previously a parallel trait surface
in the apps/server crate — have been deleted. Their canonical
equivalents live in `corelink_stripe_real::webhook_dispatch`.

| Removed (apps/server)              | Canonical replacement (corelink-stripe-real)             |
|------------------------------------|----------------------------------------------------------|
| `trait SubscriptionStateHandler`   | `trait StateMaterializer`                                |
| `trait WebhookAuditSink`           | `trait AuditEmitter`                                     |
| `trait WebhookIdempotencyStore`    | `trait IdempotencyStore`                                 |
| `trait TimeProvider`               | `trait Clock`                                            |
| `enum CanonicalEventType` (7)      | `enum CanonicalWebhookEventType` (10 — SLA taxonomy)     |
| `enum WebhookAuditEventType` (6)   | `enum AuditOutcome` (7 — canonical)                      |
| `struct WebhookAuditRecord`        | `struct AuditRecord`                                     |
| `struct WebhookEnvelope`           | `struct StripeWebhookEnvelope`                           |
| `struct InMemoryWebhookAuditSink`  | `struct RecordingAuditEmitter`                           |
| `struct InMemoryIdempotencyStore`  | `struct InMemoryIdempotencyStore` (canonical, dispatcher)|
| `struct RecordingSubscriptionHandler` | `struct RecordingStateMaterializer`                   |
| `struct SystemTimeProvider`        | `struct SystemClock`                                     |
| `struct FixedTimeProvider`         | `struct FixedClock`                                      |
| `fn process_webhook(...)`          | `WebhookDispatcher::process(...)`                        |

**Eight removed trait/struct items** (the four traits plus four no-longer-needed
support enums/structs) — every event-type dispatch decision now flows
through the wave-15 canonical pipeline.

### What `apps/server/src/webhook.rs` still owns

Exactly the axum HTTP shell:

- `pub const STRIPE_WEBHOOK_ROUTE` — route path.
- `pub struct WebhookState { dispatcher: Arc<WebhookDispatcher> }` —
  thin wrapper holding the dispatcher constructed at server boot.
- `pub fn router(state) -> axum::Router` — mounts the route.
- `pub async fn stripe_webhook_handler(...)` — extracts the
  `Stripe-Signature` header + raw body, hands to
  `dispatcher.process(...)`, maps `DispatchResponse` to a
  `(StatusCode, &'static str)` via `dispatch_response_to_axum`.
- `pub fn dispatch_response_to_axum(...)` — canonical PCI DSS SAQ-A
  status-code mapping (200 / 400 / 401 / 422 / 500).

### Integration test

`apps/server/tests/webhook_unified.rs` exercises the end-to-end HTTP
shell via `tower::ServiceExt::oneshot`:

1. **`ten_canonical_event_types_round_trip_to_200`** — drives all 10
   SLA event types HTTP → dispatcher → 200, audit row carries
   `corelink.billing.stripe_event_processed.v1` + correct
   `canonical_event_type`.
2. **`idempotent_event_id_dispatches_once_audits_duplicate`** — POSTs
   the same event twice; HTTP returns 200, 200; materializer fires
   once; audit emits `Dispatched` then `Duplicate`; SLI observed on
   both deliveries.
3. **`bad_signature_returns_401_and_emits_signature_invalid_audit`** —
   HMAC-bogus header → HTTP 401 + audit `outcome=SignatureInvalid`
   carrying the canonical `event_name`.
4. **`missing_signature_header_returns_400`** — header absent → 400.
5. **`malformed_envelope_returns_422`** — signed garbage → 422 +
   `EnvelopeInvalid` audit.
6. **`state_mutators_route_to_correct_materializer_methods`** — the
   five state-mutators dispatch to their typed materializer method.
7. **`observability_echoes_audit_without_state_mutation`** — the five
   forward-compat echoes ack 200 + emit Dispatched without calling
   the materializer.

### Quality gates (wave 16)

| Gate | Command | Result |
|------|---------|--------|
| server build | `cargo build -p corelink-server` | green |
| server clippy | `cargo clippy -p corelink-server --tests -- -D warnings` | green |
| unified e2e | `cargo test -p corelink-server --test webhook_unified` | 7 / 7 |
| server total | `cargo test -p corelink-server` | 36 / 36 (23 lib unit + 6 byok orchestrator + 7 webhook_unified) |
| stripe-real preserved | `cargo test -p corelink-stripe-real` | 73 / 73 (unchanged from v1.0.0 baseline) |
| spec validate | `python3 scripts/validate_specs.py` | green |

### Wave-16 invariants reaffirmed

- **INV-BILLING-NO-DUP** — single dedup gate (BLAKE3 token) lives in
  the dispatcher; the HTTP shell carries no parallel dedup logic.
- **INV-BILLING-NO-LOSS** — the dispatcher is the sole entry point;
  audit emit happens on every code path including 401 / 422 / 500.
- **INV-AUDIT-APPEND-ONLY** — `corelink.billing.stripe_event_processed.v1`
  is emitted exactly once per delivery, fail-CLOSED. (Verified
  end-to-end by the unified integration suite.)

### Cloudflare Worker symmetry

The dispatcher is `tokio`-free, sync at the trait boundary, and
constructed from `Arc<dyn …>` collaborators. The CF Worker route
adapter (S-13+) instantiates the same `WebhookDispatcher` with
Worker-side `IdempotencyStore` / `StateMaterializer` / `AuditEmitter`
implementations — there is no parallel "axum vs Worker" classification
path to keep in sync.

---

## Follow-ups (non-blocking, post-unification)

- Bind the production D1-backed `IdempotencyStore` (replace the
  in-memory placeholder in `main.rs`).
- Bind the production `corelink-tier-selection` /
  `corelink-billing-*` writers as the `StateMaterializer` impl.
- Bind the production `corelink-audit-chain` sink as the
  `AuditEmitter` impl (currently uses `RecordingAuditEmitter` for
  dev/CI).
- Cloudflare Worker route adoption — the dispatcher is wasm-friendly
  (no tokio in `src/`); wire `WebhookDispatcher` into the
  `corelink-clerk-cf` crate as the Worker-side handler.
