Done. Only the new migration exists in the working tree; no code, query, or other migration was touched, and no build or network was used.

FILE: migrations/d1/0103_audit_outbox_sealed_tail_index.sql
SQL: CREATE INDEX IF NOT EXISTS idx_audit_outbox_sealed_tail ON audit_outbox(tenant_id, region, sequence_number) WHERE emitted_at IS NOT NULL AND sequence_number IS NOT NULL;
HEADER: The header explains WHY the drain's sealed-tail resume query forces a full SCAN + TEMP B-TREE, why each of the four existing indexes is unusable (0001/0078 have the wrong `emitted_at IS NULL` predicate and 0099/0100 carry an `archived_at IS NULL` conjunct the query must not supply because excluding archived rows would fork the chain), that the fix is additive-only per INV-AUTH-MIGRATION-ADDITIVE with no ADR needed, and lists its canonical sources.
