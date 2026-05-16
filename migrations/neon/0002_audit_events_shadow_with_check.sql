-- CoreLink Neon Postgres — defence-in-depth RLS WITH CHECK fix (Wave 20).
--
-- Canonical sources:
--   - specs/_audits/2026-05-16-wave18-adversarial-review-streamB-neon-shadow.md
--       Finding B-P1-01: original RLS uses USING only — cross-tenant INSERT
--       lands silently if the RealNeonShadowSink driver drops the
--       `SET LOCAL app.current_tenant` GUC. Add WITH CHECK to gate INSERT
--       at the SQL layer per INV-AUTH-SCHEMA-RLS-DEFAULT-ON (CRITICAL).
--   - migrations/neon/0001_audit_events_shadow.sql (canonical shadow schema)
--   - specs/03_architecture/invariant_registry.md
--       INV-AUTH-SCHEMA-RLS-DEFAULT-ON   (CRITICAL; tenant isolation at SQL layer)
--       INV-AUTH-MIGRATION-ADDITIVE      (HIGH; this migration is additive-only —
--                                          ALTER POLICY does not DROP existing
--                                          rows, indexes, or constraints)
--       INV-TENANT-ISOLATION             (CRITICAL; defence-in-depth at app + SQL)
--
-- ## Background
--
-- The Wave-18 `0001_audit_events_shadow.sql` migration created the RLS
-- policies with `USING (...)` only. Per PostgreSQL RLS semantics, `USING`
-- gates SELECT/UPDATE/DELETE visibility but does NOT block INSERTs of
-- rows whose `tenant_id` mismatches `current_setting('app.current_tenant')`.
-- The app-layer `NeonShadowSink::sync_chunk` pin is the first line of
-- defence, but a wiring bug in `RealNeonShadowSink` that drops the GUC
-- SET would let a cross-tenant INSERT land silently (durably written but
-- subsequently invisible to SELECT). This breaks the spec's claim of
-- "tenant isolation at TWO layers" (app + SQL).
--
-- ## Fix
--
-- `ALTER POLICY ... USING (...) WITH CHECK (...)` adds the INSERT/UPDATE
-- gate. The check predicate is identical to the visibility predicate
-- (the canonical pattern from migration 002 membership/pat policies).
--
-- ## Idempotency
--
-- Wrapped in DO-blocks that catch `undefined_object` so re-runs against
-- a database that does not yet have the policies (fresh setup ordering)
-- do not fail. The migration is also safe to run repeatedly: `ALTER
-- POLICY` is idempotent at the level of the resulting policy state
-- (same `USING`, now-added `WITH CHECK`).

-- ---------------------------------------------------------------------------
-- audit_events_shadow — add WITH CHECK to the tenant-isolation policy
-- ---------------------------------------------------------------------------
DO $$
BEGIN
    ALTER POLICY tenant_isolation_audit_events_shadow
        ON audit_events_shadow
        USING (tenant_id = current_setting('app.current_tenant', true)::uuid)
        WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid);
EXCEPTION
    WHEN undefined_object THEN
        -- Policy not yet created (fresh setup ordering); no-op. The
        -- original migration's CREATE POLICY will run with USING + the
        -- caller is expected to re-run this migration after.
        NULL;
END $$;

-- ---------------------------------------------------------------------------
-- audit_shadow_lag — add WITH CHECK to the tenant-isolation policy
-- ---------------------------------------------------------------------------
DO $$
BEGIN
    ALTER POLICY tenant_isolation_audit_shadow_lag
        ON audit_shadow_lag
        USING (tenant_id = current_setting('app.current_tenant', true)::uuid)
        WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid);
EXCEPTION
    WHEN undefined_object THEN
        NULL;
END $$;
