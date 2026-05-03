-- CoreLink D1 (Cloudflare SQLite) — migration 0020 for the canonical
-- quota state machine durable mirror table (S-10 billing pipeline;
-- WI-S10-005 Quota State Machine 5-state + Idempotent Transitions +
-- 3-Invoice-Failure Suspension + S-13 Email Defer).
--
-- Canonical sources:
--   - specs/04_sprints/S10/work_items/WI-S10-005-quota-state-machine-overage-email.md §6.1.2 + §13
--   - specs/04_sprints/S10/_spec_contract.md §5.5 (R-S10-10/11)
--   - specs/03_architecture/invariant_registry.md INV-AVAIL-ISOLATION (§3.8) + INV-AUDIT-APPEND-ONLY
--   - specs/03_architecture/security_model.md CTRL-BILLING-001 + CTRL-AUTHZ-001 + CTRL-AUTHZ-002
--   - specs/03_architecture/adrs/ADR-0036-d1-schema-migration-governance.md
--
-- Schema rationale:
--   - `quota_fsm_state` mirrors the in-memory
--     `corelink-quota-fsm::InMemoryQuotaFsmStore` `BTreeMap<Uuid,
--     QuotaFsmStateRow>` set membership at the durable layer; one row
--     per tenant captures the canonical 5-state machine + the
--     invoice-failure counter.
--   - PRIMARY KEY `(tenant_id)` UNIQUE pins INV-AVAIL-ISOLATION at
--     storage layer (per-tenant scoping; cross-tenant injection blocked
--     at the row level + the WI-S10-007 production wiring's TenantCtx
--     middleware enforces the auth-derived tenant_id at request
--     admission per S-03 inheritance).
--   - `current_state` column pins the canonical 5-element state
--     taxonomy (`within_plan` / `soft_warning_80pct` /
--     `soft_warning_95pct` / `over_quota_100pct` /
--     `suspended_for_non_payment`) per WI-S10-005 §1; CHECK constraint
--     enforces; runtime enforcement via the typed `QuotaState` enum
--     at the trait surface.
--   - `invoice_failure_count` mirrors the in-memory
--     `InvoiceFailureCount` saturating-u32 counter; the canonical
--     suspension threshold is 3 (default per WI brief + sprint contract
--     §15 R-009 abuse detection inheritance) but the production wiring
--     reads the runtime config (`CORELINK_QUOTA_FSM_*` env override) so
--     the column itself stores the unsaturated counter value (the
--     state-machine threshold check happens at the trait surface;
--     storage tracks the actual count for forensic reconstruction).
--   - `updated_at` Unix epoch ms wall-clock instant of the last
--     mutation; pinned for forensic reconstruction + the operator
--     triage path.
--
-- Invariants enforced at storage layer:
--   - INV-AVAIL-ISOLATION (HIGH; invariant_registry §3.8): PRIMARY KEY
--     (tenant_id) UNIQUE prevents cross-tenant state row aliasing; the
--     production wiring's TenantCtx middleware (S-03 inheritance)
--     additionally enforces the auth-derived tenant_id at request
--     admission so cross-tenant injection is structurally impossible.
--   - INV-AUDIT-APPEND-ONLY (CRITICAL, TLA+ proven Lote 6.2; S-09
--     inheritance): the canonical state-mutation audit row lands BEFORE
--     this UPSERT per the orchestrator's fail-CLOSED envelope; audit
--     failure aborts the run + the row remains at its pre-call value
--     so the auditor evidence trail is unbroken.
--   - CTRL-BILLING-001 (security_model.md): financial integrity quota
--     enforcement; bypass = uncontrolled customer cost (the canonical
--     5-state machine is the customer-cost-protection contract +
--     service-protection rate limit per WI-S10-005 §1).
--   - CTRL-AUTHZ-001 + CTRL-AUTHZ-002 (security_model.md): the
--     reinstatement endpoint is `billing_admin`-protected at the
--     production wiring's Tower middleware; the `current_state =
--     'suspended_for_non_payment'` flag flip back to a
--     utilization-derived state requires the operator path + a
--     mandatory audit row.
--
-- Conventions (mirror migrations/d1/0001..0019):
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
-- Backfill plan: NONE. The table starts empty; rows are inserted on
-- first per-tenant transition through the canonical state machine
-- (genesis WithinPlan + 0 failures is the implicit pre-call state per
-- the orchestrator's `lookup_or_genesis()` primitive).
--
-- Migration runner: see scripts/migrate_d1.sh.

-- ---------------------------------------------------------------------------
-- quota_fsm_state — durable mirror of the canonical per-tenant quota
-- state-machine row. PRIMARY KEY (tenant_id) UNIQUE pins
-- INV-AVAIL-ISOLATION at the storage layer; the orchestrator's
-- canonical genesis arm short-circuits on missing rows (the lookup
-- returns None → the orchestrator treats this as the canonical
-- WithinPlan + 0 failures pre-call state).
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS quota_fsm_state (
  -- Tenant id (canonical UUIDv7 hyphenated lowercase from the auth
  -- middleware; S-03 Lote 10.4bis enforcement — never the request
  -- body). Per-tenant state-machine scoping.
  tenant_id                   TEXT     NOT NULL,

  -- Canonical 5-element state taxonomy: `within_plan` /
  -- `soft_warning_80pct` / `soft_warning_95pct` / `over_quota_100pct` /
  -- `suspended_for_non_payment` per WI-S10-005 §1 (CHECK constraint
  -- enforces; runtime enforcement via the typed QuotaState enum at
  -- the trait surface).
  current_state               TEXT     NOT NULL,

  -- Invoice-failure counter (cleared by `reinstate()`; bumped by
  -- `record_invoice_failure()`). The canonical suspension threshold
  -- is 3 (default) but the column stores the unsaturated counter
  -- value so forensic reconstruction can replay the exact webhook
  -- delivery sequence.
  invoice_failure_count       INTEGER  NOT NULL DEFAULT 0,

  -- Wall-clock instant of the last mutation (Unix epoch ms; canonical
  -- no `_ms` suffix per Lote 10.7bis P0-3).
  updated_at                  INTEGER  NOT NULL,

  PRIMARY KEY (tenant_id),

  -- chk_quota_fsm_state_tenant_id_uuid_len: UUIDv7 hyphenated form.
  CHECK (length(tenant_id) = 36),

  -- chk_quota_fsm_state_current_state_canonical: current_state MUST
  -- be one of the canonical 5-element taxonomy per WI-S10-005 §1.
  CHECK (current_state IN (
    'within_plan',
    'soft_warning_80pct',
    'soft_warning_95pct',
    'over_quota_100pct',
    'suspended_for_non_payment'
  )),

  -- chk_quota_fsm_state_invoice_failure_count_non_negative: the
  -- counter is monotonically non-decreasing in the production wiring's
  -- record_invoice_failure path; reinstate() resets to 0; no negative
  -- value is ever expected.
  CHECK (invoice_failure_count >= 0),

  -- chk_quota_fsm_state_updated_at_non_negative.
  CHECK (updated_at >= 0)
);

-- Index: per-state-arm scan — dashboard widget (5-element taxonomy
-- bucket) + alert-rule fan-out for SEV-1 suspension counter +
-- operator-triage UI.
CREATE INDEX IF NOT EXISTS idx_quota_fsm_state_per_state_arm
  ON quota_fsm_state(current_state);

-- Index: chronological scan — operator triage UI + audit-of-audit
-- forensic reconstruction (ordered by mutation time).
CREATE INDEX IF NOT EXISTS idx_quota_fsm_state_chronological
  ON quota_fsm_state(updated_at);
