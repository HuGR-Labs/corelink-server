-- D1 ordinal 0138 — #1844 / #1639.
--
-- 0137 is already a published migration on the stacked #1639 path. Extend
-- its durable fence additively so existing rows remain valid: historical
-- fences are revokes (0), which permits a successor grant to establish the
-- replacement identity while still blocking predecessor grants.

ALTER TABLE runner_entitlement_reconcile_fence
    ADD COLUMN is_granting INTEGER NOT NULL DEFAULT 0
    CHECK (is_granting IN (0, 1));
