/** Runner billing and entitlement persistence writers. */
import type { D1DatabaseLike } from "./billing_checkout";

/**
 * Upsert the `runner_billing` row that maps a RUNNER Stripe subscription →
 * tenant (migration 0087). Keyed by `runner_subscription_id` (PK) so it does
 * NOT clobber the one-row-per-tenant `tenant_billing` cache row and so a tenant
 * can hold both a cache AND a runner subscription. On conflict we advance only
 * `status` + `updated_at_ms` (the immutable tenant/plan/customer/created stay
 * as first written) — a redelivery converges harmlessly.
 */
export async function upsertRunnerBilling(
    db: D1DatabaseLike,
    opts: {
        runnerSubscriptionId: string;
        tenantId: string;
        plan: string;
        status: string;
        stripeCustomerId: string | null;
        nowMs: number;
    },
): Promise<void> {
    await db
        .prepare(
            `INSERT INTO runner_billing
               (runner_subscription_id, tenant_id, plan, status,
                stripe_customer_id, created_at_ms, updated_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
             ON CONFLICT (runner_subscription_id) DO UPDATE SET
               status        = excluded.status,
               updated_at_ms = excluded.updated_at_ms`,
        )
        .bind(
            opts.runnerSubscriptionId,
            opts.tenantId,
            opts.plan,
            opts.status,
            opts.stripeCustomerId,
            opts.nowMs,
        )
        .run();
}
/**
 * SEED the tenant's Runners entitlement (`runners_entitlement`, migrations
 * 0070/0072) from the runner subscription. Resolves the tenant through
 * `runner_billing` (subscription id → tenant id) via a correlated SELECT, so it
 * is a no-op if `upsertRunnerBilling` has not yet mapped the subscription (never
 * seeds an entitlement for an unknown subscription). Idempotent ON CONFLICT
 * (tenant_id) — a redelivery re-writes the same caps. `plan` is the fixed
 * 'runners' source label (matching migration 0070's informational `plan`
 * column). max_concurrency is always > 0 for every tier, satisfying the 0070
 * CHECK.
 */
export async function upsertRunnersEntitlementBySubscription(
    db: D1DatabaseLike,
    opts: {
        runnerSubscriptionId: string;
        maxConcurrency: number;
        maxVcpuH: number;
        nowMs: number;
    },
): Promise<void> {
    await db
        .prepare(
            `INSERT INTO runners_entitlement (tenant_id, max_concurrency, plan, created_at_ms, max_vcpu_h)
             SELECT tenant_id, ?1, 'runners', ?2, ?3 FROM runner_billing WHERE runner_subscription_id = ?4
             ON CONFLICT(tenant_id) DO UPDATE SET max_concurrency=excluded.max_concurrency, max_vcpu_h=excluded.max_vcpu_h`,
        )
        .bind(opts.maxConcurrency, opts.nowMs, opts.maxVcpuH, opts.runnerSubscriptionId)
        .run();
}

/**
 * Seed the runner entitlement for a KNOWN tenant id — race-free.
 *
 * WHY (money-path bug fix): the caller pushes both `upsertRunnerBilling` (the
 * `runner_billing` INSERT) and the entitlement seed onto `requiredWrites`, which
 * the handler executes in declaration order. The mapping therefore commits
 * before the entitlement seed and cannot race its lookup.
 * [`upsertRunnersEntitlementBySubscription`] variant (which `SELECT`s the tenant
 * FROM `runner_billing`) can race AHEAD of the `runner_billing` INSERT, read no
 * row, and silently seed 0 rows. A paying runner customer then gets NO capacity
 * (acquire stays 429) — non-deterministically, whoever wins the race. On the
 * `customer.subscription.{created,updated}` path we ALREADY hold the tenant id
 * (from the subscription's `metadata.tenant_id`), so seed DIRECTLY by tenant and
 * drop the dependency on the concurrent `runner_billing` write entirely. The
 * subscription-correlated variant above is retained ONLY for the no-metadata
 * path, where `runner_billing` was mapped by a PRIOR event and already exists.
 */
export async function upsertRunnersEntitlementByTenant(
    db: D1DatabaseLike,
    opts: {
        tenantId: string;
        maxConcurrency: number;
        maxVcpuH: number;
        nowMs: number;
    },
): Promise<void> {
    await db
        .prepare(
            `INSERT INTO runners_entitlement (tenant_id, max_concurrency, plan, created_at_ms, max_vcpu_h)
             VALUES (?1, ?2, 'runners', ?3, ?4)
             ON CONFLICT(tenant_id) DO UPDATE SET max_concurrency=excluded.max_concurrency, max_vcpu_h=excluded.max_vcpu_h`,
        )
        .bind(opts.tenantId, opts.maxConcurrency, opts.nowMs, opts.maxVcpuH)
        .run();
}

/**
 * REVOKE the tenant's Runners entitlement for a cancelled/lapsed subscription.
 * Resolves the tenant through `runner_billing` (subscription id → tenant id) and
 * deletes the `runners_entitlement` row ONLY when the tenant retains no other
 * active/trialing runner subscription (see the inline note — avoids nuking a
 * still-paying tenant). An ABSENT row means "no Runners entitlement" (migration
 * 0070 fail-CLOSED semantics). Idempotent: a redelivery deletes an already-absent
 * row (0 rows affected). Safe no-op if the subscription maps no billing row.
 */
export async function revokeRunnersEntitlementBySubscription(
    db: D1DatabaseLike,
    opts: { runnerSubscriptionId: string },
): Promise<void> {
    // Launch-audit finding (MED): `runners_entitlement` is ONE row per tenant, but
    // `runner_billing` is per-subscription. A blind tenant-keyed DELETE would nuke
    // the whole entitlement even when the tenant still holds ANOTHER active runner
    // subscription — over-revoking a still-paying tenant. So DELETE only when NO
    // OTHER active/trialing runner sub remains for the tenant. The
    // `runner_subscription_id != ?1` self-exclusion means a SINGLE-sub cancel (the
    // common case) always sees an empty "other active" set and DELETEs — revoke
    // stays fail-CLOSED — and it is robust whether or not this sub's own
    // `runner_billing.status` has already advanced to 'canceled'/'past_due'.
    // (Normally prevented upstream by the checkout `AlreadyActive` guard that blocks
    // a 2nd runner purchase; this is the defense-in-depth backstop.)
    await db
        .prepare(
            `DELETE FROM runners_entitlement WHERE tenant_id IN
               (SELECT tenant_id FROM runner_billing WHERE runner_subscription_id = ?1)
             AND tenant_id NOT IN
               (SELECT tenant_id FROM runner_billing
                  WHERE status IN ('active', 'trialing')
                    AND runner_subscription_id != ?1)`,
        )
        .bind(opts.runnerSubscriptionId)
        .run();
}

/**
 * Mark a `runner_billing` row's status (status-only; leaves the tenant/plan/
 * customer/created columns untouched), keyed by subscription id. Used on
 * cancel ('canceled') and terminal payment failure ('past_due') so the runner
 * billing mirror tracks the Stripe subscription state even on events that carry
 * no price. Idempotent bare UPDATE; a no-op if no row maps the subscription id.
 */
export async function markRunnerBillingStatusBySubscription(
    db: D1DatabaseLike,
    opts: { runnerSubscriptionId: string; status: string; nowMs: number },
): Promise<void> {
    await db
        .prepare(
            `UPDATE runner_billing
             SET status = ?1,
                 updated_at_ms = ?2
             WHERE runner_subscription_id = ?3`,
        )
        .bind(opts.status, opts.nowMs, opts.runnerSubscriptionId)
        .run();
}
