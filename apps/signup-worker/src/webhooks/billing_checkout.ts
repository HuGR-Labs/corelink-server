/** Durable, tenant-scoped ownership writes for paid Checkout completions. */

export interface D1RunResult { meta?: { changes?: number } }
export interface D1PreparedStatement {
    bind(...values: unknown[]): D1PreparedStatement;
    run(): Promise<D1RunResult>;
}
export interface D1DatabaseLike { prepare(query: string): D1PreparedStatement }

/** Keep webhook-side recovery shorter than Stripe's 24h idempotency window. */
export const CHECKOUT_LEDGER_RECOVERY_TTL_MS = 15 * 60 * 1000;

/**
 * Rebind a pre-mirror reservation when Stripe delivered a session but the
 * container crashed before writing its mirrors. The tenant+tier metadata and
 * Stripe object creation time are both required: a late session from an older
 * attempt must never attach to a newer checkout for the same tenant.
 */
export async function recoverCheckoutLedger(
    db: D1DatabaseLike,
    opts: { tenantId: string; tier: string; sessionId: string; stripeCustomerId: string;
        checkoutCreatedAtMs: number | null; nowMs: number },
): Promise<boolean> {
    if (opts.checkoutCreatedAtMs === null || !Number.isFinite(opts.checkoutCreatedAtMs)) {
        return false;
    }
    const result = await db.prepare(`
      UPDATE stripe_checkout_ownership_ledger
      SET session_id = ?2,
          stripe_customer_id = CASE WHEN ?3 = '' THEN stripe_customer_id ELSE ?3 END,
          state = 'session_created',
          updated_at_ms = ?4
      WHERE tenant_id = ?1 AND tier = ?5
        AND state IN ('reserved', 'session_created')
        AND (session_id IS NULL OR session_id = ?2)
        AND (?3 = '' OR stripe_customer_id IS NULL OR stripe_customer_id = ?3)
        -- Stripe exposes created in whole seconds; allow the millisecond
        -- ledger write to precede that rounded value by one clock tick.
        AND created_at_ms <= ?6 + 2000
        AND created_at_ms >= ?6 - ?7
        -- A webhook must not resurrect a reservation that has been waiting
        -- longer than the bounded recovery window, even if its Stripe
        -- metadata still happens to match an old session.
        AND ?4 - created_at_ms >= -2000
        AND ?4 - created_at_ms <= ?7
    `).bind(opts.tenantId, opts.sessionId, opts.stripeCustomerId, opts.nowMs,
        opts.tier, opts.checkoutCreatedAtMs, CHECKOUT_LEDGER_RECOVERY_TTL_MS).run();
    const changes = result.meta?.changes;
    if (changes === undefined || changes > 1) {
        throw new Error("checkout ledger recovery did not return D1 changes metadata");
    }
    return changes === 1;
}

/**
 * Materialize a paid checkout only when the session is this tenant's durable
 * pending checkout, or when Stripe is replaying the exact already-owned
 * subscription.  The guarded UPDATE is intentional: tenant_id alone is not
 * an ownership proof and must never replace a live paid subscription.
 */
export async function upsertBillingPaid(
    db: D1DatabaseLike,
    opts: { tenantId: string; stripeCustomerId: string; stripeSubscriptionId: string | null;
        sessionId: string; plan: string | null; currentPeriodEndMs: number | null; nowMs: number;
        checkoutCreatedAtMs?: number | null },
): Promise<void> {
    if (!opts.stripeSubscriptionId) throw new Error("paid checkout has no subscription id");
    await recoverCheckoutLedger(db, {
        tenantId: opts.tenantId,
        tier: opts.plan ?? "",
        sessionId: opts.sessionId,
        stripeCustomerId: opts.stripeCustomerId,
        checkoutCreatedAtMs: opts.checkoutCreatedAtMs ?? null,
        nowMs: opts.nowMs,
    });
    const result = await db.prepare(`
      INSERT INTO tenant_billing
        (tenant_id, stripe_customer_id, stripe_subscription_id, status, plan,
         current_period_end_ms, schema_version, created_at_ms, updated_at_ms)
      SELECT ?1, ?2, ?3, 'paid', ?4, ?5, 1, ?6, ?6
      WHERE EXISTS (
        SELECT 1 FROM stripe_checkout_sessions s
        JOIN tier_selections t ON t.tenant_id = s.tenant_id
          AND t.correlation_id = s.correlation_id
        WHERE s.session_id = ?7 AND s.tenant_id = ?1 AND s.tier = ?4
          AND t.subscription_state = 'pending_checkout'
          AND t.stripe_customer_id = ?2
      ) OR EXISTS (
        SELECT 1 FROM stripe_checkout_ownership_ledger l
        WHERE l.tenant_id = ?1 AND l.tier = ?4 AND l.session_id = ?7
          AND l.stripe_customer_id = ?2 AND l.state = 'session_created'
      ) OR EXISTS (
        SELECT 1 FROM tenant_billing owned
        WHERE owned.tenant_id = ?1 AND owned.stripe_customer_id = ?2
          AND owned.stripe_subscription_id = ?3 AND owned.status = 'paid'
      )
      ON CONFLICT (tenant_id) DO UPDATE SET
        stripe_customer_id = excluded.stripe_customer_id,
        stripe_subscription_id = CASE
          WHEN tenant_billing.status != 'paid' THEN excluded.stripe_subscription_id
          ELSE tenant_billing.stripe_subscription_id END,
        status = 'paid', plan = excluded.plan,
        current_period_end_ms = excluded.current_period_end_ms,
        updated_at_ms = excluded.updated_at_ms
      WHERE (tenant_billing.stripe_customer_id IS NULL
             OR tenant_billing.stripe_customer_id = excluded.stripe_customer_id)
        AND (tenant_billing.status != 'paid'
             OR tenant_billing.stripe_subscription_id IS excluded.stripe_subscription_id)
    `).bind(opts.tenantId, opts.stripeCustomerId, opts.stripeSubscriptionId, opts.plan,
        opts.currentPeriodEndMs, opts.nowMs, opts.sessionId).run();
    if (result.meta?.changes !== 1) throw new Error("Stripe subscription ownership conflict");
}

/** Activate only while the same Stripe subscription owns the billing row. */
export async function activatePaidTierSelection(
    db: D1DatabaseLike,
    opts: { tenantId: string; tier: string; stripeCustomerId: string;
        stripeSubscriptionId: string; sessionId: string; nowMs: number; correlationId: string },
): Promise<void> {
    const result = await db.prepare(`
      INSERT INTO tier_selections
        (tenant_id, tier, subscription_state, stripe_customer_id,
         subscription_started_at_ms, schema_version, correlation_id)
      SELECT ?1, ?2, 'active', ?3, ?4, 1, ?5
      WHERE EXISTS (SELECT 1 FROM tenant_billing
        WHERE tenant_id = ?1 AND stripe_customer_id = ?3
          AND stripe_subscription_id = ?6 AND status = 'paid')
      ON CONFLICT (tenant_id) DO UPDATE SET
        tier = excluded.tier, subscription_state = 'active',
        stripe_customer_id = excluded.stripe_customer_id,
        subscription_started_at_ms = excluded.subscription_started_at_ms
      WHERE (tier_selections.subscription_state <> 'active'
             OR tier_selections.tier = 'free')
        AND EXISTS (SELECT 1 FROM tenant_billing
          WHERE tenant_id = ?1 AND stripe_customer_id = ?3
            AND stripe_subscription_id = ?6 AND status = 'paid')
    `).bind(opts.tenantId, opts.tier, opts.stripeCustomerId, opts.nowMs,
        opts.correlationId, opts.stripeSubscriptionId).run();
    const changes = result.meta?.changes;
    if (changes !== 0 && changes !== 1) {
        throw new Error("tier activation write did not return D1 changes metadata");
    }
    await db.prepare(`
      UPDATE stripe_checkout_ownership_ledger
      SET state = 'completed', updated_at_ms = ?2
      WHERE tenant_id = ?1 AND session_id = ?3 AND state = 'session_created'
    `).bind(opts.tenantId, opts.nowMs, opts.sessionId).run();
}
