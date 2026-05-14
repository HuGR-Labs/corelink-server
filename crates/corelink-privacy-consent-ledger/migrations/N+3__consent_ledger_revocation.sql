-- Neon migration N+3: consent_ledger + consent_revocation
-- (canonical pós Lote 10.11.0-bis; WI-S11-003)
-- CTRL-PRIV-CONSENT-001..006 + INV-CONSENT-PROOF-VERIFIABLE CRITICAL §3.12 L168
-- + INV-AUDIT-APPEND-ONLY CRITICAL §3.6 L116.

-- ─── consent_ledger (canonical pós Lote 10.11.0-bis) ────────────────────────

CREATE TABLE IF NOT EXISTS consent_ledger (
  consent_id                TEXT        PRIMARY KEY,
  tenant_id                 TEXT        NOT NULL,
  subject_id                TEXT        NOT NULL,
  subject_id_hash           TEXT        NOT NULL,
  purpose                   TEXT        NOT NULL
    CHECK (purpose IN (
      'service_delivery',
      'account_management',
      'regulatory_compliance',
      'security_monitoring',
      'analytics_aggregated',
      'analytics_personalized',
      'marketing_email',
      'marketing_research',
      'beta_features',
      'third_party_integrations',
      'cross_tenant_benchmarks',
      'training_ml_models'
    )),
  basis_legal               TEXT        NOT NULL
    CHECK (basis_legal IN ('contract','legal_obligation','legitimate_interest','consent')),
  -- 6-field proof of informed canonical (CTRL-PRIV-CONSENT-001)
  notice_text_hash          TEXT        NOT NULL,
  notice_version            TEXT        NOT NULL,
  locale                    TEXT        NOT NULL
    CHECK (locale IN ('pt-BR','en-US','es-MX')),
  wording_id                TEXT        NOT NULL,
  ui_capture_ts             TEXT        NOT NULL,
  submission_ts             TEXT        NOT NULL,
  -- HMAC signature for stateless verify endpoint
  signature                 TEXT        NOT NULL,
  signature_kid             TEXT        NOT NULL,
  -- Optional screenshot evidence (CTRL-PRIV-CONSENT-006)
  evidence_screenshot_hash  TEXT        NULL,
  -- Optional context (CTRL-PRIV-001 + CTRL-PRIV-012)
  ip_country                TEXT        NULL,
  user_agent_class          TEXT        NULL,
  -- Stale flag: set true when notice major version bumps (AC-006)
  stale_consent             BOOLEAN     NOT NULL DEFAULT FALSE,
  -- Idempotency UNIQUE 5-tuple (PAT-RETRY-IDEMPOTENT-001)
  UNIQUE (tenant_id, subject_id, purpose, notice_text_hash, ui_capture_ts)
);

CREATE INDEX IF NOT EXISTS idx_consent_ledger_subject
  ON consent_ledger (tenant_id, subject_id, submission_ts DESC);

CREATE INDEX IF NOT EXISTS idx_consent_ledger_purpose
  ON consent_ledger (tenant_id, purpose, notice_version);

-- ─── consent_revocation (Lote 9.4 Opus H-05 schema simétrico) ───────────────

CREATE TABLE IF NOT EXISTS consent_revocation (
  revocation_id             TEXT        PRIMARY KEY,
  tenant_id                 TEXT        NOT NULL,
  subject_id                TEXT        NOT NULL,
  subject_id_hash           TEXT        NOT NULL,
  purpose                   TEXT        NOT NULL
    CHECK (purpose IN (
      'service_delivery',
      'account_management',
      'regulatory_compliance',
      'security_monitoring',
      'analytics_aggregated',
      'analytics_personalized',
      'marketing_email',
      'marketing_research',
      'beta_features',
      'third_party_integrations',
      'cross_tenant_benchmarks',
      'training_ml_models'
    )),
  -- FK to grant being revoked (NULL-safe: defensive if purpose never granted)
  revokes_consent_id        TEXT        NULL
    REFERENCES consent_ledger (consent_id),
  -- 6-field proof of informed REVOCATION (mirror schema — Lote 9.4 H-05)
  notice_text_hash          TEXT        NOT NULL,
  notice_version            TEXT        NOT NULL,
  locale                    TEXT        NOT NULL
    CHECK (locale IN ('pt-BR','en-US','es-MX')),
  wording_id                TEXT        NOT NULL,
  ui_capture_ts             TEXT        NOT NULL,
  submission_ts             TEXT        NOT NULL,
  -- HMAC signature (symmetric strength with grant)
  signature                 TEXT        NOT NULL,
  signature_kid             TEXT        NOT NULL,
  -- Cascade tracking (CTRL-PRIV-CONSENT-002 ≤24h SLA)
  cascade_started_at        TEXT        NOT NULL,
  cascade_completed_at      TEXT        NULL,
  cascade_status            TEXT        NOT NULL DEFAULT 'pending'
    CHECK (cascade_status IN ('pending','in_progress','completed','partial_failure','failed')),
  -- Optional context
  ip_country                TEXT        NULL,
  user_agent_class          TEXT        NULL,
  -- Idempotency UNIQUE 5-tuple
  UNIQUE (tenant_id, subject_id, purpose, notice_text_hash, ui_capture_ts)
);

CREATE INDEX IF NOT EXISTS idx_consent_revocation_subject
  ON consent_revocation (tenant_id, subject_id, submission_ts DESC);

CREATE INDEX IF NOT EXISTS idx_consent_revocation_cascade
  ON consent_revocation (cascade_status, cascade_started_at)
  WHERE cascade_status != 'completed';
