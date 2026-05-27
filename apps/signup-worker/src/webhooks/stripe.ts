// Stripe webhook handler.
//
// Phase 0.G scope: fire `checkout_started`, `paid_subscription_started`,
// `plan_downgraded`, `subscription_canceled` analytics events per PLG §7.1.
// Real Stripe-signature verification + tenant.plan mutation lands in
// Phase 0.C (BILLINGSTEP-DELETE-WIRE-CHECKOUT).

import type { AnalyticsEmitEnv } from "../lib/analytics-server";
import { emit, newEventId } from "../lib/analytics-server";

export interface StripeWebhookEnv extends AnalyticsEmitEnv {
    STRIPE_WEBHOOK_SECRET?: string;
}

interface StripeEvent {
    type: string;
    data: { object: Record<string, unknown> };
}

/** Pull tenant_id out of Stripe metadata. Returns null if absent. */
function tenantIdFromMetadata(obj: Record<string, unknown>): string | null {
    const meta = obj["metadata"] as Record<string, unknown> | undefined;
    const raw = meta?.["tenant_id"];
    return typeof raw === "string" && raw.length > 0 ? raw : null;
}

/** Cents → USD float. Stripe amounts are integer cents. */
function centsToUsd(cents: unknown): number {
    return typeof cents === "number" ? cents / 100 : 0;
}

export async function handleStripeWebhook(
    request: Request,
    env: StripeWebhookEnv,
    ctx: ExecutionContext,
): Promise<Response> {
    if (!env.STRIPE_WEBHOOK_SECRET) {
        return new Response("stripe webhook secret not configured", { status: 503 });
    }

    let event: StripeEvent;
    try {
        event = (await request.json()) as StripeEvent;
    } catch {
        return new Response("invalid_json", { status: 400 });
    }

    const obj = event.data.object;
    const tenantId = tenantIdFromMetadata(obj);

    switch (event.type) {
        case "checkout.session.created": {
            const targetPlan = (obj["metadata"] as Record<string, unknown> | undefined)?.["target_plan"];
            ctx.waitUntil(emit(env, {
                id: newEventId(),
                event_name: "checkout_started",
                tenant_id: tenantId,
                properties: { target_plan: typeof targetPlan === "string" ? targetPlan : "pro" },
            }));
            break;
        }
        case "checkout.session.completed":
        case "customer.subscription.created": {
            const planMeta = (obj["metadata"] as Record<string, unknown> | undefined)?.["plan"];
            const plan = typeof planMeta === "string" ? planMeta : "pro";
            // `amount_total` from checkout.session, `items.data[0].price.unit_amount`
            // from subscription. We accept whichever the event has; both are cents.
            const amountCents = (obj["amount_total"] as number | undefined)
                ?? (((obj["items"] as { data?: Array<{ price?: { unit_amount?: number } }> } | undefined)?.data?.[0]?.price?.unit_amount) ?? 0);
            ctx.waitUntil(emit(env, {
                id: newEventId(),
                event_name: "paid_subscription_started",
                tenant_id: tenantId,
                properties: { plan, mrr_usd: centsToUsd(amountCents) },
            }));
            break;
        }
        case "customer.subscription.deleted": {
            ctx.waitUntil(emit(env, {
                id: newEventId(),
                event_name: "subscription_canceled",
                tenant_id: tenantId,
                properties: {
                    from_plan: "pro",
                    to_plan: "free",
                    mrr_delta_usd: -centsToUsd(
                        ((obj["items"] as { data?: Array<{ price?: { unit_amount?: number } }> } | undefined)?.data?.[0]?.price?.unit_amount) ?? 0,
                    ),
                },
            }));
            break;
        }
        case "customer.subscription.updated": {
            // Phase 0.C will refine this to distinguish upgrade / downgrade by
            // comparing prior_attributes.price.unit_amount. For now classify as
            // a downgrade only if the new amount is strictly less than the
            // previous_attributes amount.
            const prior = (event.data as { previous_attributes?: Record<string, unknown> }).previous_attributes;
            const oldAmount = (prior?.["items"] as { data?: Array<{ price?: { unit_amount?: number } }> } | undefined)?.data?.[0]?.price?.unit_amount;
            const newAmount = ((obj["items"] as { data?: Array<{ price?: { unit_amount?: number } }> } | undefined)?.data?.[0]?.price?.unit_amount) ?? 0;
            if (typeof oldAmount === "number" && newAmount < oldAmount) {
                ctx.waitUntil(emit(env, {
                    id: newEventId(),
                    event_name: "plan_downgraded",
                    tenant_id: tenantId,
                    properties: {
                        from_plan: "pro",
                        to_plan: "free",
                        mrr_delta_usd: centsToUsd(newAmount - oldAmount),
                    },
                }));
            }
            break;
        }
        default:
            // Unhandled type — fall through. We acknowledge so Stripe doesn't
            // retry; Phase 0.C wires the rest.
            break;
    }

    return new Response("ok", { status: 200 });
}
