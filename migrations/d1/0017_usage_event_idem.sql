-- CoreLink D1 (Cloudflare SQLite) — migration 0017 for the canonical
-- usage-event idempotency staging table (S-10 billing pipeline; WI
-- WI-S10-001 Usage Event Emitter + R2 Append-Only + Idempotency).
--
-- Canonical sources:
--   - specs/04_sprints/S10/work_items/WI-S10-001-usage-event-emitter-r2-append-only-idempotency.md §6.1.4
--   - specs/04_sprints/S10/_spec_contract.md §5.1 (R-S10-1 / R-S10-2 / R-S10-3)
--   - specs/03_architecture/invariant_registry.md INV-BILLING-NO-LOSS / INV-BILLING-NO-DUP
--   - specs/03_architecture/security_model.md CTRL-BILLING-001
--   - specs/03_architecture/adrs/ADR-0036-d1-schema-migration-governance.md
--
-- Schema rationale:
--   - Per WI §6.1.4 the canonical (tenant_id, region, request_id) UNIQUE
--     idempotency staging table mirrors the in-memory
--     `corelink-billing-emit::IdempotencyTracker` set membership at
--     durable layer; cold-start production wiring rehydrates the
--     accepted set + advances the canonical idempotency watermark.
--   - `event_payload_hash` is the BLAKE3-256 of the JCS-canonical event
--     bytes (with the idem_key slot zeroed for the link input — mirrors
--     the audit-chain canonical link input); collision detection
--     surface (SEV-1 if the same key reuses across diverged canonical
--     bytes — adversarial misuse of request_id).
--   - `region` column is required for the drain consumer routing per
--     Lote 10.10bis R5 P1-A fix (drain Worker routes to the correct
--     per-region R2 bucket without deserializing the full payload).
--   - `event_type` column pins the canonical `corelink.billing.usage.recorded`
--     CloudEvents 1.0 type (sprint contract §5.1 R-S10-1).
--   - `drained_to_r2_at` watermark NULL until the retry queue (drain
--     Worker) commits the R2 PutObject; cold-start scans pending rows
--     ordered by `(region, emitted_at)` for SLO-FRESH-BILLING ≤ 15min
--     enforcement.
--
-- Invariants enforced at storage layer:
--   - INV-BILLING-NO-DUP (HIGH; invariant_registry §3.9 line 137):
--     PRIMARY KEY (tenant_id, request_id) UNIQUE prevents duplicate
--     rows; matches the in-memory `IdempotencyTracker::insert` set
--     membership (re-emit at the same (tenant, request_id) returns
--     `IdempotencyDecision::DuplicateRejected`).
--   - INV-BILLING-NO-LOSS (HIGH; invariant_registry §3.9 line 136):
--     `drained_to_r2_at` watermark + `(region, emitted_at)` index
--     guarantee no row sits silently in the staging table — the drain
--     Worker SLO is ≤ 15min p99; SEV-2 alert fires above the threshold.
--   - CTRL-BILLING-001 (security_model.md): financial integrity;
--     append-only events flow through this staging mirror to the R2
--     Object Lock 7y archive at the canonical
--     `usage/{tenant_id}/{billing_period}/{seq:08}.usage.ndjson` key.
--
-- Conventions (mirror migrations/d1/0001..0016):
--   - All timestamps stored as INTEGER Unix epoch milliseconds.
--   - Migration is idempotent via `CREATE TABLE IF NOT EXISTS` /
--     `CREATE INDEX IF NOT EXISTS`.
--   - Migrations are additive-only per scripts/check_migrations_additive.py
--     CI gate.
--   - Region stored as canonical 3-char TEXT matching
--     `corelink_analytics::Region::as_str()` (lowercase IATA-style colocode).
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
-- Backfill plan: NONE. The table starts empty; rows are inserted by
-- the canonical billing-emit hot path on every CAS PUT/GET + AC lookup
-- + GC purge + replay request.
--
-- Migration runner: see scripts/migrate_d1.sh.

-- ---------------------------------------------------------------------------
-- usage_event_staging — durable idempotency staging table mirror.
-- PRIMARY KEY (tenant_id, request_id) UNIQUE pins INV-BILLING-NO-DUP at
-- the storage layer; the drain Worker consumes pending rows ordered by
-- (region, emitted_at) for SLO-FRESH-BILLING enforcement.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS usage_event_staging (
  -- Tenant id (canonical UUIDv7 hyphenated lowercase from the auth
  -- middleware; S-03 Lote 10.4bis enforcement — never the request body).
  tenant_id                   TEXT     NOT NULL,

  -- Canonical 3-char CF colocode (e.g. `iad`, `gru`, `fra`); required
  -- for the drain Worker's per-region R2 bucket routing without
  -- deserializing the full payload (Lote 10.10bis R5 P1-A fix).
  region                      TEXT     NOT NULL,

  -- W3C trace context request id (S-09 inheritance); the canonical
  -- (tenant_id, request_id) UNIQUE coordinate for INV-BILLING-NO-DUP.
  request_id                  TEXT     NOT NULL,

  -- Canonical CloudEvents 1.0 `type` attribute. WI-S10-001 §1 pins the
  -- canonical type to `corelink.billing.usage.recorded`; the CHECK
  -- constraint here is defense-in-depth (the runtime emitter enforces
  -- the canonical type at the trait surface).
  event_type                  TEXT     NOT NULL,

  -- BLAKE3-256 of the JCS-canonical event bytes (with the idem_key
  -- slot zeroed for the link input). Hex-rendered 64-char canonical
  -- form (RFC 4648 §8 lowercase). Used for SEV-1 idempotency-collision
  -- detection: the same (tenant_id, request_id) MUST reproduce the
  -- same hash; a divergence = caller bug or BLAKE3 implementation
  -- defect.
  event_payload_hash          TEXT     NOT NULL,

  -- Canonical UUIDv7 (time-ordered) assigned by the emitter on first
  -- sight (mirrors the CloudEvents 1.0 `id` attribute).
  event_id                    TEXT     NOT NULL,

  -- Wall-clock instant of the staging-table insert (Unix epoch ms;
  -- canonical no `_ms` suffix per Lote 10.7bis P0-3).
  emitted_at                  INTEGER  NOT NULL,

  -- Wall-clock instant of the R2 PutObject commit (Unix epoch ms);
  -- NULL until the retry queue / drain Worker commits the R2 write.
  -- The drain Worker scans `WHERE drained_to_r2_at IS NULL` ordered by
  -- `emitted_at` to enforce SLO-FRESH-BILLING ≤ 15min.
  drained_to_r2_at            INTEGER,

  PRIMARY KEY (tenant_id, request_id),

  -- chk_usage_event_staging_region_3char: region MUST be exactly 3
  -- chars (matches the canonical CF colocode shape).
  CHECK (length(region) = 3),

  -- chk_usage_event_staging_request_id_non_empty.
  CHECK (length(request_id) >= 1),

  -- chk_usage_event_staging_event_type_canonical: event_type MUST be
  -- the canonical `corelink.billing.usage.recorded` slug per WI-S10-001
  -- §1 (defense-in-depth; runtime enforcement via the emitter).
  CHECK (event_type = 'corelink.billing.usage.recorded'),

  -- chk_usage_event_staging_event_payload_hash_64hex: hash MUST be
  -- exactly 64 hex chars (BLAKE3-256 canonical form).
  CHECK (length(event_payload_hash) = 64),

  -- chk_usage_event_staging_event_id_non_empty.
  CHECK (length(event_id) >= 1),

  -- chk_usage_event_staging_emitted_at_non_negative.
  CHECK (emitted_at >= 0),

  -- chk_usage_event_staging_drained_after_emitted: the drain commit
  -- watermark MUST be NULL or >= emitted_at (no time-travel commits).
  CHECK (drained_to_r2_at IS NULL OR drained_to_r2_at >= emitted_at)
);

-- Index: drain-pending scan — drain Worker streams pending rows per
-- region ordered by emitted_at; canonical SLO-FRESH-BILLING ≤ 15min
-- enforcement.
CREATE INDEX IF NOT EXISTS idx_usage_event_staging_pending_drain
  ON usage_event_staging(region, emitted_at)
  WHERE drained_to_r2_at IS NULL;

-- Index: per-tenant per-region scan — admin forensic surface +
-- reconciliation worker (WI-S10-004) Layer 1 input (Σ events vs
-- Σ counters drift detection).
CREATE INDEX IF NOT EXISTS idx_usage_event_staging_per_tenant
  ON usage_event_staging(tenant_id, region, emitted_at);
