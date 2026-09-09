-- B-127: a tenant delete must not make retained audit evidence unevaluable.
--
-- Migration 0107 rejects an audit insert when the tenant is already absent.
-- The inverse ordering remained open: production smoke cleanup could insert
-- valid audit rows and then delete `tenant` directly. That produced the five
-- unexplained tenants / 144 rows measured on 2026-09-09.
--
-- A legitimate erasure always has a durable `dsr_requested` legitimacy anchor
-- before any backend mutation. Require that anchor before deleting a tenant
-- which has retained audit rows. Tenants with no audit history can still be
-- removed, and the DSR pipeline remains replay-safe because its anchor is
-- retained under ADR-S11-013.
--
-- INV-AUTH-MIGRATION-ADDITIVE: one trigger only; no row rewrite or deletion.

CREATE TRIGGER IF NOT EXISTS trg_tenant_delete_requires_dsr_anchor
BEFORE DELETE ON tenant
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM audit_outbox WHERE tenant_id = OLD.tenant_id
)
AND NOT EXISTS (
    SELECT 1 FROM dsr_requested WHERE tenant_id = OLD.tenant_id
)
BEGIN
    SELECT RAISE(ABORT,
        'residency_unprovable: tenant with audit evidence requires dsr_requested before deletion');
END;
