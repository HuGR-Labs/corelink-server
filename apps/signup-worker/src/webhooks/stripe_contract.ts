/** Pure Stripe metadata, tier, and entitlement decoding. */
import type { StripeWebhookEnv } from "./stripe.js";

export function tenantIdFromMetadata(obj: Record<string, unknown>): string | null {
    const meta = obj["metadata"] as Record<string, unknown> | undefined;
    const raw = meta?.["tenant_id"];
    return typeof raw === "string" && raw.length > 0 ? raw : null;
}

/** Stripe Checkout Session creation time, in the same units as the ledger. */
export function checkoutCreatedAtMs(obj: Record<string, unknown>): number | null {
    const created = obj["created"];
    return typeof created === "number" && Number.isFinite(created) && created > 0
        ? created * 1000
        : null;
}

/** Stripe subscription creation time, retained as a provider ordering fact. */
export function subscriptionCreatedAtMs(obj: Record<string, unknown>): number | null {
    const created = obj["created"];
    return typeof created === "number" && Number.isFinite(created) && created > 0
        ? created * 1000
        : null;
}

/** Extract clerk_user_id from Stripe metadata. Returns null if absent. */
export function clerkUserIdFromMetadata(obj: Record<string, unknown>): string | null {
    const meta = obj["metadata"] as Record<string, unknown> | undefined;
    const raw = meta?.["clerk_user_id"];
    return typeof raw === "string" && raw.length > 0 ? raw : null;
}

/** Cents → USD float. Stripe amounts are integer cents. */
export function centsToUsd(cents: unknown): number {
    return typeof cents === "number" ? cents / 100 : 0;
}

/** The canonical paid tiers the tier_selections FSM may be activated on. */
export type PaidTier = "solo" | "starter" | "team" | "pro" | "max";

/** Coerce an arbitrary string to a known paid tier, or null. */
export function asPaidTier(raw: unknown): PaidTier | null {
    return raw === "solo" ||
        raw === "starter" ||
        raw === "team" ||
        raw === "pro" ||
        raw === "max"
        ? raw
        : null;
}

/** Read a paid tier straight from Stripe `metadata[tier]` (checkout sets it). */
export function tierFromMetadata(obj: Record<string, unknown>): PaidTier | null {
    const meta = obj["metadata"] as Record<string, unknown> | undefined;
    return asPaidTier(meta?.["tier"]);
}

/**
 * Reverse-map the Stripe price id on a subscription object back to our tier,
 * using the same `STRIPE_PRICE_ID_{TIER}` env vars the checkout backend uses to
 * pick the price. The subscription object carries the active price under
 * `items.data[0].price.id` (and, defensively, `plan.id` on older API shapes).
 *
 * Returns null if the price matches no configured tier — callers MUST treat a
 * null as "unknown, do not change the tier" (fail-safe; never guess).
 */
export function tierFromSubscriptionPrice(
    obj: Record<string, unknown>,
    env: StripeWebhookEnv,
): PaidTier | null {
    const items = obj["items"] as { data?: Array<Record<string, unknown>> } | undefined;
    const first = items?.data?.[0];
    const priceObj = first?.["price"] as Record<string, unknown> | undefined;
    const planObj = obj["plan"] as Record<string, unknown> | undefined;
    const priceId =
        (typeof priceObj?.["id"] === "string" ? (priceObj["id"] as string) : null) ??
        (typeof planObj?.["id"] === "string" ? (planObj["id"] as string) : null);
    if (!priceId) return null;

    const map: Array<[string | undefined, PaidTier]> = [
        [env.STRIPE_PRICE_ID_SOLO, "solo"],
        [env.STRIPE_PRICE_ID_STARTER, "starter"],
        [env.STRIPE_PRICE_ID_TEAM, "team"],
        [env.STRIPE_PRICE_ID_PRO, "pro"],
        [env.STRIPE_PRICE_ID_MAX, "max"],
    ];
    for (const [configured, tier] of map) {
        if (configured && configured === priceId) return tier;
    }
    return null;
}

/**
 * Resolve the tier of a subscription event: prefer the explicit
 * `metadata[tier]` (rare on subscription objects — only present if the checkout
 * copied it into subscription_data), then fall back to the price→tier map.
 */
export function resolveSubscriptionTier(
    obj: Record<string, unknown>,
    env: StripeWebhookEnv,
): PaidTier | null {
    return tierFromMetadata(obj) ?? tierFromSubscriptionPrice(obj, env);
}

/**
 * Defense-in-depth price↔tier cross-check (audit fix 3).
 *
 * `metadata[tier]` is server-set by our checkout backend (client.rs:631), but a
 * checkout-backend bug could desync it from the Stripe price actually
 * subscribed (the price is selected from `STRIPE_PRICE_ID_{TIER}` by the same
 * tier — if that wiring ever breaks, the customer pays one SKU and gets
 * entitled to another). Subscription objects are the first webhook payloads
 * that carry BOTH signals (`metadata[tier]` when the checkout copied it +
 * `items.data[0].price.id` / `plan.id` always), so we cross-check them there.
 *
 * Returns the conflicting pair when both resolve AND disagree, else null.
 * Callers MUST fail loud (500 → Stripe redelivery, operator signal) on a
 * conflict — never pick a winner and keep writing entitlement.
 *
 * NOTE: `checkout.session.completed` cannot run this check — the webhook
 * session payload carries NO price-shaped data (`line_items` is only present
 * on an API retrieval with `expand[]`, never in event payloads, and the
 * session object has no `items`/`plan`), and we deliberately make no Stripe
 * API calls from the webhook. The cross-check therefore lives on the
 * customer.subscription.created/updated events Stripe fires immediately after
 * checkout, where the price IS in the payload.
 */
export function detectTierPriceMismatch(
    obj: Record<string, unknown>,
    env: StripeWebhookEnv,
): { metaTier: PaidTier; priceTier: PaidTier } | null {
    const metaTier = tierFromMetadata(obj);
    const priceTier = tierFromSubscriptionPrice(obj, env);
    return metaTier && priceTier && metaTier !== priceTier
        ? { metaTier, priceTier }
        : null;
}

/**
 * Whether a Stripe subscription `status` means the tenant still has entitlement.
 * `active` and `trialing` keep access; everything else (past_due, unpaid,
 * canceled, incomplete, incomplete_expired, paused) loses it. Fail-safe: an
 * unknown/absent status is treated as NOT entitled.
 */
export function subscriptionStatusGrantsAccess(rawStatus: unknown): boolean {
    return rawStatus === "active" || rawStatus === "trialing";
}

// ---------------------------------------------------------------------------
// Runner subscription helpers (self-serve Runners entitlement)
//
// The runner subscription is a SEPARATE Stripe subscription from the cache
// subscription. Its lifecycle seeds/revokes `runners_entitlement` (migrations
// 0070/0072) via the dedicated `runner_billing` map (migration 0087), NOT
// tenant_billing (which is one-row-per-tenant and reserved for the cache sub).
// ---------------------------------------------------------------------------

/** The canonical runner tiers a runner purchase may be recorded on. */
export type RunnerTier =
    | "runner_starter"
    | "runner_pro"
    | "runner_team"
    | "runner_scale"
    | "runner_max";

/** The per-tier Runners entitlement (max_concurrency, max_vcpu_h). */
export interface RunnerEntitlement {
    maxConcurrency: number;
    maxVcpuH: number;
}

/**
 * The frozen runner tier → entitlement ladder (max_concurrency, max_vcpu_h).
 * Mirrors the operator-provisioned axes on `runners_entitlement`:
 *   runner_starter 20/100 · runner_pro 40/240 · runner_team 80/600 ·
 *   runner_scale 160/1200 · runner_max 320/2400.
 * All max_concurrency values are > 0, satisfying the 0070 CHECK.
 */
export const RUNNER_TIER_ENTITLEMENT: Record<RunnerTier, RunnerEntitlement> = {
    runner_starter: { maxConcurrency: 20, maxVcpuH: 100 },
    runner_pro: { maxConcurrency: 40, maxVcpuH: 240 },
    runner_team: { maxConcurrency: 80, maxVcpuH: 600 },
    runner_scale: { maxConcurrency: 160, maxVcpuH: 1200 },
    runner_max: { maxConcurrency: 320, maxVcpuH: 2400 },
};

/** Coerce an arbitrary string to a known runner tier, or null (fail-safe). */
function asRunnerTier(raw: unknown): RunnerTier | null {
    return raw === "runner_starter" ||
        raw === "runner_pro" ||
        raw === "runner_team" ||
        raw === "runner_scale" ||
        raw === "runner_max"
        ? raw
        : null;
}

/**
 * Read a runner tier straight from Stripe `metadata[tier]`. Used on
 * `checkout.session.completed` / `async_payment_succeeded`, which carry NO
 * price data in the webhook payload — the checkout backend stamps the runner
 * tier into metadata[tier] exactly as it does for the cache tiers. Returns null
 * for a cache tier / unknown value, so a CACHE checkout is never misclassified
 * as a runner purchase (the cache and runner arms are mutually exclusive on the
 * metadata[tier] value: asPaidTier vs asRunnerTier are disjoint sets).
 */
export function runnerTierFromMetadata(obj: Record<string, unknown>): RunnerTier | null {
    const meta = obj["metadata"] as Record<string, unknown> | undefined;
    return asRunnerTier(meta?.["tier"]);
}

/**
 * Reverse-map a subscription's Stripe price id(s) → runner entitlement, using
 * the `STRIPE_PRICE_ID_RUNNER_{TIER}` env vars. Scans ALL `items.data[*].price.id`
 * (multi-item subscriptions) plus the legacy top-level `plan.id`, and returns
 * the FIRST recognised runner price's entitlement, else null.
 *
 * A null return means "this subscription is not a runner subscription (by
 * price)" — the caller then leaves runner entitlement untouched. Because a
 * runner price is in a DISJOINT env set from the cache price map, the cache
 * path's tierFromSubscriptionPrice returns null for a runner price (and
 * vice-versa), so the two paths never both fire on one subscription.
 */
export function runnerEntitlementFromSubscriptionPrice(
    obj: Record<string, unknown>,
    env: StripeWebhookEnv,
): RunnerEntitlement | null {
    const map: Array<[string | undefined, RunnerTier]> = [
        [env.STRIPE_PRICE_ID_RUNNER_STARTER, "runner_starter"],
        [env.STRIPE_PRICE_ID_RUNNER_PRO, "runner_pro"],
        [env.STRIPE_PRICE_ID_RUNNER_TEAM, "runner_team"],
        [env.STRIPE_PRICE_ID_RUNNER_SCALE, "runner_scale"],
        [env.STRIPE_PRICE_ID_RUNNER_MAX, "runner_max"],
    ];

    // Collect every price id the subscription carries: each item's price.id
    // (modern shape) plus the legacy top-level plan.id (older API shape).
    const priceIds: string[] = [];
    const items = obj["items"] as { data?: Array<Record<string, unknown>> } | undefined;
    for (const item of items?.data ?? []) {
        const priceObj = item?.["price"] as Record<string, unknown> | undefined;
        if (typeof priceObj?.["id"] === "string") priceIds.push(priceObj["id"] as string);
    }
    const planObj = obj["plan"] as Record<string, unknown> | undefined;
    if (typeof planObj?.["id"] === "string") priceIds.push(planObj["id"] as string);

    for (const priceId of priceIds) {
        for (const [configured, tier] of map) {
            if (configured && configured === priceId) return RUNNER_TIER_ENTITLEMENT[tier];
        }
    }
    return null;
}

// ---------------------------------------------------------------------------
// Idempotency claim (#34/#36): dedup on Stripe event.id
// ---------------------------------------------------------------------------

/**
 * Event types this handler actually dispatches side effects for.
 *
 * Built from the single-source-of-truth JSON (handled-stripe-events.json) so the
 * runtime allowlist and the live Stripe endpoint's `enabled_events` stay in lockstep
 * (the reconcile script reads the SAME JSON). Never inline a literal here — a literal
 * that drifts from the dashboard is exactly the class of bug this indirection kills.
 */
