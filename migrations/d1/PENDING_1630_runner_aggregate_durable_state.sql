-- PROVISIONAL #1630 migration: assign its ordinal only after the active
-- 0133–0137 migration queue has been serialized.
-- Durable, immutable authority for historical runner aggregation. The writer
-- reads terms from this table and claims every staged event in the same D1
-- batch that advances counters, heads, and the source watermark.
CREATE TABLE IF NOT EXISTS runner_period_terms_snapshot (
  tenant_id TEXT NOT NULL,
  billing_period TEXT NOT NULL CHECK (length(billing_period) = 7),
  allowance_vcpu_seconds TEXT NOT NULL CHECK (
    length(allowance_vcpu_seconds) BETWEEN 1 AND 39
    AND allowance_vcpu_seconds NOT GLOB '*[^0-9]*'
    AND (length(allowance_vcpu_seconds) = 1 OR substr(allowance_vcpu_seconds, 1, 1) <> '0')
    AND (length(allowance_vcpu_seconds) < 39 OR allowance_vcpu_seconds <= '340282366920938463463374607431768211455')
  ),
  rate_cents_per_vcpu_hour TEXT NOT NULL CHECK (
    length(rate_cents_per_vcpu_hour) BETWEEN 1 AND 20
    AND rate_cents_per_vcpu_hour NOT GLOB '*[^0-9]*'
    AND (length(rate_cents_per_vcpu_hour) = 1 OR substr(rate_cents_per_vcpu_hour, 1, 1) <> '0')
    AND (length(rate_cents_per_vcpu_hour) < 20 OR rate_cents_per_vcpu_hour <= '18446744073709551615')
  ),
  terms_snapshot_ref TEXT NOT NULL CHECK (length(terms_snapshot_ref) > 0),
  terms_snapshot_digest_hex TEXT NOT NULL CHECK (
    length(terms_snapshot_digest_hex) = 64
    AND terms_snapshot_digest_hex NOT GLOB '*[^0-9a-f]*'
  ),
  captured_at_ms INTEGER NOT NULL CHECK (captured_at_ms >= 0),
  PRIMARY KEY (tenant_id, billing_period)
);

CREATE TRIGGER IF NOT EXISTS runner_period_terms_snapshot_no_update
BEFORE UPDATE ON runner_period_terms_snapshot
BEGIN SELECT RAISE(ABORT, 'runner period terms snapshot is immutable'); END;

CREATE TRIGGER IF NOT EXISTS runner_period_terms_snapshot_no_delete
BEFORE DELETE ON runner_period_terms_snapshot
BEGIN SELECT RAISE(ABORT, 'runner period terms snapshot is immutable'); END;

-- A claim is the durable, per-event boundary for one logical aggregate batch.
-- A first insert is Inserted; an exact replay is Deduped; any other existing
-- winner is Conflict and aborts the whole D1 batch before accounting mutates.
CREATE TABLE IF NOT EXISTS runner_aggregate_event_claim (
  tenant_id TEXT NOT NULL,
  request_id TEXT NOT NULL CHECK (length(request_id) > 0),
  aggregate_batch_id TEXT NOT NULL CHECK (length(aggregate_batch_id) > 0),
  billing_period TEXT NOT NULL CHECK (length(billing_period) = 7),
  claim_fingerprint TEXT NOT NULL CHECK (
    length(claim_fingerprint) = 64
    AND claim_fingerprint NOT GLOB '*[^0-9a-f]*'
  ),
  terms_snapshot_ref TEXT NOT NULL CHECK (length(terms_snapshot_ref) > 0),
  terms_snapshot_digest_hex TEXT NOT NULL CHECK (
    length(terms_snapshot_digest_hex) = 64
    AND terms_snapshot_digest_hex NOT GLOB '*[^0-9a-f]*'
  ),
  evidence_ref TEXT NOT NULL CHECK (length(evidence_ref) > 0),
  evidence_digest_hex TEXT NOT NULL CHECK (
    length(evidence_digest_hex) = 64
    AND evidence_digest_hex NOT GLOB '*[^0-9a-f]*'
  ),
  claimed_at_ms INTEGER NOT NULL CHECK (claimed_at_ms >= 0),
  PRIMARY KEY (tenant_id, request_id)
);

CREATE TRIGGER IF NOT EXISTS runner_aggregate_event_claim_no_update
BEFORE UPDATE ON runner_aggregate_event_claim
BEGIN SELECT RAISE(ABORT, 'runner aggregate event claim is immutable'); END;

CREATE TRIGGER IF NOT EXISTS runner_aggregate_event_claim_no_delete
BEFORE DELETE ON runner_aggregate_event_claim
BEGIN SELECT RAISE(ABORT, 'runner aggregate event claim is immutable'); END;

CREATE INDEX IF NOT EXISTS idx_runner_aggregate_event_claim_batch
  ON runner_aggregate_event_claim (aggregate_batch_id, claimed_at_ms, tenant_id, request_id);
CREATE INDEX IF NOT EXISTS idx_runner_aggregate_event_claim_evidence
  ON runner_aggregate_event_claim (evidence_digest_hex, tenant_id, billing_period, request_id);
CREATE INDEX IF NOT EXISTS idx_runner_aggregate_event_claim_terms
  ON runner_aggregate_event_claim (tenant_id, billing_period, terms_snapshot_digest_hex, request_id);
