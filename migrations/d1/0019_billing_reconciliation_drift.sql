-- CoreLink D1 (Cloudflare SQLite) — migration 0019 for the canonical
-- billing reconciliation drift-history + Stripe-submission flag tables
-- (S-10 billing pipeline; WI-S10-004 Reconciliation Worker 3-Layer +
-- Drift Alerts SEV-1/2 + Reconciliation Reports R2 7y).
--
-- Canonical sources:
--   - specs/04_sprints/S10/work_items/WI-S10-004-reconciliation-worker-3-layer-drift-alerts.md §6.1.7 + §13
--   - specs/04_sprints/S10/_spec_contract.md §5.4 (R-S10-8/9) + §8 (NEW INV-BILLING-RECONCILE-3-LAYER)
--   - specs/03_architecture/invariant_registry.md INV-BILLING-RECONCILE-3-LAYER (§3.12 line 166) + INV-BILLING-NO-LOSS (§3.9 line 136) + INV-BILLING-NO-DUP (§3.9 line 137) + INV-AUDIT-APPEND-ONLY
--   - specs/03_architecture/security_model.md CTRL-BILLING-001
--   - specs/03_architecture/adrs/ADR-0036-d1-schema-migration-governance.md
--
-- Schema rationale:
--   - `billing_reconciliation_drift` mirrors the in-memory
--     `corelink-billing-reconcile::InMemoryDriftHistoryLedger` set
--     membership at the durable layer; each daily reconcile cron tick
--     INSERTs one row per `(tenant_id, billing_period)` so the
--     SOC 2 CC1.4 + GAAP ASC 606 7-year evidence trail is unbroken.
--   - PRIMARY KEY `(tenant_id, billing_period, run_started_at)` UNIQUE
--     pins INV-BILLING-NO-DUP at storage layer; idempotent re-run over
--     the canonical watermark (production wiring's CF Cron DO uses
--     `corelink_time::next_month_first_utc_midnight()` so the watermark
--     is canonical per cron tick — re-runs produce the same row by
--     construction).
--   - `decision` column pins the canonical 5-element decision taxonomy
--     (`no_drift` / `auto_fixed` / `ticket_sev3` / `page_sev2` /
--     `page_sev1_auto_paused`) per WI-S10-004 §6.1; CHECK constraint
--     enforces; runtime enforcement via the typed
--     `ReconcileDecision` enum at the trait surface.
--   - `primary_layer` column pins the canonical 3-element layer
--     taxonomy (`layer1_emit` / `layer2_aggregate` / `layer3_stripe`)
--     per WI-S10-004 §1; nullable on the `no_drift` arm (canonical
--     null-when-not-applicable convention).
--   - `max_drift_pct_e9` stores the drift percentage as a fixed-point
--     `INTEGER` (multiplied by `10^9` to avoid SQLite REAL precision
--     issues; the canonical fractional unit `0.001 = 1_000_000` in
--     this column). Production wiring at the trait surface uses `f64`
--     internally; the storage layer converts at INSERT-time.
--   - `stripe_submission_state` mirrors the in-memory
--     `corelink-billing-reconcile::InMemoryStripeSubmissionControl`
--     `BTreeSet`; PRIMARY KEY `(tenant_id, billing_period)` UNIQUE
--     pins idempotent SEV-1 re-fire (per Stripe-pause API spec the
--     pause flag is set once per period until operator clearance).
--   - `paused_at` Unix epoch ms wall-clock instant of the canonical
--     SEV-1 audit row that triggered the pause; pinned for forensic
--     reconstruction.
--
-- Invariants enforced at storage layer:
--   - INV-BILLING-NO-DUP (HIGH; invariant_registry §3.9 line 137):
--     PRIMARY KEY (tenant_id, billing_period, run_started_at) UNIQUE
--     on `billing_reconciliation_drift` prevents duplicate
--     reconciliation rows; PRIMARY KEY (tenant_id, billing_period)
--     UNIQUE on `stripe_submission_state` prevents duplicate
--     pause-flag rows under SEV-1 idempotent re-fire.
--   - INV-BILLING-RECONCILE-3-LAYER (HIGH; invariant_registry §3.12
--     line 166): every cron tick lands one drift-history row per
--     (tenant, billing_period); 30-day clean streak prerequisite per
--     sprint contract §6 DoD lifts the canonical `decision = 'no_drift'`
--     count over the trailing window.
--   - INV-AUDIT-APPEND-ONLY (CRITICAL, TLA+ proven Lote 6.2; S-09
--     inheritance): both tables are INSERT-only at the storage layer;
--     ON CONFLICT DO NOTHING returns 0 rows affected → orchestrator
--     dispatches to the canonical idempotent-rerun arm.
--   - CTRL-BILLING-001 (security_model.md): financial integrity
--     3-layer enforcement; drift > 0.1% triggers invoice freeze (Stripe
--     submission pause) until operator clearance.
--
-- Conventions (mirror migrations/d1/0001..0018):
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
-- the canonical billing-reconcile orchestrator (every daily cron tick
-- per (tenant, billing_period)).
--
-- Migration runner: see scripts/migrate_d1.sh.

-- ---------------------------------------------------------------------------
-- billing_reconciliation_drift — durable mirror of the canonical
-- drift-history ledger. PRIMARY KEY (tenant_id, billing_period,
-- run_started_at) UNIQUE pins INV-BILLING-NO-DUP at the storage layer;
-- the orchestrator short-circuits on the second sight (idempotent
-- re-run over the canonical watermark) so the auditor evidence trail
-- is unbroken.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS billing_reconciliation_drift (
  -- Tenant id (canonical UUIDv7 hyphenated lowercase from the auth
  -- middleware; S-03 Lote 10.4bis enforcement — never the request
  -- body). Per-tenant reconciliation scoping.
  tenant_id                   TEXT     NOT NULL,

  -- Canonical billing period (`YYYY-MM` per WI-S10-001 §1
  -- validate_billing_period guard).
  billing_period              TEXT     NOT NULL,

  -- Run-started watermark (Unix epoch ms; pinned BEFORE any layer
  -- query per the canonical RunStarted audit envelope; canonical
  -- corelink_time::next_month_first_utc_midnight() boundary inherited
  -- from Lote 10.8bis P0-D + WI-S10-002).
  run_started_at              INTEGER  NOT NULL,

  -- Canonical 5-element decision taxonomy: `no_drift` / `auto_fixed` /
  -- `ticket_sev3` / `page_sev2` / `page_sev1_auto_paused` per
  -- WI-S10-004 §6.1 (CHECK constraint enforces; runtime enforcement
  -- via the typed ReconcileDecision enum at the trait surface).
  decision                    TEXT     NOT NULL,

  -- Canonical 3-element primary-layer taxonomy: `layer1_emit` /
  -- `layer2_aggregate` / `layer3_stripe` per WI-S10-004 §1 (nullable
  -- on the `no_drift` arm; CHECK constraint enforces validity when
  -- non-null).
  primary_layer               TEXT,

  -- Maximum drift percentage observed across the three pairwise
  -- layer comparisons, scaled by `10^9` and stored as INTEGER to
  -- avoid SQLite REAL precision issues (`0.001 = 1_000_000`;
  -- `0.01 = 10_000_000`; `1.0 = 1_000_000_000`).
  max_drift_pct_e9            INTEGER  NOT NULL,

  -- Drift record count at the moment of the gate decision (the
  -- "absolute count" arm of the dual-condition auto-fix gate; mirrors
  -- WI-S06 reconcile + the canonical Lote 10.6bis P0-6 scale-invariant
  -- pattern).
  drift_record_count          INTEGER  NOT NULL,

  -- Free-form context (e.g. drift_pct value, primary layer). Reserved
  -- for adversarial debug + Finance-triage dashboard widget grouping.
  context                     TEXT     NOT NULL,

  PRIMARY KEY (tenant_id, billing_period, run_started_at),

  -- chk_billing_recon_drift_tenant_id_uuid_len: UUIDv7 hyphenated form.
  CHECK (length(tenant_id) = 36),

  -- chk_billing_recon_drift_billing_period_yyyy_mm: canonical 7-char
  -- format `YYYY-MM` (e.g. `2026-05`).
  CHECK (length(billing_period) = 7),

  -- chk_billing_recon_drift_run_started_at_non_negative.
  CHECK (run_started_at >= 0),

  -- chk_billing_recon_drift_decision_canonical: decision MUST be one
  -- of the canonical 5-element taxonomy per WI-S10-004 §6.1.
  CHECK (decision IN (
    'no_drift',
    'auto_fixed',
    'ticket_sev3',
    'page_sev2',
    'page_sev1_auto_paused'
  )),

  -- chk_billing_recon_drift_primary_layer_canonical: primary_layer
  -- MUST be NULL or one of the canonical 3-element taxonomy per
  -- WI-S10-004 §1.
  CHECK (
    primary_layer IS NULL
    OR primary_layer IN ('layer1_emit', 'layer2_aggregate', 'layer3_stripe')
  ),

  -- chk_billing_recon_drift_max_drift_pct_bounded: drift is a
  -- fractional unit `0.0 ≤ drift ≤ 1.0`; scaled by 10^9 the upper
  -- bound is 1_000_000_000.
  CHECK (max_drift_pct_e9 >= 0 AND max_drift_pct_e9 <= 1000000000),

  -- chk_billing_recon_drift_record_count_non_negative.
  CHECK (drift_record_count >= 0),

  -- chk_billing_recon_drift_context_non_empty.
  CHECK (length(context) >= 1)
);

-- Index: per-tenant per-period chronological scan — Finance-triage
-- forensic surface + 30-day clean streak gauge tracker (sprint contract
-- §6 DoD prerequisite).
CREATE INDEX IF NOT EXISTS idx_billing_recon_drift_tenant_period_run
  ON billing_reconciliation_drift(tenant_id, billing_period, run_started_at);

-- Index: per-decision-arm scan — dashboard widget (5-element taxonomy
-- bucket) + alert-rule fan-out for SEV-1/SEV-2/SEV-3 PagerDuty dispatch.
CREATE INDEX IF NOT EXISTS idx_billing_recon_drift_per_decision_arm
  ON billing_reconciliation_drift(decision, run_started_at);

-- ---------------------------------------------------------------------------
-- stripe_submission_state — durable mirror of the canonical
-- Stripe-submission control surface flag. PRIMARY KEY (tenant_id,
-- billing_period) UNIQUE pins idempotent SEV-1 re-fire (per
-- WI-S10-004 §1: SEV-1 arm is idempotent across re-runs until
-- operator clearance flips the flag back to submission_open).
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS stripe_submission_state (
  -- Tenant id (canonical UUIDv7 hyphenated lowercase).
  tenant_id                   TEXT     NOT NULL,

  -- Canonical billing period (`YYYY-MM`).
  billing_period              TEXT     NOT NULL,

  -- Wall-clock instant of the SEV-1 audit row that triggered the
  -- pause (Unix epoch ms; canonical no `_ms` suffix per Lote 10.7bis
  -- P0-3). Pinned for forensic reconstruction.
  paused_at                   INTEGER  NOT NULL,

  -- Free-form reason context (e.g. the canonical SEV-1 audit row
  -- context: drift_pct, primary_layer). Reserved for operator triage.
  reason                      TEXT     NOT NULL,

  PRIMARY KEY (tenant_id, billing_period),

  -- chk_stripe_submission_state_tenant_id_uuid_len.
  CHECK (length(tenant_id) = 36),

  -- chk_stripe_submission_state_billing_period_yyyy_mm.
  CHECK (length(billing_period) = 7),

  -- chk_stripe_submission_state_paused_at_non_negative.
  CHECK (paused_at >= 0),

  -- chk_stripe_submission_state_reason_non_empty.
  CHECK (length(reason) >= 1)
);

-- Index: chronological scan — operator triage UI + clean-streak
-- gauge tracker (clean streak resets when a row lands here).
CREATE INDEX IF NOT EXISTS idx_stripe_submission_state_paused_chronological
  ON stripe_submission_state(paused_at);
