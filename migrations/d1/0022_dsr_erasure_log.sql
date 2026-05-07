-- D1 migration: dsr_erasure_log — per-backend erasure tombstones
--
-- WI-S11-002 §6.1.7 canonical DDL. Drives the canonical
-- `(dsr_id, backend)` UNIQUE constraint enforcing replay-safe
-- per PAT-RETRY-IDEMPOTENT-001 (sprint contract §9 14.s11.4).
-- Powers the 12-backend canonical erasure pipeline pós Lote
-- 10.11.0-bis (8 effective + 4 pseudonymized — privacy_model.md
-- §6.2 source-of-truth).
--
-- Retention: forensic 7y (canonical pós cycle 15 SEAL unified per
-- privacy_model.md §8); deletion only on the canonical 7y
-- expiry sweep + ANPD/Irish DPC investigation hold release.
--
-- INV-DATA-ERASURE-COMPLETE CRITICAL §3.5 L110: every successful
-- per-backend erasure step writes one row here; the verification
-- job 24h sweep asserts 12 tombstones per dsr_id pre-`completed.v1`.

CREATE TABLE IF NOT EXISTS dsr_erasure_log (
  log_id              TEXT        PRIMARY KEY,                         -- ULID 26 chars
  dsr_id              TEXT        NOT NULL,                            -- FK dsr_tickets.dsr_id (UUIDv7 hex)
  tenant_id           TEXT        NOT NULL,
  -- subject_id_hash = sha256(subject_id || tenant_salt) — never
  -- plaintext (CTRL-PRIV-014 minimization).
  subject_id_hash     TEXT        NOT NULL,
  -- 12 canonical backends pós Lote 10.11.0-bis (privacy_model.md §6.2
  -- source-of-truth): 8 effective + 4 pseudonymized.
  backend             TEXT        NOT NULL CHECK (backend IN (
                                                    'neon_main',
                                                    'neon_billing',
                                                    'r2_cas',
                                                    'r2_ac',
                                                    'd1',
                                                    'kv',
                                                    'stripe',
                                                    'loki',
                                                    'r2_audit_pseudo',
                                                    'neon_pitr_pseudo',
                                                    'r2_cas_legalhold_pseudo',
                                                    'r2_evidence_pseudo'
                                                 )),
  -- 5-arm canonical outcome (sprint contract §5.2 R-S11-6).
  outcome             TEXT        NOT NULL CHECK (outcome IN (
                                                    'erased',
                                                    'pseudonymized',
                                                    'partial_failure',
                                                    'failed',
                                                    'not_applicable'
                                                 )),
  records_affected    INTEGER     NOT NULL DEFAULT 0,                  -- count_deleted OR count_redacted
  error_classes       TEXT        NULL,                                -- JSON array enum (only on partial_failure | failed)
  retry_count         INTEGER     NOT NULL DEFAULT 0
                                  CHECK (retry_count >= 0 AND retry_count <= 5),
  -- 35-char canonical idempotency key per Lote 10.10-quaters:
  -- corelink-{dsr_id_short(8)}-{backend}-{retry_count(3)}.
  idempotency_key     TEXT        NOT NULL,
  started_at          TEXT        NOT NULL,                            -- ISO 8601 UTC
  completed_at        TEXT        NULL,
  -- Idempotency UNIQUE (dsr_id, backend) → replay-safe
  -- (PAT-RETRY-IDEMPOTENT-001; sprint contract §9 14.s11.4).
  UNIQUE (dsr_id, backend)
);

-- Index: per-dsr per-backend lookup (canonical surface for the
-- orchestrator's lookup-first replay-safe arm).
CREATE INDEX IF NOT EXISTS idx_dsr_erasure_log_dsr
  ON dsr_erasure_log(dsr_id, backend);

-- Index: per-tenant outcome-time scan (dashboard widget surface for
-- corelink_dsr_erasure_backend_outcome_total{tenant, status, period}).
CREATE INDEX IF NOT EXISTS idx_dsr_erasure_log_tenant_outcome
  ON dsr_erasure_log(tenant_id, outcome, completed_at DESC);

-- Index: per-subject_id_hash forensic re-correlation (queryable by
-- the customer-held erasure_salt under court order; production
-- wiring at WI-S11-008 binds this to the canonical
-- corelink_privacy_pseudonymize::verify_pseudonym surface).
CREATE INDEX IF NOT EXISTS idx_dsr_erasure_log_subject_hash
  ON dsr_erasure_log(subject_id_hash);
