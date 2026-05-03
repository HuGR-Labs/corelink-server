-- CoreLink D1 (Cloudflare SQLite) — migration 0021 for the canonical
-- billing replay audit + idempotency ledger table (S-10 billing
-- pipeline; WI-S10-006 Replay Forensic Endpoint + Role + Audit Trail).
--
-- Canonical sources:
--   - specs/04_sprints/S10/work_items/WI-S10-006-replay-forensic-endpoint-role-audit-trail.md §6.1 + §13
--   - specs/04_sprints/S10/_spec_contract.md §5.6 (R-S10-12/13) + §8 (INV-BILLING-REPLAYABLE-FROM-EVENTS HIGH)
--   - specs/03_architecture/invariant_registry.md INV-BILLING-REPLAYABLE-FROM-EVENTS (§3.12 line 167) + INV-AUDIT-APPEND-ONLY
--   - specs/03_architecture/security_model.md CTRL-AUTHZ-001 + CTRL-AUTHZ-002 (billing_forensics_admin role + MFA)
--   - specs/03_architecture/adrs/ADR-0036-d1-schema-migration-governance.md
--
-- Schema rationale:
--   - `billing_replay_audit` mirrors the in-memory
--     `corelink-billing-replay::InMemoryReplayIdempotencyLedger` row
--     storage at the durable layer; each replay invocation INSERTs one
--     row per canonical UUIDv7 `request_id` so the SOC 2 CC1.4 + GAAP
--     ASC 606 + GDPR Art. 22 (automated decision review) 7-year evidence
--     trail is unbroken.
--   - PRIMARY KEY `(request_id)` UNIQUE pins the canonical idempotency
--     contract (WI-S10-006 §1 invariant 6): same request_id → same
--     outcome by construction; a re-submission of the same request_id
--     paired with a divergent (tenant_id, billing_period, reason)
--     tuple is a SEV-1 forensic anomaly the orchestrator surfaces via
--     the `corelink-billing-replay::ReplayIdempotencyError::DivergentPayload`
--     surface.
--   - `decision` column pins the canonical 4-element decision taxonomy
--     (`authorized` / `denied_403` / `dry_run_plan` / `executed`) per
--     WI-S10-006 §6.1; CHECK constraint enforces; runtime enforcement
--     via the typed `ReplayDecision` enum at the trait surface.
--   - `reason` column pins the canonical 4-element reason taxonomy
--     (`drift_investigation` / `customer_dispute` / `compliance_audit`
--     / `dry_run`) per WI-S10-006 §6.1; CHECK constraint enforces.
--   - `presented_role` column captures the role string the request
--     presented (logged for forensic trail; never used for
--     authorization). The canonical authorized role is
--     `billing_forensics_admin` per CTRL-AUTHZ-002 (separate from
--     regular admin so the audit-grade replay capability is least-
--     privilege-bounded).
--   - `layer_drift_summary` column pins the canonical 5-element layer-
--     drift taxonomy (`all_layers_match` / `layer1_diverged` /
--     `layer2_diverged` / `layer3_diverged` / `multiple_layers_diverged`)
--     per WI-S10-006 §6.1; nullable on the `denied_403` + `dry_run_plan`
--     arms (canonical null-when-not-applicable convention); CHECK
--     constraint enforces validity when non-null.
--   - `layer1_total_qty_reconstructed`, `layer2_total_qty_reconstructed`,
--     `layer3_total_qty_reconstructed` store the canonical 3-layer
--     reconstructed totals as TEXT (u128 fits in 39 decimal digits;
--     stored as TEXT to avoid SQLite INTEGER overflow at u128 scale).
--     Production wiring at the trait surface uses `u128` internally;
--     the storage layer converts at INSERT-time.
--
-- Invariants enforced at storage layer:
--   - INV-BILLING-REPLAYABLE-FROM-EVENTS (HIGH; invariant_registry
--     §3.12 line 167): every replay invocation lands one audit row;
--     30-day clean streak prerequisite per sprint contract §6 DoD lifts
--     the canonical `decision = 'executed' AND layer_drift_summary =
--     'all_layers_match'` count over the trailing window.
--   - INV-AUDIT-APPEND-ONLY (CRITICAL, TLA+ proven Lote 6.2; S-09
--     inheritance): the table is INSERT-only at the storage layer;
--     ON CONFLICT DO NOTHING returns 0 rows affected → orchestrator
--     dispatches to the canonical idempotent-rerun arm.
--   - INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER (HIGH; lift from S-07 P1-1
--     fix + Lote 10.6bis pattern + S-09 inheritance): the canonical
--     `corelink.billing_replay.{request_authorized, request_denied,
--     dry_run_planned, executed, layer_diverged}` audit chain emits
--     BEFORE this table's INSERT (production wiring at WI-S10-007
--     binds the audit envelope to the S-09 chain APPEND surface).
--   - CTRL-AUTHZ-002 (security_model.md): the canonical
--     `billing_forensics_admin` role (separate from regular admin) is
--     enforced at the production wiring's Tower middleware BEFORE the
--     orchestrator dispatches; the trait surface here treats the role
--     as opaque + matches against the canonical string by exact
--     equality.
--
-- Conventions (mirror migrations/d1/0001..0020):
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
-- Backfill plan: NONE. The table starts empty; rows are inserted by
-- the canonical billing-replay orchestrator (every replay invocation
-- per UUIDv7 request_id).
--
-- Migration runner: see scripts/migrate_d1.sh.

-- ---------------------------------------------------------------------------
-- billing_replay_audit — durable mirror of the canonical replay
-- idempotency ledger. PRIMARY KEY (request_id) UNIQUE pins the
-- canonical idempotency contract at the storage layer; a duplicate
-- re-submission of the same request_id retrieves this row + short-
-- circuits the orchestrator to the `Authorized { idempotent_replay =
-- true }` arm without re-running the pipeline.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS billing_replay_audit (
  -- Canonical UUIDv7 request id (the canonical idempotency ledger key;
  -- time-ordered for forensic trail localization). Hyphenated
  -- lowercase 36-char form per S-09 inheritance.
  request_id                  TEXT     NOT NULL,

  -- Tenant id (canonical UUIDv7 hyphenated lowercase from the auth
  -- middleware; S-03 Lote 10.4bis enforcement — never the request
  -- body). Per-tenant replay scoping.
  tenant_id                   TEXT     NOT NULL,

  -- Admin pubkey of the requesting principal (canonical UUIDv7
  -- hyphenated lowercase; S-03 inheritance).
  requested_by                TEXT     NOT NULL,

  -- Role string the request presented (logged for forensic trail;
  -- never used for authorization). Canonical authorized role is
  -- `billing_forensics_admin` per CTRL-AUTHZ-002.
  presented_role              TEXT     NOT NULL,

  -- Canonical billing period (`YYYY-MM` per WI-S10-001 §1
  -- validate_billing_period guard).
  billing_period              TEXT     NOT NULL,

  -- Canonical 4-element reason taxonomy: `drift_investigation` /
  -- `customer_dispute` / `compliance_audit` / `dry_run` per
  -- WI-S10-006 §6.1 (CHECK constraint enforces; runtime enforcement
  -- via the typed ReplayReason enum at the trait surface).
  reason                      TEXT     NOT NULL,

  -- Canonical 4-element decision taxonomy: `authorized` /
  -- `denied_403` / `dry_run_plan` / `executed` per WI-S10-006 §6.1
  -- (CHECK constraint enforces).
  decision                    TEXT     NOT NULL,

  -- Canonical 5-element layer-drift summary: `all_layers_match` /
  -- `layer1_diverged` / `layer2_diverged` / `layer3_diverged` /
  -- `multiple_layers_diverged` per WI-S10-006 §6.1 (nullable on
  -- `denied_403` + `dry_run_plan` arms; CHECK constraint enforces
  -- validity when non-null).
  layer_drift_summary         TEXT,

  -- 3-layer reconstructed totals (u128 fits in 39 decimal digits;
  -- stored as TEXT to avoid SQLite INTEGER overflow at u128 scale).
  -- Empty string sentinel for the `denied_403` + `dry_run_plan` arms
  -- (canonical empty-when-not-applicable convention; CHECK constraint
  -- pins the empty-or-numeric form).
  layer1_total_qty_reconstructed TEXT  NOT NULL,
  layer2_total_qty_reconstructed TEXT  NOT NULL,
  layer3_total_qty_reconstructed TEXT  NOT NULL,

  -- Wall-clock instant of the canonical replay invocation that
  -- landed this row (Unix epoch ms; canonical no `_ms` suffix per
  -- Lote 10.7bis P0-3). Pinned for forensic reconstruction.
  landed_at                   INTEGER  NOT NULL,

  PRIMARY KEY (request_id),

  -- chk_billing_replay_audit_request_id_uuid_len: UUIDv7 hyphenated form.
  CHECK (length(request_id) = 36),

  -- chk_billing_replay_audit_tenant_id_uuid_len.
  CHECK (length(tenant_id) = 36),

  -- chk_billing_replay_audit_requested_by_uuid_len.
  CHECK (length(requested_by) = 36),

  -- chk_billing_replay_audit_presented_role_non_empty.
  CHECK (length(presented_role) >= 1),

  -- chk_billing_replay_audit_billing_period_yyyy_mm: canonical 7-char
  -- format `YYYY-MM` (e.g. `2026-05`).
  CHECK (length(billing_period) = 7),

  -- chk_billing_replay_audit_reason_canonical: reason MUST be one
  -- of the canonical 4-element taxonomy per WI-S10-006 §6.1.
  CHECK (reason IN (
    'drift_investigation',
    'customer_dispute',
    'compliance_audit',
    'dry_run'
  )),

  -- chk_billing_replay_audit_decision_canonical: decision MUST be one
  -- of the canonical 4-element taxonomy per WI-S10-006 §6.1.
  CHECK (decision IN (
    'authorized',
    'denied_403',
    'dry_run_plan',
    'executed'
  )),

  -- chk_billing_replay_audit_layer_drift_summary_canonical:
  -- layer_drift_summary MUST be NULL or one of the canonical
  -- 5-element taxonomy per WI-S10-006 §6.1.
  CHECK (
    layer_drift_summary IS NULL
    OR layer_drift_summary IN (
      'all_layers_match',
      'layer1_diverged',
      'layer2_diverged',
      'layer3_diverged',
      'multiple_layers_diverged'
    )
  ),

  -- chk_billing_replay_audit_landed_at_non_negative.
  CHECK (landed_at >= 0)
);

-- Index: per-tenant per-period chronological scan — Compliance Officer
-- + Finance triage forensic surface + 30-day clean streak gauge tracker
-- (sprint contract §6 DoD prerequisite).
CREATE INDEX IF NOT EXISTS idx_billing_replay_audit_tenant_period_landed
  ON billing_replay_audit(tenant_id, billing_period, landed_at);

-- Index: per-decision-arm scan — dashboard widget (4-element taxonomy
-- bucket) + alert-rule fan-out for SEV-3 (request_denied) +
-- SEV-2 (layer_diverged) audit pattern.
CREATE INDEX IF NOT EXISTS idx_billing_replay_audit_per_decision_arm
  ON billing_replay_audit(decision, landed_at);

-- Index: per-requesting-admin scan — forensic abuse pattern detection
-- (compromised billing_forensics_admin replays > 100/day per sprint
-- contract §15 R-007 mitigation).
CREATE INDEX IF NOT EXISTS idx_billing_replay_audit_per_requested_by
  ON billing_replay_audit(requested_by, landed_at);
