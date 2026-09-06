-- 0112_githugr_tenant_org_map.sql
-- Keep the separate githugr issuer's identity map out of the CoreLink-Clerk
-- `tenant_org_map` table. CoreLink signup-worker remains that table's single
-- writer; an issuer-specific table makes cross-issuer PK collisions impossible.
CREATE TABLE IF NOT EXISTS githugr_tenant_org_map (
    githugr_subject TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_githugr_tenant_org_map_tenant_id
    ON githugr_tenant_org_map (tenant_id);
