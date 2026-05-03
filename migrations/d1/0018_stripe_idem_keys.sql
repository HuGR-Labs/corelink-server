-- CoreLink D1 (Cloudflare SQLite) — migration 0018 for the canonical
-- Stripe idempotency-key + webhook event log staging tables (S-10
-- billing pipeline; WI-S10-003 Stripe Adapter + Idempotency-Key +
-- HMAC-SHA256 Webhook Signature Verify).
--
-- Canonical sources:
--   - specs/04_sprints/S10/work_items/WI-S10-003-corelink-billing-stripe-adapter-idempotency-webhook.md §6.1.5 + §6.1.6
--   - specs/04_sprints/S10/_spec_contract.md §5.3 (R-S10-6 / R-S10-7)
--   - specs/03_architecture/invariant_registry.md INV-BILLING-NO-DUP / INV-AUDIT-APPEND-ONLY
--   - specs/03_architecture/security_model.md CTRL-BILLING-001 / CTRL-PRIV-002
--   - specs/03_architecture/adrs/ADR-0036-d1-schema-migration-governance.md
--
-- Schema rationale:
--   - `stripe_idempotency_keys` mirrors the in-memory
--     `corelink-billing-stripe::InMemoryStripeUsageLedger` set
--     membership at the durable layer; cold-start production wiring
--     rehydrates the accepted set (within Stripe's canonical 24h
--     idempotency window) so a Worker restart never produces a
--     duplicate Stripe charge.
--   - `idempotency_key` is the canonical 64-char hex BLAKE3-256 digest
--     of the JCS-canonical AggregatedCounter bytes (per WI-S10-003 §6.1
--     + sprint contract §5.3 R-S10-6); same aggregate reproduces the
--     same key by construction (collision probability < 2^-128).
--   - `stripe_event_log` mirrors the in-memory
--     `corelink-billing-stripe::InMemoryStripeWebhookLog`; PRIMARY KEY
--     `(stripe_event_id)` UNIQUE prevents Stripe webhook double-
--     processing per the canonical Stripe webhook spec retry semantics.
--   - `event_type` column pins the canonical 5-element webhook taxonomy
--     (`invoice.created` / `invoice.paid` / `invoice.payment_failed` /
--     `customer.subscription.updated` / `customer.created`) per
--     WI-S10-003 §6.1.6.
--   - `payload_redacted` column stores the JSON bytes AFTER PII
--     redaction (production wiring uses the `CustomerEmail` /
--     `CustomerName` Serialize-impl wrappers per WI-S10-003 §6.1
--     invariant 6 / Lote 10.9-quinquies NEW-P0-2 absorption); raw PII
--     never persists in this column.
--
-- Invariants enforced at storage layer:
--   - INV-BILLING-NO-DUP (HIGH; invariant_registry §3.9 line 137):
--     PRIMARY KEY (idempotency_key) UNIQUE on `stripe_idempotency_keys`
--     prevents duplicate Stripe charges within Stripe's canonical 24h
--     idempotency window; PRIMARY KEY (stripe_event_id) UNIQUE on
--     `stripe_event_log` prevents duplicate webhook event processing
--     under Stripe network-retry semantics.
--   - INV-AUDIT-APPEND-ONLY (CRITICAL, TLA+ proven Lote 6.2; S-09
--     inheritance): both tables are INSERT-only at the storage layer;
--     ON CONFLICT DO NOTHING returns 0 rows affected → orchestrator
--     dispatches to the canonical duplicate-rejected audit arm.
--   - CTRL-BILLING-001 (security_model.md): financial integrity;
--     idempotency-key is the canonical Stripe charge dedup primitive.
--   - CTRL-PRIV-002 (privacy_model.md L209): PII data classification;
--     `payload_redacted` column never stores raw email / name.
--
-- Conventions (mirror migrations/d1/0001..0017):
--   - All timestamps stored as INTEGER Unix epoch milliseconds.
--   - Migration is idempotent via `CREATE TABLE IF NOT EXISTS` /
--     `CREATE INDEX IF NOT EXISTS`.
--   - Migrations are additive-only per scripts/check_migrations_additive.py
--     CI gate.
--
-- D1 SQL correctness gates (Lote 10.4bis P0 lessons; pre-deploy CI):
--   - CHECK constraints inlined in CREATE TABLE (SQLite/D1 does NOT
--     support `ALTER TABLE … ADD CONSTRAINT chk_*`; only inline at
--     CREATE TABLE per ADR-0036 Rule 1).
--   - BEGIN/COMMIT NOT included (`wrangler d1 migrations apply` uses
--     an implicit transaction).
--   - No `_ms` column-name suffix per Lote 10.7bis P0-3 column-drift
--     lesson; instead `*_at` Unix epoch ms.
--
-- Backfill plan: NONE. Both tables start empty; rows are inserted by
-- the canonical billing-stripe adapter (every Stripe usage record write)
-- + the webhook handler (every Stripe webhook delivery).
--
-- Migration runner: see scripts/migrate_d1.sh.

-- ---------------------------------------------------------------------------
-- stripe_idempotency_keys — durable mirror of the canonical
-- `Idempotency-Key` ledger. PRIMARY KEY (idempotency_key) UNIQUE pins
-- INV-BILLING-NO-DUP at the storage layer; the orchestrator
-- short-circuits on the second sight (within Stripe's canonical 24h
-- window) so the customer is charged exactly once.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS stripe_idempotency_keys (
  -- 64-char hex lowercase BLAKE3-256 digest of the JCS-canonical
  -- AggregatedCounter bytes (per WI-S10-003 §6.1 + sprint contract
  -- §5.3 R-S10-6). PRIMARY KEY UNIQUE pins INV-BILLING-NO-DUP at
  -- storage layer.
  idempotency_key             TEXT     NOT NULL,

  -- Tenant id (canonical UUIDv7 hyphenated lowercase from the auth
  -- middleware; S-03 Lote 10.4bis enforcement — never the request
  -- body). Per-tenant Stripe Customer scoping.
  tenant_id                   TEXT     NOT NULL,

  -- Canonical billing period (`YYYY-MM` per WI-S10-001 §1
  -- validate_billing_period guard).
  billing_period              TEXT     NOT NULL,

  -- Stripe `subscription_item_id` (e.g. `si_1A2B3C…`) — per-tenant
  -- subscription item populated from the Neon Postgres `subscription`
  -- table at usage-record emit time.
  subscription_item_id        TEXT     NOT NULL,

  -- Total billable quantity recorded at the Stripe usage ledger
  -- (sum of contributing event qty's per the upstream
  -- AggregatedCounter::data.total_qty; u128 in Rust, stored as TEXT
  -- in D1 to avoid u64-truncation defenses).
  total_qty_text              TEXT     NOT NULL,

  -- Wall-clock instant of the Stripe usage record write (Unix epoch
  -- ms; canonical no `_ms` suffix per Lote 10.7bis P0-3).
  recorded_at                 INTEGER  NOT NULL,

  PRIMARY KEY (idempotency_key),

  -- chk_stripe_idem_keys_idempotency_key_64hex: idempotency_key MUST
  -- be exactly 64 hex chars (BLAKE3-256 canonical form).
  CHECK (length(idempotency_key) = 64),

  -- chk_stripe_idem_keys_tenant_id_uuid_len: UUIDv7 hyphenated form.
  CHECK (length(tenant_id) = 36),

  -- chk_stripe_idem_keys_billing_period_yyyy_mm: canonical 7-char
  -- format `YYYY-MM` (e.g. `2026-05`).
  CHECK (length(billing_period) = 7),

  -- chk_stripe_idem_keys_subscription_item_non_empty.
  CHECK (length(subscription_item_id) >= 1),

  -- chk_stripe_idem_keys_total_qty_text_non_empty.
  CHECK (length(total_qty_text) >= 1),

  -- chk_stripe_idem_keys_recorded_at_non_negative.
  CHECK (recorded_at >= 0)
);

-- Index: per-tenant per-period scan — admin forensic surface +
-- reconciliation worker (WI-S10-004) Layer 3 input (Σ Stripe usage
-- records vs Σ counter aggregates drift detection).
CREATE INDEX IF NOT EXISTS idx_stripe_idem_keys_per_tenant_period
  ON stripe_idempotency_keys(tenant_id, billing_period, recorded_at);

-- ---------------------------------------------------------------------------
-- stripe_event_log — durable mirror of the canonical webhook event log.
-- PRIMARY KEY (stripe_event_id) UNIQUE pins webhook redelivery dedup
-- at storage layer per WI-S10-003 §6.1.6 + Stripe webhook spec
-- (Stripe may deliver the same webhook 2× under network failure).
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS stripe_event_log (
  -- Canonical Stripe `evt_*` event id (UNIQUE per delivery; PRIMARY
  -- KEY pins INV-BILLING-NO-DUP at the webhook layer).
  stripe_event_id             TEXT     NOT NULL,

  -- Canonical 5-element webhook event taxonomy: `invoice.created` /
  -- `invoice.paid` / `invoice.payment_failed` /
  -- `customer.subscription.updated` / `customer.created`
  -- (CHECK constraint enforces; runtime enforcement via the typed
  -- WebhookEventKind enum at the trait surface).
  event_type                  TEXT     NOT NULL,

  -- Stripe-side wall-clock instant of the event emit (Unix epoch ms;
  -- canonical no `_ms` suffix per Lote 10.7bis P0-3).
  event_ts                    INTEGER  NOT NULL,

  -- Receiver-side wall-clock instant of the webhook delivery arrival
  -- (Unix epoch ms). Pins the canonical arrival watermark for
  -- forensic reconstruction.
  received_at                 INTEGER  NOT NULL,

  -- PII-redacted JSON bytes of the Stripe webhook payload (per
  -- WI-S10-003 §6.1 invariant 6 / Lote 10.9-quinquies NEW-P0-2: the
  -- redaction lives at the CustomerEmail / CustomerName Serialize-impl
  -- wrappers in the production wiring). Raw PII NEVER lands here.
  payload_redacted            BLOB     NOT NULL,

  PRIMARY KEY (stripe_event_id),

  -- chk_stripe_event_log_event_type_canonical: event_type MUST be one
  -- of the canonical 5-element taxonomy per WI-S10-003 §6.1.6.
  CHECK (event_type IN (
    'invoice.created',
    'invoice.paid',
    'invoice.payment_failed',
    'customer.subscription.updated',
    'customer.created'
  )),

  -- chk_stripe_event_log_event_id_starts_evt: Stripe canonical event
  -- ids start with `evt_` per the Stripe API spec.
  CHECK (substr(stripe_event_id, 1, 4) = 'evt_'),

  -- chk_stripe_event_log_event_ts_non_negative.
  CHECK (event_ts >= 0),

  -- chk_stripe_event_log_received_at_non_negative.
  CHECK (received_at >= 0)
);

-- Index: per-event-type scan — dashboard widget (5-element taxonomy
-- bucket) + per-kind dispatch fan-out for SEV-2 alert (signature
-- rejection spike monitor).
CREATE INDEX IF NOT EXISTS idx_stripe_event_log_per_type_received
  ON stripe_event_log(event_type, received_at);

-- Index: chronological scan — admin forensic surface + replay
-- forensic endpoint (WI-S10-006) input.
CREATE INDEX IF NOT EXISTS idx_stripe_event_log_received_chronological
  ON stripe_event_log(received_at);
