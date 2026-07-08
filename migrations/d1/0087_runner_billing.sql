-- 0087_runner_billing.sql
--
-- Self-serve RUNNER subscription ↔ tenant mapping (`runner_billing`): the
-- authoritative record that lets the live Stripe webhook (apps/signup-worker
-- src/webhooks/stripe.ts) SEED and REVOKE per-tenant Runners entitlement
-- (`runners_entitlement`, migrations 0070/0072) on the runner Stripe
-- subscription lifecycle. This migration lands that read/write model — and
-- NOTHING else.
--
-- ## Why a DEDICATED table (NOT tenant_billing)
--
-- `tenant_billing` is ONE ROW PER TENANT (`ON CONFLICT (tenant_id)`,
-- stripe.ts upsertBillingPaid). A tenant may hold BOTH a cache subscription
-- AND a runner subscription at the same time, so a single tenant-keyed row
-- cannot map both without one clobbering the other. `runner_billing` is keyed
-- by the RUNNER Stripe subscription id, so it coexists with the cache row and
-- never collides with it: cache-sub → tenant_billing, runner-sub →
-- runner_billing.
--
-- ## Why keyed by `runner_subscription_id` → tenant (the disambiguation)
--
-- Some Stripe events that must revoke entitlement carry NO price (notably
-- `invoice.payment_failed`, whose invoice object has no line-item price in the
-- webhook payload) — so the webhook cannot tell a runner-sub failure from a
-- cache-sub failure from the event alone. Keying `runner_billing` by the
-- subscription id lets the handler resolve "is THIS subscription a runner
-- subscription?" with a single lookup (`… WHERE runner_subscription_id = ?`)
-- and, if so, revoke the tenant's runner entitlement (resolving tenant_id
-- through this table). That disambiguation is the reason this table exists.
--
-- ## Writer / reader authority (single)
--
-- WRITTEN ONLY by the signup-worker Stripe webhook (the authoritative live
-- handler): it upserts a row on a runner checkout / subscription create/update
-- and marks the row's status on cancel / terminal payment failure. The
-- entitlement writers (`upsertRunnersEntitlementBySubscription` /
-- `revokeRunnersEntitlementBySubscription`) JOIN through this table to resolve
-- the tenant, then seed or delete the `runners_entitlement` row. No hot path
-- reads this table — the runners fabric reads `runners_entitlement` via the
-- container `/internal/v1/auth/introspect` lookup (migration 0070), never this
-- billing mirror.
--
-- ## Columns
--
--   runner_subscription_id  TEXT PRIMARY KEY — the RUNNER Stripe subscription id
--                                     (`sub_…`). PK so a redelivered create/update
--                                     is an idempotent ON CONFLICT upsert and the
--                                     subscription↔tenant map is 1:1.
--   tenant_id               TEXT NOT NULL  — the CoreLink tenant UUID that owns the
--                                     runner subscription (from the checkout
--                                     session's metadata[tenant_id]). Indexed
--                                     below for the tenant-side join.
--   plan                    TEXT NOT NULL  — the runner tier label
--                                     (runner_starter..runner_max), for operator
--                                     forensics + the entitlement source.
--   status                  TEXT NOT NULL  — the Stripe subscription status
--                                     (active|trialing|past_due|canceled|…),
--                                     mirrored so a later event that lacks a price
--                                     can still be classified.
--   stripe_customer_id      TEXT           — the Stripe customer (nullable; some
--                                     subscription payloads omit it).
--   created_at_ms           INTEGER NOT NULL — provisioning wall-clock (Unix epoch
--                                     ms). Matches the wider D1 `*_at_ms`
--                                     convention.
--   updated_at_ms           INTEGER NOT NULL — last-mutation wall-clock (Unix epoch
--                                     ms); advanced on every status upsert.
--
-- ## Idempotency / additive-only / replay posture
--
-- `CREATE TABLE IF NOT EXISTS` + `CREATE INDEX IF NOT EXISTS` let the migration
-- replay safely. The migration is additive-only (INV-AUTH-MIGRATION-ADDITIVE):
-- it adds a NEW table and a NEW index and never alters/drops/renames an existing
-- object, so it needs no ADR waiver and no `-- additive-allowed:` suppression.
-- No FK to `tenant` is declared (D1 FKs are logical/per-connection inconsistent
-- in CF Workers — see 0002/0003 — matching the sibling 0070 runners table).

CREATE TABLE IF NOT EXISTS runner_billing (
    runner_subscription_id TEXT PRIMARY KEY,
    tenant_id              TEXT NOT NULL,
    plan                   TEXT NOT NULL,   -- runner_starter..runner_max
    status                 TEXT NOT NULL,   -- active|trialing|past_due|canceled|...
    stripe_customer_id     TEXT,
    created_at_ms          INTEGER NOT NULL,
    updated_at_ms          INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_runner_billing_tenant ON runner_billing(tenant_id);
