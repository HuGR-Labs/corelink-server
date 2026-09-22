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
 * Apply a Runners entitlement through the shared durable fence.
 *
 * The provider tuple is `(subscription.created, subscription.id,
 * event.created, event.id)`. Every coordinate is supplied by Stripe; the
 * subscription id resolves the documented same-second replacement ambiguity.
 * D1 `batch` is a transaction, so the fence advancement and guarded mutation
 * cannot interleave with the materializer's identical CAS.
 */
export async function reconcileRunnersEntitlement(
    db: D1DatabaseLike,
    opts: {
        tenantId: string | null;
        runnerSubscriptionId: string;
        subscriptionCreatedAtMs: number | null;
        stripeEventCreatedAtMs: number | null;
        stripeEventId: string;
        entitlement: { maxConcurrency: number; maxVcpuH: number } | null;
        nowMs: number;
    },
): Promise<void> {
    if (!db.batch) throw new Error("D1 batch support is required for runner entitlement CAS");
    if (!opts.runnerSubscriptionId || !opts.stripeEventId ||
        !Number.isFinite(opts.subscriptionCreatedAtMs) || (opts.subscriptionCreatedAtMs ?? 0) <= 0 ||
        !Number.isFinite(opts.stripeEventCreatedAtMs) || (opts.stripeEventCreatedAtMs ?? 0) <= 0) {
        throw new Error("runner entitlement CAS requires complete Stripe ordering facts");
    }
    if (opts.entitlement && opts.entitlement.maxConcurrency <= 0) {
        throw new Error("runner entitlement CAS max concurrency must be positive");
    }
    const tenantSource = opts.tenantId
        ? { sql: "VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)", binds: [opts.tenantId] }
        : { sql: "SELECT tenant_id, ?1, ?2, ?3, ?4, ?5, ?6 FROM runner_billing WHERE runner_subscription_id = ?1", binds: [] as unknown[] };
    const fence = db.prepare(
        `INSERT INTO runner_entitlement_reconcile_fence
           (tenant_id, stripe_subscription_id, subscription_created_at_ms,
            stripe_event_created_at_ms, stripe_event_id, is_granting, applied_at_ms)
         ${tenantSource.sql}
         ON CONFLICT(tenant_id) DO UPDATE SET
           stripe_subscription_id=excluded.stripe_subscription_id,
           subscription_created_at_ms=excluded.subscription_created_at_ms,
           stripe_event_created_at_ms=excluded.stripe_event_created_at_ms,
           stripe_event_id=excluded.stripe_event_id,
           is_granting=excluded.is_granting, applied_at_ms=excluded.applied_at_ms
         WHERE excluded.subscription_created_at_ms > runner_entitlement_reconcile_fence.subscription_created_at_ms
            OR (excluded.subscription_created_at_ms = runner_entitlement_reconcile_fence.subscription_created_at_ms
                AND excluded.stripe_subscription_id > runner_entitlement_reconcile_fence.stripe_subscription_id)
            OR (excluded.subscription_created_at_ms = runner_entitlement_reconcile_fence.subscription_created_at_ms
                AND excluded.stripe_subscription_id = runner_entitlement_reconcile_fence.stripe_subscription_id
                AND excluded.stripe_event_created_at_ms > runner_entitlement_reconcile_fence.stripe_event_created_at_ms)
            OR (excluded.subscription_created_at_ms = runner_entitlement_reconcile_fence.subscription_created_at_ms
                AND excluded.stripe_subscription_id = runner_entitlement_reconcile_fence.stripe_subscription_id
                AND excluded.stripe_event_created_at_ms = runner_entitlement_reconcile_fence.stripe_event_created_at_ms
                AND excluded.stripe_event_id > runner_entitlement_reconcile_fence.stripe_event_id)
         RETURNING stripe_subscription_id`,
    ).bind(
        ...(tenantSource.binds), opts.runnerSubscriptionId, opts.subscriptionCreatedAtMs,
        opts.stripeEventCreatedAtMs, opts.stripeEventId, opts.entitlement ? 1 : 0, opts.nowMs,
    );
    const mutation = opts.entitlement
        ? opts.tenantId
            ? db.prepare(
                `INSERT INTO runners_entitlement (tenant_id, max_concurrency, plan, created_at_ms, max_vcpu_h)
                 SELECT ?1, ?2, 'runners', ?3, ?4
                 WHERE EXISTS (SELECT 1 FROM runner_entitlement_reconcile_fence
                   WHERE tenant_id = ?5 AND stripe_subscription_id = ?6
                     AND subscription_created_at_ms = ?7 AND stripe_event_created_at_ms = ?8
                     AND stripe_event_id = ?9)
                 ON CONFLICT(tenant_id) DO UPDATE SET max_concurrency=excluded.max_concurrency, max_vcpu_h=excluded.max_vcpu_h`,
              ).bind(opts.tenantId, opts.entitlement.maxConcurrency, opts.nowMs,
                opts.entitlement.maxVcpuH, opts.tenantId, opts.runnerSubscriptionId,
                opts.subscriptionCreatedAtMs, opts.stripeEventCreatedAtMs, opts.stripeEventId)
            : db.prepare(
                `INSERT INTO runners_entitlement (tenant_id, max_concurrency, plan, created_at_ms, max_vcpu_h)
                 SELECT rb.tenant_id, ?1, 'runners', ?2, ?3 FROM runner_billing rb
                 WHERE rb.runner_subscription_id = ?4 AND EXISTS
                   (SELECT 1 FROM runner_entitlement_reconcile_fence WHERE tenant_id = rb.tenant_id
                     AND stripe_subscription_id = ?4 AND subscription_created_at_ms = ?5
                     AND stripe_event_created_at_ms = ?6 AND stripe_event_id = ?7)
                 ON CONFLICT(tenant_id) DO UPDATE SET max_concurrency=excluded.max_concurrency, max_vcpu_h=excluded.max_vcpu_h`,
              ).bind(opts.entitlement.maxConcurrency, opts.nowMs, opts.entitlement.maxVcpuH,
                opts.runnerSubscriptionId, opts.subscriptionCreatedAtMs,
                opts.stripeEventCreatedAtMs, opts.stripeEventId)
        : opts.tenantId
            ? db.prepare(
                `DELETE FROM runners_entitlement WHERE tenant_id = ?1 AND EXISTS
                   (SELECT 1 FROM runner_entitlement_reconcile_fence WHERE tenant_id = ?1
                     AND stripe_subscription_id = ?2 AND subscription_created_at_ms = ?3
                     AND stripe_event_created_at_ms = ?4 AND stripe_event_id = ?5)`,
              ).bind(opts.tenantId, opts.runnerSubscriptionId, opts.subscriptionCreatedAtMs,
                opts.stripeEventCreatedAtMs, opts.stripeEventId)
            : db.prepare(
                `DELETE FROM runners_entitlement WHERE tenant_id =
                   (SELECT tenant_id FROM runner_billing WHERE runner_subscription_id = ?1)
                   AND EXISTS (SELECT 1 FROM runner_entitlement_reconcile_fence WHERE
                     tenant_id = (SELECT tenant_id FROM runner_billing WHERE runner_subscription_id = ?1)
                     AND stripe_subscription_id = ?1 AND subscription_created_at_ms = ?2
                     AND stripe_event_created_at_ms = ?3 AND stripe_event_id = ?4)`,
              ).bind(opts.runnerSubscriptionId, opts.subscriptionCreatedAtMs,
                opts.stripeEventCreatedAtMs, opts.stripeEventId);
    const readFence = db.prepare(
        `SELECT stripe_subscription_id, subscription_created_at_ms, stripe_event_created_at_ms, stripe_event_id
         FROM runner_entitlement_reconcile_fence WHERE tenant_id = ${opts.tenantId ? "?1" : "(SELECT tenant_id FROM runner_billing WHERE runner_subscription_id = ?1)"}`,
    ).bind(opts.tenantId ?? opts.runnerSubscriptionId);
    const results = await db.batch([fence, mutation, readFence]);
    if (results[0]?.results?.length) return;
    const current = results[2]?.results?.[0];
    if (current?.["stripe_subscription_id"] === opts.runnerSubscriptionId &&
        current["subscription_created_at_ms"] === opts.subscriptionCreatedAtMs &&
        current["stripe_event_created_at_ms"] === opts.stripeEventCreatedAtMs &&
        current["stripe_event_id"] === opts.stripeEventId) return;
    throw new Error("stale runner entitlement authority rejected");
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
