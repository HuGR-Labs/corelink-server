-- Migration 0114: BYOK revocation status + customer audit atomicity.
--
-- The native container uses the D1 REST API, which exposes one statement per
-- request (unlike a Worker binding's db.batch()).  These AFTER UPDATE triggers
-- make the customer-facing audit row part of the same SQLite transaction as
-- the tenant status mutation: either both are committed or neither is.
--
-- Do not move this INSERT back into a second D1 request in the alerter.  A
-- network failure between two requests would otherwise leave a degraded or
-- restored tenant without its customer-visible audit event.

CREATE TRIGGER IF NOT EXISTS trg_tenant_byok_revocation_customer_audit
AFTER UPDATE OF byok_status ON tenant
WHEN OLD.byok_status IS NOT NEW.byok_status
  AND NEW.byok_status IN ('degraded_read_only', 'active')
BEGIN
    INSERT INTO customer_audit_events
        (tenant_id, event_type, actor, target, ts_ms, detail)
    VALUES (
        NEW.tenant_id,
        CASE NEW.byok_status
            WHEN 'degraded_read_only' THEN 'byok.cmk_revoked'
            ELSE 'byok.cmk_restored'
        END,
        'corelink.revocation',
        'byok',
        COALESCE(
            NEW.byok_revoked_at_ms,
            CAST(strftime('%s', 'now') AS INTEGER) * 1000
        ),
        CASE NEW.byok_status
            WHEN 'degraded_read_only' THEN
                'Customer CMK access revoked; tenant writes suspended.'
            ELSE
                'Customer CMK access restored; tenant writes resumed.'
        END
    );
END;
