/** Shared checkout activation writer and analytics event builder. */
import type { StripeWebhookEnv } from "./stripe.js";
import { emit, newEventId } from "../lib/analytics-server";
import { asPaidTier, centsToUsd } from "./stripe_contract.js";
import { activatePaidTierSelection, upsertBillingPaid } from "./stripe_persistence_billing.js";

/**
 * Queue the checkout activation (audit fix 1 extraction): the tenant_billing
 * 'paid' upsert + the canonical tier_selections activation, and build the
 * `paid_subscription_started` analytics event the caller emits exactly-once.
 *
 * SHARED by the two paths that may grant entitlement for a checkout session —
 * `checkout.session.completed` with payment_status paid/no_payment_required,
 * and `checkout.session.async_payment_succeeded` (delayed payment methods) —
 * so the activation semantics CANNOT drift between them.
 *
 * The inner `env.BILLING_DB` guard is TypeScript narrowing only: the handler
 * fails closed (503) before dispatch when the binding is missing.
 */
export function queueCheckoutActivation(
    env: StripeWebhookEnv,
    requiredWrites: Array<() => Promise<void>>,
    opts: {
        sessionId: string | undefined;
        tenantId: string;
        stripeCustomerId: string;
        stripeSubscriptionId: string | null;
        plan: string;
        amountTotal: number | undefined;
        nowMs: number;
        checkoutCreatedAtMs: number | null;
    },
): Parameters<typeof emit>[1] {
    if (env.BILLING_DB) {
        const db = env.BILLING_DB;
        // REQUIRED durable write — awaited by the caller; a failure returns
        // 500 so Stripe redelivers (the customer paid, they are owed the
        // billing row).
        requiredWrites.push(async () => {
            await upsertBillingPaid(db, {
                tenantId: opts.tenantId,
                stripeCustomerId: opts.stripeCustomerId,
                stripeSubscriptionId: opts.stripeSubscriptionId,
                sessionId: opts.sessionId ?? "",
                plan: opts.plan,
                currentPeriodEndMs: null,
                nowMs: opts.nowMs,
                checkoutCreatedAtMs: opts.checkoutCreatedAtMs,
            });
        // GAP-6: reconcile the canonical subscription FSM the money path
        // reads. Only a real paid tier may flip to 'active' (free never
        // reaches Stripe; enterprise uses the inquiry form). An unrecognised
        // tier is left un-activated rather than written with a bogus value.
        // Uses the canonical asPaidTier set — an inline starter/team/pro
        // triple here silently skipped Solo/Max activation
        // (pay-but-not-entitled for the $15/$149 SKUs).
        const paidTier = asPaidTier(opts.plan);
            if (paidTier) {
                await activatePaidTierSelection(db, {
                    tenantId: opts.tenantId,
                    tier: paidTier,
                    stripeCustomerId: opts.stripeCustomerId,
                    stripeSubscriptionId: opts.stripeSubscriptionId ?? "",
                    sessionId: opts.sessionId ?? "",
                    nowMs: opts.nowMs,
                    correlationId: `stripe_checkout:${opts.sessionId ?? opts.stripeCustomerId}`,
                });
            }
        });
    }
    // paid_subscription_started carries non-idempotent MRR; emitted exactly
    // once on the first SUCCESSFUL delivery (gated on the post-write claim).
    return {
        id: newEventId(),
        event_name: "paid_subscription_started",
        tenant_id: opts.tenantId,
        properties: {
            plan: opts.plan,
            mrr_usd: centsToUsd(opts.amountTotal),
            stripe_customer_id: opts.stripeCustomerId,
            stripe_subscription_id: opts.stripeSubscriptionId,
        },
    };
}
