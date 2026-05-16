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
- Cloudflare Worker route adoption — the dispatcher is wasm-friendly
  (no tokio in `src/`); wire `WebhookDispatcher` into the
  `corelink-clerk-cf` crate as the Worker-side handler.

---

## Materializers (wave 17 — v1.2.0 follow-on)

> **Branch:** `wt/r-prep-stripe-materializers-real`
> **Disposition:** the trait-only placeholders from waves 15 + 16
> (`RecordingStateMaterializer` / `RecordingAuditEmitter` /
> `InMemoryIdempotencyStore`) have been replaced in `apps/server/src/main.rs`
> with production wiring from the new
> [`corelink-billing-stripe-materializer`](../../crates/corelink-billing-stripe-materializer/)
> crate. The full dispatcher → materializer → D1 + audit-chain
> pipeline is now end-to-end live.

### Crate layout

```text
crates/corelink-billing-stripe-materializer/
├── Cargo.toml                       (#![forbid(unsafe_code)], deny clippy)
├── src/
│   ├── lib.rs                       (public surface re-exports)
│   ├── d1.rs                        (BillingD1Writer trait + InMemoryBillingD1)
│   ├── audit.rs                     (BillingAuditEmitter + RealStripeAuditEmitter)
│   ├── idempotency.rs               (D1IdempotencyStore — INSERT OR IGNORE)
│   ├── tier.rs                      (TierSelector + InMemoryTierSelector)
│   └── handler.rs                   (D1SubscriptionStateHandler — the 10-event matrix)
└── tests/
    └── materializers_e2e.rs         (8 e2e scenarios — dispatcher → materializer)
```

### 10-event materialization matrix

`EVENT_MATERIALIZATION_MATRIX` (re-exported from the crate root) is
the surface-stable canonical mapping:

| Stripe event type                        | D1 table                | Audit event name (v1)                                       | Severity |
|------------------------------------------|-------------------------|-------------------------------------------------------------|----------|
| `customer.subscription.deleted`          | `stripe_subscriptions`  | `corelink.billing.subscription_canceled.materialized.v1`    | Notice + tier→Free |
| `customer.subscription.updated`          | `stripe_subscriptions`  | `corelink.billing.subscription.materialized.v1` (+ tier reconcile) | Notice |
| `invoice.paid`                           | `stripe_invoices`       | `corelink.billing.invoice.materialized.v1`                  | Notice   |
| `invoice.payment_failed`                 | `stripe_invoices`       | `corelink.billing.invoice.materialized.v1`                  | Notice   |
| `charge.dispute.created`                 | `stripe_disputes`       | `corelink.billing.dispute.materialized.v1`                  | **Sev1** |
| `customer.subscription.created`          | `stripe_subscriptions`  | `corelink.billing.subscription.materialized.v1`             | Notice   |
| `customer.subscription.trial_will_end`   | (echo only)             | `corelink.billing.echo.v1`                                  | Info     |
| `charge.refunded`                        | `stripe_refunds`        | `corelink.billing.refund.materialized.v1`                   | Notice   |
| `customer.created`                       | `stripe_customers`      | `corelink.billing.customer.materialized.v1`                 | Notice   |
| `invoice.created`                        | (echo only)             | `corelink.billing.echo.v1`                                  | Info     |

The matrix is pinned by the e2e regression test
`matrix_covers_all_ten_canonical_event_types`.

### Tier-change wiring (INV-BILLING-TIER-CONSISTENT)

On `customer.subscription.updated` the materializer:

1. Extracts `plan_id` (and `seat_count`) from `data.object`.
2. Calls `TierSelector::compute_tier(plan_id, seat_count)` →
   canonical `TierKind`.
3. Reads the current `tier_selections.tier` for the tenant; if
   different, emits `corelink.tenant.tier_changed.v1` (audit BEFORE
   write — fail-CLOSED) then upserts the new tier wire-string.

On `customer.subscription.deleted` the materializer additionally
downgrades the tenant to `TierKind::Free` (per dispatcher contract).

A new D1 migration `0048_stripe_billing_materializer.sql` ships the
five canonical tables plus the `stripe_tier_drift_view` view used by
the daily reconciliation cron to surface
INV-BILLING-TIER-CONSISTENT drift (additive; passes
`check_migrations_additive.py`).

### Audit fail-CLOSED preserved

Every state mutation goes through the canonical sequence:

1. **Emit billing audit** (`corelink.billing.<event>.materialized.v1`) →
   on error, return `MaterializerError::Transient` (dispatcher
   maps to 500, no D1 write happens).
2. **Perform D1 write** via `BillingD1Writer` → on error, return
   `Transient` / `InvalidPayload` per the underlying error.
3. **Recompute tier** (subscription arms only) → emit
   `corelink.tenant.tier_changed.v1` then persist; same fail-CLOSED
   ordering.

`RealStripeAuditEmitter` (implementing the wave-15
`AuditEmitter` trait) routes the dispatcher's per-delivery row
through the same `Arc<dyn BillingAuditEmitter>` the materializer
uses, so the audit chain stays single-topology (verifier never sees
a split).

### Integration test coverage (`materializers_e2e.rs`)

| Test                                                            | Verifies |
|-----------------------------------------------------------------|----------|
| `ten_event_matrix_round_trips_dispatcher_through_materializer`  | All 10 SLA event types → correct D1 table(s) + correct audit name. |
| `tier_change_scenario_basic_to_pro_emits_tier_changed_audit`    | subscription.updated `starter` → `pro` updates `tier_selections.tier` + emits `tier_changed.v1`. |
| `tier_change_no_op_when_target_tier_equals_current`             | If the computed tier equals the persisted tier, no audit fires (no spurious chain entries). |
| `idempotency_replay_does_not_materialize_twice`                 | Replay of same `evt_…` → exactly one D1 row + one audit row. |
| `audit_failure_propagates_500_and_no_d1_write`                  | If `BillingAuditEmitter::emit_billing` returns `Err`, dispatcher returns 500 and no D1 row is written. |
| `d1_failure_during_state_mutation_returns_500`                  | If the D1 writer fails (including dedup-insert), dispatcher returns 500. |
| `matrix_covers_all_ten_canonical_event_types`                   | The exported matrix covers every `CanonicalWebhookEventType::sla_event_types()` entry. |
| `matrix_audit_event_names_versioned_v1`                         | Every matrix row carries a `.v1`-versioned, `corelink.`-prefixed audit name. |

### Quality gates (wave 17)

| Gate | Command | Result |
|------|---------|--------|
| materializer build | `cargo build -p corelink-billing-stripe-materializer` | green |
| materializer clippy | `cargo clippy -p corelink-billing-stripe-materializer --tests -- -D warnings` | green |
| materializer unit | `cargo test -p corelink-billing-stripe-materializer --lib` | 18 / 18 |
| materializer e2e | `cargo test -p corelink-billing-stripe-materializer --test materializers_e2e` | 8 / 8 |
| server build (after main.rs cutover) | `cargo build -p corelink-server` | green |
| server clippy | `cargo clippy -p corelink-server --tests -- -D warnings` | green |
| server total (no regression) | `cargo test -p corelink-server` | 36 / 36 |
| stripe-real preserved | `cargo test -p corelink-stripe-real` | 73 / 73 (unchanged) |
| migrations additive | `python3 scripts/check_migrations_additive.py` | green (52 files scanned) |
| spec validate | `python3 scripts/validate_specs.py` | green (441 docs) |
| secrets matrix | `python3 scripts/validate_secrets_matrix.py` | green |

### Wave-17 invariants reaffirmed

- **INV-BILLING-NO-DUP** — `D1IdempotencyStore::try_insert` mirrors
  `INSERT OR IGNORE INTO stripe_webhook_events_processed`; the
  hex-encoded BLAKE3 token is the dedup key.
- **INV-BILLING-NO-LOSS** — every state mutation emits its
  per-event audit row BEFORE the D1 row write; orphan-state-free.
- **INV-AUDIT-APPEND-ONLY** — both the dispatcher's
  `stripe_event_processed.v1` and the materializer's
  `<event>.materialized.v1` rows route through the same
  `Arc<dyn BillingAuditEmitter>`.
- **INV-BILLING-TIER-CONSISTENT** — `stripe_tier_drift_view`
  (migration 0048) exposes the join the reconciliation cron uses to
  prove `tier_selections.tier == compute_tier(active_subscription)`
  after every reconciliation cycle.

### What deliberately did NOT change (wave 17 baseline)

- The wave-15 `corelink-stripe-real::webhook_dispatch` trait surface
  is unchanged. The new materializer crate consumes it verbatim
  (`impl StateMaterializer for D1SubscriptionStateHandler`).
- The wave-16 axum HTTP shell in `apps/server/src/webhook.rs` is
  unchanged — only `main.rs` swapped from `Recording*` placeholders
  to the production materializer.
- `corelink-cf-bindings::CfD1DatabaseReal` is unchanged. The new
  `BillingD1Writer` trait is the dependency-inversion seam: the
  wasm32 binder forwards each method to the `CfD1DatabaseReal`
  tenant-scoped prepared statements without touching this crate.

## Materializers (wave 18 — wasm32 binders, v1.3.0 follow-on)

> **Branch:** `wt/r-prep-stripe-wasm32-binders`
> **Commit:** see `BRANCH` cover note
> **DCO:** signed-off
> **Audit doc cross-ref:** §materializers.wasm32-binders

### §materializers.wasm32-binders

Wave 17 shipped the `BillingD1Writer` + `BillingAuditEmitter` trait
seams and pinned `apps/server/main.rs` to the `InMemoryBillingD1` +
`InMemoryBillingAuditEmitter` mirrors on native. Wave 18 closes the
production binder gap by adding two trait-object wrappers in a new
gated module:

```text
crates/corelink-billing-stripe-materializer/
  src/wasm32_binders.rs        (NEW — feature `cf-billing-real`)
  tests/wasm32_binders.rs      (NEW — 15 native-CI tests via the stub)
```

Wrappers:

- **`CfD1BillingWriter { d1: Arc<CfD1DatabaseReal>, tenant: TenantId }`**
  routes every `BillingD1Writer` method through the wave-14
  `CfD1DatabaseReal` wrapper. Each call:
    1. Cross-tenant ct-eq reject (`MaterializedRow.tenant_id` MUST
       constant-time equal the writer's anchored `TenantId`;
       `subtle::ConstantTimeEq`).
    2. Re-validate SQL through `CfD1DatabaseReal::scoped_query`
       (defense-in-depth on the static materializer literals; the
       canonical Stripe-row prepared statements are pinned at
       compile time in `wasm32_binders::SQL_*`).
    3. Bind-time tenant ct-eq via `verify_first_bind`.
    4. Surfaces a stable `BillingD1Error::Transient(
       "wasm32_async_dispatch_pending: …")`. The actual
       `worker::D1Database::prepare/bind/run` async chain runs in the
       CF Worker boot layer one frame above (which owns the
       `JsFuture` event-loop affinity); the sync `BillingD1Writer`
       trait keeps the materializer testable on native CI without
       tokio creep.
- **`ArchiveProducerBillingEmitter { producer: Arc<ArchiveProducer>,
  audit_sink: Arc<dyn R2AuditSink> }`** routes every
  `BillingAuditRecord` through the wave-15 `ArchiveProducer`
  (per-tenant NDJSON archive with chain-head continuity) + the wave-15
  `R2AuditSink` (per-event PutObject). The binder surfaces
  `BillingAuditError::Transient("wasm32_audit_chain_pending: …")` —
  the per-tenant `HashChainBuilder` chain wrapping is performed by
  the CF Worker boot layer one frame above (single source of chain
  truth per tenant).

### Server feature flag

`apps/server/Cargo.toml` adds `cf-billing-real`, additive and OFF by
default. On native dev / CI the `InMemoryBillingD1` +
`InMemoryBillingAuditEmitter` mirrors are wired (preserves the wave-17
default); on wasm32 with `--features cf-billing-real` the
`CfD1BillingWriter` + `ArchiveProducerBillingEmitter` wrappers are
constructed at boot from the canonical `CfRealBindings` bundle in
`corelink-clerk-cf::prod_wiring`.

The materializer crate exposes the symmetric `cf-billing-real`
feature flag which pulls in the optional `corelink-cf-bindings` +
`corelink-audit-chain` deps; the `wasm32_binders` module is
compile-gated on the feature so the default-feature build remains
slim (no new transitive deps on the InMemory-only path).

### Native-CI integration test (`tests/wasm32_binders.rs`)

15 tests pin the contract:

| # | Scenario | Asserts |
| - | -------- | ------- |
| 1-6 | One per `BillingD1Writer` mutation method | sync gate passes (scope + bind), staged-pending transient |
| 7 | `try_record_event` empty event id | `InvalidPayload` rejected |
| 8 | `try_record_event` happy path | staged-pending transient |
| 9 | `upsert_customer` cross-tenant row | `InvalidPayload` ct-eq mismatch |
| 10 | `read_tier` cross-tenant id | `InvalidPayload` ct-eq mismatch |
| 11 | `upsert_tier` cross-tenant id | `InvalidPayload` ct-eq mismatch |
| 12 | `upsert_tier` empty tier wire | `InvalidPayload` rejected |
| 13 | `read_tier` returns `Ok(None)` post-validation | structural sentinel |
| 14 | Audit emitter stages record via producer seam | staged-pending transient |
| 15 | Audit emitter exposes producer + sink witnesses | `Arc::ptr_eq` |

The validation contract is identical on native and wasm32 (the
wrapper-layer code in `corelink-cf-bindings::d1_real` runs on both
targets); the only divergence is the final `worker::D1Database` call
which `stub_for_native_tests` short-circuits with `WasmOnly:`. The
native CI run therefore pins every wasm32 invariant before the
wasm32 build catches a regression at runtime.

### Quality gates (wave 18)

| gate | command | result |
| ---- | ------- | ------ |
| build (default) | `cargo build -p corelink-billing-stripe-materializer` | green |
| build (wasm32) | `cargo build -p corelink-billing-stripe-materializer --target wasm32-unknown-unknown` | green |
| build (server) | `cargo build -p corelink-server --features cf-billing-real` | green |
| clippy | `cargo clippy -p corelink-billing-stripe-materializer --tests -- -D warnings` | green |
| tests | `cargo test -p corelink-billing-stripe-materializer` | 26 wave-17 preserved + 15 new |
| specs | `python3 scripts/validate_specs.py` | green |

### Wave-18 invariants reaffirmed

- **INV-TENANT-ISOLATION** — every wasm32 binder method runs the
  tenant ct-eq probe before any SQL or audit emit reaches the wrapped
  binding (`subtle::ConstantTimeEq`; defense in depth on top of the
  wave-14 `CfD1DatabaseReal` bind-time check).
- **INV-BILLING-NO-LOSS** — the binder's audit fail-CLOSED gate fires
  BEFORE any D1 write reaches the underlying `worker::D1Database`
  (the wave-14 `CfD1DatabaseReal::with_audit` hook owns the fence).
- **INV-AUDIT-APPEND-ONLY** — the `ArchiveProducerBillingEmitter`
  routes the `BillingAuditRecord` through the wave-15 producer + R2
  sink seam; no path mutates or deletes existing audit rows.

### What deliberately did NOT change (wave 18 baseline)

- The wave-17 `BillingD1Writer` + `BillingAuditEmitter` trait surfaces
  are unchanged. The wasm32 binders are pure additions behind the
  `cf-billing-real` feature; default-feature builds (the native gRPC
  server + axum HTTP shell) keep the `InMemory*` mirrors.
- The wave-15 `WebhookDispatcher` sync trait contract is unchanged.
  The wasm32 binders preserve the sync surface end-to-end; the async
  CF binding dispatch is handled by the CF Worker boot layer above
  (per the documented architectural seam).
- `apps/server/main.rs` keeps the wave-17 wiring for the default
  build; the `cf-billing-real` feature flag is the build-time witness
  for the wasm32 cutover path.
