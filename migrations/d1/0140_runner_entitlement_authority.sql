-- D1 ordinal 0140 — same-second replacement identity is authorized only by
-- the provider-backed current customer subscription lookup, never by an
-- opaque Stripe id's lexical order.

ALTER TABLE runner_entitlement_reconcile_fence
    ADD COLUMN authority_is_current INTEGER NOT NULL DEFAULT 0
    CHECK (authority_is_current IN (0, 1));
