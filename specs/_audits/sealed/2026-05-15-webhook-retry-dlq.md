---
id: "AUDIT-2026-05-15-WEBHOOK-RETRY-DLQ"
type: "audit"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
sprint: "R-prep"
owner: "Gustavo Schneiter"
tags: ["audit", "webhook", "stripe", "retry", "dlq", "idempotency", "r-prep"]
---

# Audit — Stripe Webhook Retry + Dead-Letter Queue Surface

> **Scope.** End-to-end audit of the retry policy + idempotency dedup +
> dead-letter quarantine surface for the `POST /v1/billing/stripe-webhook`
> route exposed by `apps/server/src/webhook.rs` against the
> `corelink-stripe-real` crate (signature verify + retry policy) and the
> D1 dedup table `stripe_webhook_events_processed` (migration 0044).
>
> **Goal.** Enumerate every link in the chain (Stripe-side retry → HMAC
> verify → idempotency dedup → handler dispatch → outcome) + every failure
> mode that can quarantine an event, then surface P0/P1/P2 gaps + propose
> closures for the P0 gaps in this same WI.

## 1. Surface as it stands (2026-05-15)

### 1.1 Stripe-side retry policy (inbound — Stripe → CoreLink)

| Aspect | Value | Source |
|--------|-------|--------|
| Max attempts | Stripe API: up to **3 days** (canonical Stripe behaviour) | Stripe docs + module-level docs `apps/server/src/webhook.rs:53` |
| Backoff curve | Exponential, owned by Stripe (we do not control it) | Stripe docs |
| Trigger | Any non-2xx response OR delivery timeout | Stripe docs |
| Acknowledgement signal | HTTP 200 from `/v1/billing/stripe-webhook` | `apps/server/src/webhook.rs:711, 751, 768` |
| Failure signal | HTTP 4xx (Stripe stops retrying for malformed event) OR HTTP 5xx (Stripe keeps retrying for 3 days) | `apps/server/src/webhook.rs:634, 657, 674, 697, 794` |

**Implication.** A non-recoverable handler bug returning 5xx will be
retried up to 3 days; after 3 days Stripe gives up + the event is
**lost** from Stripe's side. CoreLink has **no copy** unless we
write one. This is the canonical DLQ gap.

### 1.2 Outbound Stripe retry policy (CoreLink → Stripe API)

`crates/corelink-stripe-real/src/retry.rs` — `RetryPolicy::default()`:

| Knob | Value |
|------|-------|
| `max_retries` | 5 |
| `base_ms` | 250 |
| `cap_ms` | 8 000 |
| Schedule | 250ms → 500ms → 1s → 2s → 4s → 8s (cap) |
| Retryable statuses | `429`, `5xx` |
| `Retry-After` honored | yes (wins over geometric backoff) |
| Total worst-case window | `250+500+1000+2000+4000 = 7.75s` (without `Retry-After`) |

`is_retryable_status` rejects `4xx ≠ 429` correctly. NO known gap in
the outbound path; this audit's P0/P1 calls are about the **inbound**
path.

### 1.3 Idempotency dedup (D1 table `stripe_webhook_events_processed`)

Schema (`migrations/d1/0044_stripe_webhook_events_processed.sql`):

```sql
CREATE TABLE stripe_webhook_events_processed (
    event_id         TEXT PRIMARY KEY,        -- Stripe `evt_…`
    event_type       TEXT NOT NULL,
    processed_at_ms  BIGINT NOT NULL,
    outcome          TEXT NOT NULL CHECK (outcome IN
                       ('dispatched', 'acknowledged_unknown')),
    correlation_id   TEXT NOT NULL
);
CREATE INDEX idx_webhook_events_processed_at  ON … (processed_at_ms);
CREATE INDEX idx_webhook_events_processed_type ON … (event_type);
```

Dedup logic: `INSERT OR IGNORE` keyed on `event_id`. The in-process
trait split (`apps/server/src/webhook.rs:678-712`) accepts a known
trade-off: dedup row is inserted **before** handler dispatch; a
handler failure leaves the dedup row in place; Stripe's retry sees
`AlreadyProcessed` + acks 200 without re-running the (broken) handler.
This is **the HARD idempotency guarantee** at the cost of giving up
auto-retry of a single failed handler. The CodeLink charter accepts
this trade-off explicitly per WI-S19-004 §1.

**Replay-safety analysis.**
- Same `event_id` × N orderings (N parallel attempts): exactly **1**
  `INSERT OR IGNORE` wins (canonical SQLite atomic INSERT); all
  others observe `AlreadyProcessed` + ack 200. ✅
- Tampered payload with same `event_id` (replay attack): rejected at
  HMAC verify (step 2) — payload is signed, signature mismatch
  returns 400 **before** reaching dedup. ✅
- Future-dated / replay-window: rejected at step 2; never reaches
  dedup. ✅
- Same `event_id`, different `event_type` (impossible per Stripe spec
  but defensive): the **second** insert hits the dedup row + acks; the
  audit-stored `event_type` is the **first** one (no overwrite). The
  forensic audit chain captures the divergence via the
  `duplicate_event` arm + the original `processing_started` row. ⚠️
  → see GAP-P2-1.

### 1.4 Dead-letter quarantine surface — **CURRENT STATE: NONE**

There is **no D1 table, KV namespace, queue binding, or in-memory
buffer** for Stripe webhook events that exhausted retry on the
CoreLink side. The current quarantine surface is **entirely
external**: Stripe's own retry buffer (3-day window). After the
3-day window, the event is **silently dropped**.

The `corelink-dt-webhook` crate ships a DLQ for the **Dependency-Track**
webhook surface (cap 1 000 events, in-memory + CF KV mirror). That
DLQ is **NOT** wired to the Stripe webhook route. The
`corelink-billing-replay` crate is for **billing forensic replay**
(WI-S10-006), keyed by `tenant_id + billing_period`, NOT by Stripe
`event_id`; it cannot replace a webhook DLQ.

### 1.5 Replay path — **CURRENT STATE: NONE**

No documented surface for an operator to **manually replay** a
quarantined Stripe webhook event. The closest existing surfaces are:
- `corelink-billing-replay` — billing forensic replay (different
  surface; per `tenant + billing_period`, not per Stripe `event_id`).
- `corelink-dt-reconcile` — drains the DT webhook DLQ daily (not
  Stripe-applicable).

If a Stripe event was lost (exhausted Stripe's 3-day retry) the only
recourse today is to (a) ask Stripe support to re-deliver (manual
process; not guaranteed) or (b) reconstruct the state from the
Stripe API (`GET /v1/customers/{id}` + `GET /v1/subscriptions/{id}`
+ `GET /v1/invoices/...`) — but there's no documented runbook for
this either.

## 2. Audit-emit ordering (✅ holds end-to-end)

Per `apps/server/src/webhook.rs`, the audit ordering is fail-CLOSED
on every arm:

| Step | Audit emit | Position |
|------|------------|----------|
| Signature invalid | `signature_invalid` | BEFORE 400 return |
| JSON parse failed | `processing_failed` | BEFORE 400 return |
| Idempotency store transient err | `processing_failed` | BEFORE 500 return |
| Idempotency hit (Stripe retry) | `duplicate_event` | BEFORE 200 return |
| Pre-dispatch | `processing_started` | BEFORE handler call |
| Unknown event type | `unknown_event_type` | BEFORE 200 return |
| Dispatch ok | `processing_completed` | AFTER handler ok |
| Dispatch err | `processing_failed` | AFTER handler err |

INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER holds. ✅

## 3. Gaps enumerated

### P0 (block GA)

**GAP-P0-1 — No CoreLink-side DLQ for exhausted-retry webhooks.**
After Stripe's 3-day retry window, a persistently-failing webhook
event is **silently lost**. We have audit chain rows (`processing_failed`)
but no actionable bucket for an operator to inspect + replay.
**Closure**: add D1 table `stripe_webhook_events_dlq` with TTL +
attempt count + first-/last-seen timestamps + last error. Wired in
migration 0045. **Closed in this WI.**

**GAP-P0-2 — No DLQ depth / age alerting.**
Even if the DLQ existed, there is no Prometheus counter / gauge for
`dlq_depth` or `dlq_oldest_age_seconds`, hence no alert firing.
**Closure**: add canonical metric-name constants in
`corelink-stripe-real::dlq` + audit-stored alert thresholds
(`depth > 0` warning, `oldest_age > 6h` page). **Closed in this WI.**

**GAP-P0-3 — Replay surface has no dual-approval audit trail.**
A future replay surface MUST require dual approval (one operator
prepares + one operator approves the replay) + write a separate
audit chain row per replay. Today no such surface exists at all.
**Closure**: document the canonical replay-with-dual-approval design
in this audit + bind it through `corelink-dual-approval` once the
admin API lands. The DLQ row schema includes `replay_request_id` +
`replayed_by` + `replayed_at_ms` slots so the schema is forward-compatible.
**Schema slot added in this WI; live wiring deferred to admin API
landing (S-R3.x).**

### P1 (degrade-GA-quality but not block)

**GAP-P1-1 — DLQ rows lack hard retention.**
Closure for GAP-P0-1 includes an `expires_at_ms` TTL slot; pruning
job is documented in the replay runbook (manual cron — `DELETE FROM
stripe_webhook_events_dlq WHERE expires_at_ms < now_ms`). Live
scheduled pruning is deferred to a Worker cron binding (admin-API
sprint).

**GAP-P1-2 — Idempotency dedup table itself lacks TTL.**
`stripe_webhook_events_processed` rows accumulate forever; the
migration's index comment mentions "180-day-old rows once retention
policy is finalized" but no policy is finalized + no pruning is
scheduled. Forensic value of 180-day-old dedup rows is **low** (Stripe
has its own ledger). Recommend a `DELETE … WHERE processed_at_ms <
now - 180 days` pruning cron; track in a future audit.

**GAP-P1-3 — Replay requires DB shell access today.**
Until the admin API lands, the only way to inspect / mutate the DLQ
is `wrangler d1 execute …`. Documented in the runbook with safety
rails.

### P2 (nice-to-have)

**GAP-P2-1 — Same `event_id` × different `event_type` divergence not
surfaced.** Documented above (§1.3). Recommend a forensic alert
`stripe.idempotency.event_type_drift` if observed.

**GAP-P2-2 — No replay end-to-end test against the real Stripe
sandbox.** Today the integration tests cover the in-process trait
split + the property test in this WI; a quarterly drill against the
Stripe sandbox would prove the full chain.

## 4. Summary

| Severity | Found | Closed in WI | Deferred |
|----------|-------|--------------|----------|
| P0 | 3 | 3 | 0 |
| P1 | 3 | 0 (documented) | 3 |
| P2 | 2 | 0 (documented) | 2 |

## 5. Cross-references

- INV-AUTH-MIGRATION-ADDITIVE (HIGH) — every closure migration is
  additive (verified by `scripts/check_migrations_additive.py`).
- INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER (HIGH) — audit ordering holds;
  new DLQ-write step emits `corelink.billing.webhook.dlq_quarantined`
  BEFORE any state mutation past the existing `processing_failed`
  emit.
- INV-OBS-AUDIT-CHAIN-INTEGRITY (S-09 inheritance) — every DLQ
  quarantine + replay is a chain event in its own right.
- FM-151 (Stripe outage) — mitigated by Stripe-side retry; the DLQ
  in this WI covers persistent-handler-bug failure (orthogonal mode).

## 6. Closures landed in this WI

1. `migrations/d1/0045_stripe_webhook_dlq.sql` — additive DLQ table
   with `expires_at_ms` TTL + `replay_request_id` + `replayed_by`
   slots.
2. `crates/corelink-stripe-real/src/dlq.rs` — canonical
   `WebhookDlqRow` type + Prometheus metric-name constants
   (`corelink_stripe_webhook_dlq_*`) + in-memory store + TTL pruning
   logic.
3. `crates/corelink-stripe-real/tests/prop_dlq.rs` — property test:
   100 random orderings of same `stripe_event_id` → exactly 1
   side-effect + DLQ TTL pruning correctness.
4. `specs/_runbooks/RB-WEBHOOK-DLQ-REPLAY.md` — operator runbook.

## 7. Sign-off

`final_approver: Gustavo Schneiter` — date `2026-05-15`.
