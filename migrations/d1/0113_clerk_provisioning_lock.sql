-- B-222: one durable provisioning lease per Clerk user.
--
-- A UNIQUE tenant.clerk_user_id prevents duplicate tenants, but it does not
-- serialize the later PAT mint: two concurrent webhook deliveries can both
-- observe the tenant before either has inserted a PAT. This lease is the
-- durable claim at the mint boundary. A short lease lets a crashed delivery
-- be retried without leaving the account permanently stuck.
CREATE TABLE IF NOT EXISTS clerk_provisioning_lock (
    clerk_user_id   TEXT    NOT NULL PRIMARY KEY,
    state           TEXT    NOT NULL CHECK (state IN ('in_progress', 'complete')),
    lease_until_ms  INTEGER NOT NULL,
    created_ms      INTEGER NOT NULL,
    updated_ms      INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_clerk_provisioning_lock_lease
    ON clerk_provisioning_lock (state, lease_until_ms);
