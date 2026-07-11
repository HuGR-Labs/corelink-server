// Stripe webhook handler — self-serve Checkout subscription lifecycle.
//
// Stream 2.10 (BILLINGSTEP-WIRE-CHECKOUT): verifies Stripe webhook signature
// then mutates the `tenant_billing` D1 table on the three lifecycle events that
// matter for self-serve billing:
//
//   checkout.session.completed    → INSERT / UPDATE tenant_billing (paid) +
//                                   activate tier_selections (access gate) —
//                                   ONLY when payment_status is
//                                   paid/no_payment_required (async payment
//                                   methods deliver 'unpaid' here; entitlement
//                                   then arrives via async_payment_succeeded)
//   checkout.session.async_payment_succeeded
//                                 → same activation as a paid completed session
//                                   (delayed payment methods: SEPA, ACH, …)
//   checkout.session.async_payment_failed
//                                 → log + ack; nothing was granted at
//                                   completed-time (payment_status was
//                                   'unpaid'), so there is nothing to revoke
//   customer.subscription.created → backfill tenant_billing.current_period_end_ms
//   customer.subscription.updated → UPDATE period_end + status; propagate a plan
//                                   change to tier_selections.tier; sync the
//                                   access gate (deactivate if status no longer
//                                   grants access)
//   customer.subscription.deleted → tenant_billing.status='canceled' AND
//                                   tier_selections.subscription_state='inactive'
//                                   (lose access + allow re-subscribe)
//   invoice.payment_failed        → on TERMINAL dunning failure: tenant_billing
//                                   status='past_due' AND tier_selections away
//                                   from 'active' (revoke entitlement)
//
// CANONICAL access gate: `tier_selections.subscription_state = 'active'` (read by
// the container's `has_active_subscription`, tier_select_store.rs). tenant_billing
// is the secondary mirror. Every handler that changes entitlement updates the
// canonical gate, not just tenant_billing.
//
// Each arm also emits an analytics event to the analytics-worker.
//
// DURABILITY + IDEMPOTENCY (Stripe redelivers; #37, process-then-claim
// re-architecture of #34/#36 option (b)):
//
// The entitlement/billing D1 writes are the MONEY PATH — a customer paid and is
// owed access. They used to be dispatched FIRE-AND-FORGET (ctx.waitUntil) and
// the handler returned 200 regardless, so a D1 error/throttle silently lost the
// write: Stripe saw its 200, never redelivered, and the customer had no access
// and no recovery. We now make the writes DURABLE by AWAITING them and returning
// a non-2xx (500) on failure so Stripe redelivers. Every write is an idempotent
// ON CONFLICT upsert / guarded UPDATE, so a redelivery re-runs them harmlessly.
//
// The ONLY non-idempotent side effects are the analytics emits (e.g.
// `paid_subscription_started` MRR), which must fire EXACTLY ONCE — on the first
// SUCCESSFUL delivery. We therefore moved the durable dedup claim from BEFORE
// processing to AFTER the writes succeed (PROCESS-THEN-CLAIM): claiming first
// was wrong once writes are awaited + retried, because a first delivery whose
// write FAILED would already have claimed the event, so Stripe's retry would see
// the claim as a duplicate and SKIP the emit → MRR undercount. By claiming only
// after the writes commit, the claim row exists iff a delivery has SUCCEEDED, so:
//   - first SUCCESSFUL delivery   → claim newly inserted (changes=1) → emit.
//   - retry after a prior SUCCESS → claim is a PK conflict (changes=0) → SKIP
//                                   emit (writes re-run harmlessly, still 200).
//   - retry after a prior FAILURE → no claim was ever made (the failed attempt
//                                   returned 500 before claiming) → completes
//                                   the writes + claims + emits, exactly once.
//
// Ordering: verify sig → parse → AWAIT writes (500 on any failure) → claim →
// emit only if first claim. The emit is best-effort (analytics, non-money-path):
// an emit failure is caught + logged and does NOT fail the webhook — writes
// succeeding + 200 is the success contract. See `claimWebhookEvent`.
//
// Security invariants (audited):
//   1. Stripe-Timestamp MUST be within 5 minutes of now (replay-attack window).
//   2. Signature verified via HMAC-SHA256 + constant-time compare BEFORE
//      any side effect (no partial processing on bad sig).
//   3. No card data is ever logged — Stripe never sends it in webhooks, but
//      we are defensive: no `payload_json` logging in error paths.
//   4. Idempotency: tenant_billing uses INSERT … ON CONFLICT DO UPDATE so
//      retried deliveries are safe no-ops for completed state.
//
// Stripe webhook signature algorithm (v1):
//   signed_payload = `${timestamp}.${raw_body}`
//   sig = HMAC-SHA256(webhook_secret, signed_payload)
//   header: `Stripe-Signature: t=<timestamp>,v1=<hex_sig>[,v1=<hex_sig>…]`

import type { AnalyticsEmitEnv } from "../lib/analytics-server";
import { emit, newEventId } from "../lib/analytics-server";
// SINGLE SOURCE OF TRUTH for the dispatched event set (see handled-stripe-events.json).
// The SAME file drives the live Stripe endpoint's enabled_events via
// scripts/ops/stripe-reconcile-webhook-events.sh, so the runtime allowlist and the
// dashboard subscription can never drift. Edit the JSON, not a literal here.
import handledStripeEvents from "./handled-stripe-events.json";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface StripeWebhookEnv extends AnalyticsEmitEnv {
    STRIPE_WEBHOOK_SECRET?: string;
    BILLING_DB?: D1DatabaseLike;
    // Stripe price ids per paid tier — the same env vars the checkout backend
    // (corelink-stripe-real client.rs:649 `STRIPE_PRICE_ID_{TIER}`) reads to
    // CREATE a session. The webhook reads them in REVERSE (price → tier) so an
    // in-place Stripe plan change on customer.subscription.updated can map the
    // new price back to our tier and propagate it to tier_selections.tier.
    STRIPE_PRICE_ID_SOLO?: string;
    STRIPE_PRICE_ID_STARTER?: string;
    STRIPE_PRICE_ID_TEAM?: string;
    STRIPE_PRICE_ID_PRO?: string;
    STRIPE_PRICE_ID_MAX?: string;
    // Stripe price ids per RUNNER tier — the runner subscription is its OWN
    // Stripe subscription (SEPARATE from the cache subscription), so it has its
    // own price ladder. The webhook reads these in REVERSE (price → runner
    // entitlement) to seed/revoke `runners_entitlement` (migrations 0070/0072)
    // via the dedicated `runner_billing` mapping table (migration 0087). A
    // runner price maps to NO cache tier, so the existing cache-path helpers
    // (tierFromSubscriptionPrice / resolveSubscriptionTier / detectTierPriceMismatch)
    // all resolve null for it — the cache path is a clean no-op for runner subs.
    STRIPE_PRICE_ID_RUNNER_STARTER?: string;
    STRIPE_PRICE_ID_RUNNER_PRO?: string;
    STRIPE_PRICE_ID_RUNNER_TEAM?: string;
    STRIPE_PRICE_ID_RUNNER_SCALE?: string;
    STRIPE_PRICE_ID_RUNNER_MAX?: string;
}

// Minimal D1 interface — keeps unit tests independent of @cloudflare/workers-types.
//
// `run()` returns the D1 result envelope; we only read `meta.changes` (the
// number of rows actually written) to detect whether an `INSERT OR IGNORE`
// idempotency claim newly inserted (changes === 1) or hit the PK conflict
// (changes === 0 → already processed).
interface D1RunResult {
    meta?: { changes?: number };
}
interface D1PreparedStatement {
    bind(...values: unknown[]): D1PreparedStatement;
    run(): Promise<D1RunResult>;
}
interface D1DatabaseLike {
    prepare(query: string): D1PreparedStatement;
}

interface StripeEventData {
    object: Record<string, unknown>;
    previous_attributes?: Record<string, unknown>;
}

interface StripeEvent {
    id: string;
    type: string;
    data: StripeEventData;
}

// ---------------------------------------------------------------------------
// Stripe signature verification
// ---------------------------------------------------------------------------

/** Max age of a Stripe webhook timestamp before we reject as a replay. */
const MAX_TIMESTAMP_AGE_MS = 5 * 60 * 1000; // 5 minutes

/**
 * Derive the HMAC-SHA256 key from a Stripe webhook signing secret.
 *
 * Stripe uses the **entire `whsec_…` secret string** (prefix included) as the
 * raw HMAC key — it does NOT strip the `whsec_` prefix and does NOT base64-decode
 * the remainder. Verified empirically against stripe-node's
 * `Stripe.webhooks.generateTestHeaderString` (see the KAT in the unit tests).
 *
 * The previous implementation base64-decoded the post-prefix remainder, which
 * yields a WRONG key, so every real Stripe signature failed verification (400
 * `invalid_signature`) and no paid customer's entitlement was ever materialised.
 */
function decodeWebhookSecret(secret: string): Uint8Array | null {
    if (!secret.startsWith("whsec_")) return null;
    return new TextEncoder().encode(secret);
}

/**
 * Verify a Stripe webhook signature header (constant-time compare).
 *
 * Returns the parsed payload string on success, null on failure.
 * Reads the raw body rather than re-serialising the parsed JSON to avoid
 * any canonicalization drift.
 */
export async function verifyStripeSignature(
    rawBody: string,
    sigHeader: string | null,
    secret: string,
    nowMs: number = Date.now(),
): Promise<boolean> {
    if (!sigHeader) return false;

    const secretBytes = decodeWebhookSecret(secret);
    if (!secretBytes) return false;

    // Parse `t=<timestamp>,v1=<sig>[,v1=<sig>…]`
    const parts = sigHeader.split(",");
    let timestamp: string | null = null;
    const v1Sigs: string[] = [];

    for (const part of parts) {
        const eq = part.indexOf("=");
        if (eq === -1) continue;
        const k = part.slice(0, eq);
        const v = part.slice(eq + 1);
        if (k === "t") timestamp = v;
        else if (k === "v1") v1Sigs.push(v);
    }

    if (!timestamp || v1Sigs.length === 0) return false;

    // Replay-attack guard: timestamp must be within MAX_TIMESTAMP_AGE_MS.
    const ts = parseInt(timestamp, 10);
    if (isNaN(ts) || Math.abs(nowMs - ts * 1000) > MAX_TIMESTAMP_AGE_MS) return false;

    // Compute expected HMAC-SHA256 over `${timestamp}.${rawBody}`.
    const key = await crypto.subtle.importKey(
        "raw",
        secretBytes,
        { name: "HMAC", hash: "SHA-256" },
        false,
        ["sign"],
    );
    const toSign = new TextEncoder().encode(`${timestamp}.${rawBody}`);
    const sigBytes = new Uint8Array(await crypto.subtle.sign("HMAC", key, toSign));

    // Convert to lowercase hex (Stripe uses hex, not base64).
    const expected = Array.from(sigBytes)
        .map((b) => b.toString(16).padStart(2, "0"))
        .join("");

    // Constant-time compare against each v1 candidate.
    for (const candidate of v1Sigs) {
        if (candidate.length !== expected.length) continue;
        let diff = 0;
        for (let i = 0; i < candidate.length; i++) {
            diff |= candidate.charCodeAt(i) ^ expected.charCodeAt(i);
        }
        if (diff === 0) return true;
    }

    return false;
}

// ---------------------------------------------------------------------------
// Metadata helpers
// ---------------------------------------------------------------------------

/** Extract tenant_id from Stripe metadata. Returns null if absent. */
function tenantIdFromMetadata(obj: Record<string, unknown>): string | null {
    const meta = obj["metadata"] as Record<string, unknown> | undefined;
    const raw = meta?.["tenant_id"];
    return typeof raw === "string" && raw.length > 0 ? raw : null;
}

/** Extract clerk_user_id from Stripe metadata. Returns null if absent. */
function clerkUserIdFromMetadata(obj: Record<string, unknown>): string | null {
    const meta = obj["metadata"] as Record<string, unknown> | undefined;
    const raw = meta?.["clerk_user_id"];
    return typeof raw === "string" && raw.length > 0 ? raw : null;
}

/** Cents → USD float. Stripe amounts are integer cents. */
function centsToUsd(cents: unknown): number {
    return typeof cents === "number" ? cents / 100 : 0;
}

/** The canonical paid tiers the tier_selections FSM may be activated on. */
type PaidTier = "solo" | "starter" | "team" | "pro" | "max";

/** Coerce an arbitrary string to a known paid tier, or null. */
function asPaidTier(raw: unknown): PaidTier | null {
    return raw === "solo" ||
        raw === "starter" ||
        raw === "team" ||
        raw === "pro" ||
        raw === "max"
        ? raw
        : null;
}

/** Read a paid tier straight from Stripe `metadata[tier]` (checkout sets it). */
function tierFromMetadata(obj: Record<string, unknown>): PaidTier | null {
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
function tierFromSubscriptionPrice(
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
function resolveSubscriptionTier(
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
function detectTierPriceMismatch(
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
function subscriptionStatusGrantsAccess(rawStatus: unknown): boolean {
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
type RunnerTier =
    | "runner_starter"
    | "runner_pro"
    | "runner_team"
    | "runner_scale"
    | "runner_max";

/** The per-tier Runners entitlement (max_concurrency, max_vcpu_h). */
interface RunnerEntitlement {
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
const RUNNER_TIER_ENTITLEMENT: Record<RunnerTier, RunnerEntitlement> = {
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
function runnerTierFromMetadata(obj: Record<string, unknown>): RunnerTier | null {
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
function runnerEntitlementFromSubscriptionPrice(
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
export const HANDLED_EVENT_TYPES = new Set<string>(handledStripeEvents.enabled_events);

/** Outcome of an idempotency claim against `stripe_webhook_events_processed`. */
type ClaimResult =
    | "claimed" // newly inserted — this delivery is the FIRST: process + emit.
    | "duplicate" // PK conflict — already processed: process (idempotent writes), SKIP emit.
    | "claim_error"; // the claim INSERT itself failed (D1 error) — fail-safe → treat as first.

/**
 * Durably claim a Stripe `event.id` AFTER the idempotent entitlement/billing
 * writes have SUCCEEDED (migration 0044 `stripe_webhook_events_processed`). The
 * claim outcome gates ONLY the non-idempotent analytics emit (#37,
 * process-then-claim): the writes already committed before we get here.
 *
 * Uses `INSERT OR IGNORE` on the `event_id` PRIMARY KEY and inspects
 * `meta.changes`:
 *   - changes === 1 → row newly inserted → "claimed" (first SUCCESSFUL delivery: emit).
 *   - changes === 0 → PK conflict → "duplicate" (retry after a prior success: SKIP emit).
 *
 * Ordering / fail-safe (PROCESS-then-CLAIM): the caller awaits the idempotent D1
 * upserts FIRST (returning 500 on any failure so Stripe redelivers), and only
 * then claims — so the claim row exists iff a delivery SUCCEEDED. This makes the
 * emit exactly-once on the successful delivery: a first attempt that FAILED
 * never claimed (it 500'd before reaching here), so Stripe's retry completes the
 * writes and emits; a retry after a prior SUCCESS hits the PK conflict and skips
 * the emit (no double MRR) while the idempotent writes re-run harmlessly.
 *
 * If the claim INSERT itself throws (D1 unavailable), we return "claim_error"
 * and the caller treats it as a FIRST delivery (emit): the writes already
 * succeeded, so a 200 is correct, and losing a first-time revenue emit is worse
 * than a possible double count. We never drop a genuine first emit.
 *
 * `outcome` records whether we dispatched a known event or merely acknowledged
 * an unknown one (forensics column; not read by the dedup path).
 */
async function claimWebhookEvent(
    db: D1DatabaseLike,
    opts: {
        eventId: string;
        eventType: string;
        nowMs: number;
        correlationId: string;
    },
): Promise<ClaimResult> {
    const outcome = HANDLED_EVENT_TYPES.has(opts.eventType)
        ? "dispatched"
        : "acknowledged_unknown";
    try {
        const res = await db
            .prepare(
                `INSERT OR IGNORE INTO stripe_webhook_events_processed
                   (event_id, event_type, processed_at_ms, outcome, correlation_id)
                 VALUES (?1, ?2, ?3, ?4, ?5)`,
            )
            .bind(opts.eventId, opts.eventType, opts.nowMs, outcome, opts.correlationId)
            .run();
        // `INSERT OR IGNORE` writes 1 row on first delivery, 0 on PK conflict.
        return (res?.meta?.changes ?? 0) > 0 ? "claimed" : "duplicate";
    } catch (e: unknown) {
        // Fail-safe: do NOT lose a first-time event. Proceed to process.
        console.warn(
            `[stripe-webhook] idempotency claim failed (processing anyway): ${(e as Error).message}`,
        );
        return "claim_error";
    }
}

// ---------------------------------------------------------------------------
// D1 writers
// ---------------------------------------------------------------------------

/**
 * Upsert a tenant_billing row on `checkout.session.completed`.
 *
 * Uses INSERT … ON CONFLICT (tenant_id) DO UPDATE so Stripe retries are safe.
 */
async function upsertBillingPaid(
    db: D1DatabaseLike,
    opts: {
        tenantId: string;
        stripeCustomerId: string;
        stripeSubscriptionId: string | null;
        plan: string | null;
        currentPeriodEndMs: number | null;
        nowMs: number;
    },
): Promise<void> {
    await db
        .prepare(
            `INSERT INTO tenant_billing
               (tenant_id, stripe_customer_id, stripe_subscription_id,
                status, plan, current_period_end_ms,
                schema_version, created_at_ms, updated_at_ms)
             VALUES (?1, ?2, ?3, 'paid', ?4, ?5, 1, ?6, ?6)
             ON CONFLICT (tenant_id) DO UPDATE SET
               stripe_customer_id     = excluded.stripe_customer_id,
               stripe_subscription_id = excluded.stripe_subscription_id,
               status                 = 'paid',
               plan                   = excluded.plan,
               current_period_end_ms  = excluded.current_period_end_ms,
               updated_at_ms          = excluded.updated_at_ms`,
        )
        .bind(
            opts.tenantId,
            opts.stripeCustomerId,
            opts.stripeSubscriptionId,
            opts.plan,
            opts.currentPeriodEndMs,
            opts.nowMs,
        )
        .run();
}

/**
 * Update billing status + period-end on `customer.subscription.updated`.
 * Looks up by stripe_subscription_id so we can find the tenant even if the
 * webhook arrives before tenant_id metadata is available.
 *
 * TERMINAL-STATE GUARD (audit fix 4): 'canceled' is terminal for a given
 * stripe_subscription_id. Stripe webhooks are NOT ordered — a late/out-of-order
 * `customer.subscription.updated(status=active)` arriving AFTER
 * `customer.subscription.deleted` used to resurrect tenant_billing to 'paid'
 * (the canonical tier_selections gate was already safe: activation only happens
 * via checkout.session.completed). `AND status != 'canceled'` makes the mirror
 * match the canonical gate: once canceled, only a NEW checkout (the
 * upsertBillingPaid ON CONFLICT path, which carries a new subscription id) can
 * move the row forward.
 */
async function updateBillingSubscription(
    db: D1DatabaseLike,
    opts: {
        stripeSubscriptionId: string;
        status: string;
        currentPeriodEndMs: number | null;
        nowMs: number;
    },
): Promise<void> {
    await db
        .prepare(
            `UPDATE tenant_billing
             SET status = ?1,
                 current_period_end_ms = ?2,
                 updated_at_ms = ?3
             WHERE stripe_subscription_id = ?4
               AND status != 'canceled'`,
        )
        .bind(opts.status, opts.currentPeriodEndMs, opts.nowMs, opts.stripeSubscriptionId)
        .run();
}

/**
 * Update ONLY `tenant_billing.status` (leave period-end + ids untouched), keyed
 * by subscription id. Used on invoice.payment_failed so a dunning failure marks
 * the row 'past_due' without clobbering the existing current_period_end_ms.
 * Idempotent on redelivery. Same terminal-state guard as
 * updateBillingSubscription (audit fix 4): a late invoice.payment_failed after
 * subscription.deleted must not move a 'canceled' row to 'past_due'.
 */
async function updateBillingStatus(
    db: D1DatabaseLike,
    opts: { stripeSubscriptionId: string; status: string; nowMs: number },
): Promise<void> {
    await db
        .prepare(
            `UPDATE tenant_billing
             SET status = ?1,
                 updated_at_ms = ?2
             WHERE stripe_subscription_id = ?3
               AND status != 'canceled'`,
        )
        .bind(opts.status, opts.nowMs, opts.stripeSubscriptionId)
        .run();
}

/**
 * Mark billing as canceled on `customer.subscription.deleted`.
 */
async function cancelBilling(
    db: D1DatabaseLike,
    opts: { stripeSubscriptionId: string; nowMs: number },
): Promise<void> {
    await db
        .prepare(
            `UPDATE tenant_billing
             SET status = 'canceled',
                 updated_at_ms = ?1
             WHERE stripe_subscription_id = ?2`,
        )
        .bind(opts.nowMs, opts.stripeSubscriptionId)
        .run();
}

/**
 * Advance `tier_selections.subscription_state` → 'active' on a completed paid
 * checkout (GAP-6 reconciliation).
 *
 * The container persists the row as `pending_checkout` when checkout starts;
 * THIS is the only writer on the live path that flips it to `active` (the
 * in-process `corelink-tier-selection::ledger` that does so is test-only and
 * never runs in production). Without this, a paid tenant stays
 * `pending_checkout` forever, the container's `has_active_subscription` guard
 * never fires, and a returning customer can open a SECOND subscription.
 *
 * Idempotent (`ON CONFLICT DO UPDATE`) so Stripe retries converge. Uses UPSERT
 * (not a bare UPDATE) so that even if the container's `pending_checkout` persist
 * was lost, a paid completion still yields an `active` row. `subscription_state`
 * is only ever set to 'active' here with a non-null `subscription_started_at_ms`,
 * satisfying the `subscription_started_when_active` CHECK.
 *
 * The ON CONFLICT UPDATE is guarded `WHERE subscription_state <> 'active'`, so a
 * duplicate or out-of-order `checkout.session.completed` can never downgrade an
 * already-active tier or shift its activation timestamp (review hardening —
 * Stripe can redeliver and reorder webhook events). When the guard DOES fire
 * (the row was previously deactivated — state <> 'active' with
 * subscription_started_at_ms = NULL), the UPDATE rewrites
 * subscription_started_at_ms to a fresh non-null value so the re-activated row
 * still satisfies the `subscription_started_when_active` CHECK (a returning
 * customer would otherwise be locked out — the NULL timestamp left by
 * deactivateTierSelection* would violate the CHECK and throw).
 */
async function activatePaidTierSelection(
    db: D1DatabaseLike,
    opts: {
        tenantId: string;
        tier: string;
        stripeCustomerId: string;
        nowMs: number;
        correlationId: string;
    },
): Promise<void> {
    await db
        .prepare(
            `INSERT INTO tier_selections
               (tenant_id, tier, subscription_state, stripe_customer_id,
                subscription_started_at_ms, schema_version, correlation_id)
             VALUES (?1, ?2, 'active', ?3, ?4, 1, ?5)
             ON CONFLICT (tenant_id) DO UPDATE SET
               tier                       = excluded.tier,
               subscription_state         = 'active',
               stripe_customer_id         = excluded.stripe_customer_id,
               -- Re-activation MUST write a fresh non-null timestamp: a prior
               -- cancel/payment-failure ran deactivateTierSelection* which set
               -- subscription_started_at_ms = NULL, so without this the
               -- (state='active' AND started_at IS NULL) row violates the 0039
               -- subscription_started_when_active CHECK and the write throws —
               -- locking the paying re-subscriber out. Bind ?4 (= nowMs), the
               -- same value the INSERT arm writes; NOT a COALESCE of the
               -- existing column (which is NULL after cancel).
               subscription_started_at_ms = ?4
             WHERE tier_selections.subscription_state <> 'active'`,
        )
        .bind(
            opts.tenantId,
            opts.tier,
            opts.stripeCustomerId,
            opts.nowMs,
            opts.correlationId,
        )
        .run();
}

/**
 * Revoke the canonical access gate: flip `tier_selections.subscription_state`
 * away from 'active' so `has_active_subscription` (tier_select_store.rs:192,
 * `… WHERE subscription_state = 'active'`) returns false. Used on
 * invoice.payment_failed (final dunning), subscription.deleted (cancel), and
 * any subscription.updated that no longer grants access.
 *
 * Keyed by `stripe_customer_id` because subscription-lifecycle events identify
 * the tenant by customer, not by tenant_id metadata (which subscription objects
 * may lack — only the checkout SESSION carried it). We resolve the customer
 * here; tier_selections.stripe_customer_id is written on activation.
 *
 * `inactive` (NOT `pending_checkout`) is chosen per the 0039 CHECK so the tenant
 * (a) immediately loses access and (b) can re-subscribe — tier_select.rs:699
 * `AlreadyActive` only blocks a tenant whose state is still 'active'.
 *
 * Idempotent: a bare UPDATE with `WHERE … <> 'inactive'` is a safe no-op on
 * redelivery, and clearing `subscription_started_at_ms` keeps the
 * `subscription_started_when_active` CHECK satisfied for the non-active state.
 */
async function deactivateTierSelectionByCustomer(
    db: D1DatabaseLike,
    opts: { stripeCustomerId: string },
): Promise<void> {
    await db
        .prepare(
            `UPDATE tier_selections
             SET subscription_state = 'inactive',
                 subscription_started_at_ms = NULL
             WHERE stripe_customer_id = ?1
               AND subscription_state <> 'inactive'`,
        )
        .bind(opts.stripeCustomerId)
        .run();
}

/**
 * Revoke the canonical access gate keyed by `stripe_subscription_id` — used when
 * a subscription-lifecycle event (e.g. subscription.updated → canceled/unpaid)
 * arrives WITHOUT a top-level `customer` field. Stripe subscription objects are
 * not guaranteed to carry `customer`, but they always carry their own id; if we
 * gated revocation on `customer` we would fail-OPEN and leave a non-paying
 * tenant entitled.
 *
 * `tier_selections` has no `stripe_subscription_id` column (0039), so we resolve
 * the tenant through `tenant_billing` (which maps subscription id → tenant id,
 * 0055) via a correlated subquery. Bare idempotent UPDATE; clears
 * `subscription_started_at_ms` to keep the `subscription_started_when_active`
 * CHECK satisfied for the now non-active state. Safe no-op if no billing row
 * maps the subscription id (e.g. event for an unknown/foreign subscription).
 */
async function deactivateTierSelectionBySubscription(
    db: D1DatabaseLike,
    opts: { stripeSubscriptionId: string },
): Promise<void> {
    await db
        .prepare(
            `UPDATE tier_selections
             SET subscription_state = 'inactive',
                 subscription_started_at_ms = NULL
             WHERE tenant_id IN (
                       SELECT tenant_id FROM tenant_billing
                       WHERE stripe_subscription_id = ?1
                   )
               AND subscription_state <> 'inactive'`,
        )
        .bind(opts.stripeSubscriptionId)
        .run();
}

/**
 * Re-activate the canonical access gate after a DUNNING RECOVERY: flip
 * `tier_selections.subscription_state` back to 'active' when Stripe reports a
 * subscription has returned to active/trialing WITHOUT a fresh checkout session
 * (payment-method fix + automatic retry succeeds, an operator marks an invoice
 * paid, or an incomplete state resolves to trialing). Stripe fires
 * `customer.subscription.updated` with grantsAccess=true in those cases but
 * never re-runs checkout.session.completed, so without this the paying tenant is
 * stranded with subscription_state='inactive' and the quota gate denies them.
 *
 * TERMINAL-CANCEL GUARD (critical): a CANCEL and a DUNNING deactivation BOTH
 * leave `tier_selections.subscription_state='inactive'`, so that column alone
 * cannot tell a recoverable dunning lapse from a terminal cancel. Stripe
 * webhooks are unordered — a late, out-of-order `subscription.updated(active)`
 * arriving AFTER `subscription.deleted` must NOT resurrect a canceled
 * subscription (re-subscribing must go through checkout). We therefore gate the
 * re-activation on the authoritative `tenant_billing.status != 'canceled'`
 * (mirroring updateBillingSubscription's audit-fix-4 guard): the tenant is
 * resolved through `tenant_billing` BY THIS SUBSCRIPTION ID, and a canceled
 * billing row makes the re-activation a no-op. Keyed by subscription id (not
 * customer) precisely so we can join `tenant_billing` for that guard;
 * subscription objects always carry their own id.
 *
 * This NEVER inserts — it can ONLY resurrect a row a prior checkout already
 * created — so the single-activation-writer invariant holds (checkout remains
 * the sole CREATOR). Restores `subscription_started_at_ms` to a fresh non-null
 * value (the prior deactivate set it NULL) so the re-activated row still
 * satisfies the 0039 `subscription_started_when_active` CHECK. `tier` is left to
 * the separate `updateTierSelectionTierByCustomer` propagation. Idempotent: the
 * `WHERE … <> 'active'` filter makes a redelivery a no-op (and prevents shifting
 * the activation timestamp of an already-active row).
 */
async function reactivateTierSelectionBySubscription(
    db: D1DatabaseLike,
    opts: { stripeSubscriptionId: string; nowMs: number },
): Promise<void> {
    await db
        .prepare(
            `UPDATE tier_selections
             SET subscription_state = 'active',
                 subscription_started_at_ms = ?2
             WHERE tenant_id IN (
                       SELECT tenant_id FROM tenant_billing
                       WHERE stripe_subscription_id = ?1
                         AND status != 'canceled'
                   )
               AND subscription_state <> 'active'`,
        )
        .bind(opts.stripeSubscriptionId, opts.nowMs)
        .run();
}

/**
 * Propagate an in-place plan change (Stripe price swap on the SAME subscription)
 * to `tier_selections.tier`. checkout.session.completed is the only OTHER writer
 * of `tier`, so without this a tenant who upgrades/downgrades inside Stripe keeps
 * their old entitlement tier forever.
 *
 * Keyed by customer; only touches the `tier` column. Idempotent (writing the
 * same tier twice is a no-op). Never called with an unknown tier (caller gates
 * on a non-null PaidTier — fail-safe).
 *
 * F35 FIX: `AND subscription_state = 'active'` is added so this is a no-op on
 * rows that are already inactive/canceled. Without the filter, a concurrent
 * `customer.subscription.updated` event that both changes the price AND cancels
 * the subscription (both paths flushed via `Promise.all`) writes a stale tier
 * onto an inactive row: the access gate (quota.ts `subscription_state='active'`)
 * is unaffected, but the row carries an incorrect tier label that misleads
 * forensic/audit queries. The filter makes the tier update a no-op when
 * `deactivateTierSelectionByCustomer` wins the D1 race (or has already run),
 * preventing stale tier data on inactive rows.
 */
async function updateTierSelectionTierByCustomer(
    db: D1DatabaseLike,
    opts: { stripeCustomerId: string; tier: PaidTier },
): Promise<void> {
    await db
        .prepare(
            `UPDATE tier_selections
             SET tier = ?1
             WHERE stripe_customer_id = ?2
               AND subscription_state = 'active'`,
        )
        .bind(opts.tier, opts.stripeCustomerId)
        .run();
}

/**
 * Backfill `tenant_billing.current_period_end_ms` on customer.subscription.created.
 * checkout.session.completed writes the row with a null period-end (the session
 * object does not carry it); .created fires immediately after and DOES carry
 * `current_period_end`. Keyed by subscription id. Pure UPDATE — never inserts —
 * so it can only enrich an existing paid row, never create one out of band.
 * Idempotent: writing the same period-end twice is a no-op.
 */
async function backfillPeriodEnd(
    db: D1DatabaseLike,
    opts: { stripeSubscriptionId: string; currentPeriodEndMs: number; nowMs: number },
): Promise<void> {
    await db
        .prepare(
            `UPDATE tenant_billing
             SET current_period_end_ms = ?1,
                 updated_at_ms = ?2
             WHERE stripe_subscription_id = ?3`,
        )
        .bind(opts.currentPeriodEndMs, opts.nowMs, opts.stripeSubscriptionId)
        .run();
}

// ---------------------------------------------------------------------------
// Runner billing / entitlement D1 writers
//
// All idempotent (ON CONFLICT upsert / guarded UPDATE / DELETE), so a Stripe
// redelivery re-runs them harmlessly. Pushed onto `requiredWrites` so they
// inherit the #37 await/500-on-failure durability contract (a runner customer
// paid and is owed entitlement — a failed write must 500 for redelivery, never
// silently 200).
// ---------------------------------------------------------------------------

/**
 * Upsert the `runner_billing` row that maps a RUNNER Stripe subscription →
 * tenant (migration 0087). Keyed by `runner_subscription_id` (PK) so it does
 * NOT clobber the one-row-per-tenant `tenant_billing` cache row and so a tenant
 * can hold both a cache AND a runner subscription. On conflict we advance only
 * `status` + `updated_at_ms` (the immutable tenant/plan/customer/created stay
 * as first written) — a redelivery converges harmlessly.
 */
async function upsertRunnerBilling(
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
async function upsertRunnersEntitlementBySubscription(
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
 * REVOKE the tenant's Runners entitlement for a cancelled/lapsed subscription.
 * Resolves the tenant through `runner_billing` (subscription id → tenant id) and
 * deletes the `runners_entitlement` row ONLY when the tenant retains no other
 * active/trialing runner subscription (see the inline note — avoids nuking a
 * still-paying tenant). An ABSENT row means "no Runners entitlement" (migration
 * 0070 fail-CLOSED semantics). Idempotent: a redelivery deletes an already-absent
 * row (0 rows affected). Safe no-op if the subscription maps no billing row.
 */
async function revokeRunnersEntitlementBySubscription(
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
async function markRunnerBillingStatusBySubscription(
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
function queueCheckoutActivation(
    env: StripeWebhookEnv,
    requiredWrites: Array<Promise<void>>,
    opts: {
        sessionId: string | undefined;
        tenantId: string;
        stripeCustomerId: string;
        stripeSubscriptionId: string | null;
        plan: string;
        amountTotal: number | undefined;
        nowMs: number;
    },
): Parameters<typeof emit>[1] {
    if (env.BILLING_DB) {
        const db = env.BILLING_DB;
        // REQUIRED durable write — awaited by the caller; a failure returns
        // 500 so Stripe redelivers (the customer paid, they are owed the
        // billing row).
        requiredWrites.push(
            upsertBillingPaid(db, {
                tenantId: opts.tenantId,
                stripeCustomerId: opts.stripeCustomerId,
                stripeSubscriptionId: opts.stripeSubscriptionId,
                plan: opts.plan,
                currentPeriodEndMs: null,
                nowMs: opts.nowMs,
            }),
        );
        // GAP-6: reconcile the canonical subscription FSM the money path
        // reads. Only a real paid tier may flip to 'active' (free never
        // reaches Stripe; enterprise uses the inquiry form). An unrecognised
        // tier is left un-activated rather than written with a bogus value.
        // Uses the canonical asPaidTier set — an inline starter/team/pro
        // triple here silently skipped Solo/Max activation
        // (pay-but-not-entitled for the $15/$149 SKUs).
        const paidTier = asPaidTier(opts.plan);
        if (paidTier) {
            requiredWrites.push(
                activatePaidTierSelection(db, {
                    tenantId: opts.tenantId,
                    tier: paidTier,
                    stripeCustomerId: opts.stripeCustomerId,
                    nowMs: opts.nowMs,
                    correlationId: `stripe_checkout:${opts.sessionId ?? opts.stripeCustomerId}`,
                }),
            );
        }
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

// ---------------------------------------------------------------------------
// Main handler
// ---------------------------------------------------------------------------

export async function handleStripeWebhook(
    request: Request,
    env: StripeWebhookEnv,
    ctx: ExecutionContext,
): Promise<Response> {
    if (!env.STRIPE_WEBHOOK_SECRET) {
        return new Response("stripe webhook secret not configured", { status: 503 });
    }

    // FAIL-CLOSED (audit fix 2, mirrors the secret check above): without the
    // BILLING_DB binding every money-path write below would silently no-op and
    // the handler would still return 200 — Stripe would never retry, so a PAID
    // checkout would be acked with zero entitlement writes and no recovery
    // (silent paid-but-no-access). A missing binding is a deploy
    // misconfiguration: return 503 so Stripe retries until it is fixed. The
    // inner `if (env.BILLING_DB)` guards below remain for TS narrowing only.
    if (!env.BILLING_DB) {
        console.error(
            "[stripe-webhook] BILLING_DB binding missing — returning 503 (fail-closed) " +
                "so Stripe retries instead of silently dropping money-path writes",
        );
        return new Response("billing_db_unbound", { status: 503 });
    }

    // --- 1. Read raw body (required for signature verification). --------
    // We MUST read the raw bytes before any JSON.parse — re-serializing
    // would invalidate the HMAC.
    let rawBody: string;
    try {
        rawBody = await request.text();
    } catch {
        return new Response("body_read_error", { status: 400 });
    }

    // --- 2. Verify Stripe webhook signature BEFORE any side effect. -----
    const sigHeader = request.headers.get("stripe-signature");
    const valid = await verifyStripeSignature(rawBody, sigHeader, env.STRIPE_WEBHOOK_SECRET);
    if (!valid) {
        // 400 tells Stripe the event was rejected cleanly (vs 5xx which triggers retry).
        return new Response("invalid_signature", { status: 400 });
    }

    // --- 3. Parse event. ------------------------------------------------
    let event: StripeEvent;
    try {
        event = JSON.parse(rawBody) as StripeEvent;
    } catch {
        return new Response("invalid_json", { status: 400 });
    }

    const obj = event.data.object;
    const tenantId = tenantIdFromMetadata(obj);
    const nowMs = Date.now();

    // --- 4. Collect the REQUIRED durable entitlement/billing writes + the
    // analytics emit, then run them in the unified process-then-claim flow below
    // (#37). Each case pushes its idempotent D1 writes onto `requiredWrites` (we
    // AWAIT them — a failure means 500 so Stripe redelivers) and sets `emitEvent`
    // to the analytics payload it would emit. The claim happens AFTER the writes
    // succeed, so the emit is exactly-once on the first SUCCESSFUL delivery.
    const requiredWrites: Array<Promise<void>> = [];
    let emitEvent: Parameters<typeof emit>[1] | null = null;

    switch (event.type) {
        // checkout.session.async_payment_succeeded is how a DELAYED payment
        // method (SEPA debit, ACH, …) eventually activates: its session
        // completed earlier with payment_status='unpaid' (gated below, no
        // writes), and this event fires once the payment actually clears. The
        // session object is the same shape, so it shares the completed arm —
        // identical validation + identical activation (audit fix 1).
        case "checkout.session.async_payment_succeeded":
        case "checkout.session.completed": {
            // PAYMENT-STATUS ACTIVATION GATE (audit fix 1): a
            // checkout.session.completed fires even when an async payment
            // method has NOT been charged yet (payment_status='unpaid'). The
            // old handler activated entitlement without ever reading
            // payment_status — granting access for money that may never
            // arrive. Gate ALL writes on the session being actually paid:
            //   - 'paid' / 'no_payment_required' → activate below.
            //   - 'unpaid' → ack with 200 and write NOTHING; entitlement
            //     arrives via checkout.session.async_payment_succeeded (and a
            //     failure via …async_payment_failed, which has nothing to
            //     revoke precisely because we skipped the writes here).
            //   - anything else (missing/unknown) → fail loud (500,
            //     redelivery): fail-safe in BOTH directions — never grant on
            //     an unproven payment, never silently drop a session we do
            //     not understand.
            const paymentStatus = obj["payment_status"];
            if (paymentStatus === "unpaid") {
                console.log(
                    `[stripe-webhook] ${event.type} with payment_status=unpaid ` +
                        `(session=${(obj["id"] as string | undefined) ?? "unknown"}) — ` +
                        `async payment still pending; skipping all entitlement/billing ` +
                        `writes (activation arrives via checkout.session.async_payment_succeeded)`,
                );
                return new Response("ok", { status: 200 });
            }
            if (paymentStatus !== "paid" && paymentStatus !== "no_payment_required") {
                console.error(
                    `[stripe-webhook] ${event.type} with unrecognized payment_status=` +
                        `${typeof paymentStatus === "string" ? paymentStatus : "<absent>"} ` +
                        `(session=${(obj["id"] as string | undefined) ?? "unknown"}); ` +
                        `returning 500 for redelivery rather than activating an unproven payment`,
                );
                return new Response("checkout_payment_status_unknown", { status: 500 });
            }

            // Extract Stripe IDs from the session object.
            const stripeCustomerId = obj["customer"] as string | null | undefined;
            const stripeSubscriptionId = obj["subscription"] as string | null | undefined;

            // RUNNER PURCHASE BRANCH. A runner checkout carries metadata[tier]
            // = runner_starter..runner_max (asPaidTier(runner_*) is null, so the
            // cache activation below is ALREADY a no-op for it — this branch is
            // the explicit, self-documenting seed of the SEPARATE runner
            // subscription). We map the runner subscription → tenant in
            // runner_billing (the dedicated 0087 table, so it never clobbers the
            // one-row-per-tenant tenant_billing) and return WITHOUT running any
            // cache activation. The entitlement itself (runners_entitlement) is
            // seeded on the customer.subscription.created/updated event that
            // carries the runner PRICE (checkout sessions carry no price).
            const runnerTier = runnerTierFromMetadata(obj);
            if (runnerTier) {
                if (!tenantId || typeof stripeCustomerId !== "string" || !stripeCustomerId) {
                    console.error(
                        `[stripe-webhook] ${event.type} runner purchase missing ` +
                            `${!tenantId ? "tenant_id" : "customer"} ` +
                            `(session=${(obj["id"] as string | undefined) ?? "unknown"}); ` +
                            `returning 500 for redelivery rather than dropping a paid runner signup`,
                    );
                    return new Response("checkout_runner_missing_tenant_or_customer", {
                        status: 500,
                    });
                }
                // A runner subscription MUST carry its subscription id (the map
                // key). Without it we cannot seed entitlement on the follow-up
                // subscription event — fail loud so Stripe redelivers.
                if (typeof stripeSubscriptionId !== "string" || !stripeSubscriptionId) {
                    console.error(
                        `[stripe-webhook] ${event.type} runner purchase missing subscription id ` +
                            `(session=${(obj["id"] as string | undefined) ?? "unknown"}); ` +
                            `returning 500 for redelivery rather than dropping a paid runner signup`,
                    );
                    return new Response("checkout_runner_missing_subscription", { status: 500 });
                }
                requiredWrites.push(
                    upsertRunnerBilling(env.BILLING_DB, {
                        runnerSubscriptionId: stripeSubscriptionId,
                        tenantId,
                        plan: runnerTier,
                        status: "active",
                        stripeCustomerId,
                        nowMs,
                    }),
                );
                emitEvent = {
                    id: newEventId(),
                    event_name: "runner_subscription_started",
                    tenant_id: tenantId,
                    properties: {
                        plan: runnerTier,
                        mrr_usd: centsToUsd(obj["amount_total"] as number | undefined),
                        stripe_customer_id: stripeCustomerId,
                        stripe_subscription_id: stripeSubscriptionId,
                    },
                };
                break;
            }
            // The checkout session sets metadata[tier] (corelink-stripe-real
            // client.rs:630) — there is NO metadata[plan]. Read the real tier so
            // the billing row + the tier_selections activation record the right
            // plan. (Previously this read [plan], which is always absent, and
            // silently recorded "starter" for every team/pro customer.)
            const tierMeta = (obj["metadata"] as Record<string, unknown> | undefined)?.["tier"];
            // checkout.session does not carry current_period_end — that lives on
            // the subscription object. We leave it null here; it will be filled by
            // the subsequent customer.subscription.created / updated event Stripe
            // fires immediately after. Period-end is non-critical for activation.
            const amountTotal = obj["amount_total"] as number | undefined;
            const clerkUserId = clerkUserIdFromMetadata(obj);
            void clerkUserId; // recorded in metadata for audit, not needed here

            // FAIL-LOUD (money path): a paid checkout.session.completed MUST
            // carry tenant_id (the checkout backend sets metadata[tenant_id])
            // AND a Stripe customer. If either is missing, the customer has
            // PAID but cannot be provisioned — silently returning 200 (the old
            // behavior) dropped the entitlement with NO retry and NO operator
            // signal. Return 500 BEFORE the claim so Stripe redelivers and the
            // failure is visible; never silently ack a paid checkout we could
            // not apply.
            if (!tenantId || typeof stripeCustomerId !== "string" || !stripeCustomerId) {
                console.error(
                    `[stripe-webhook] ${event.type} missing ` +
                        `${!tenantId ? "tenant_id" : "customer"} ` +
                        `(session=${(obj["id"] as string | undefined) ?? "unknown"}); ` +
                        `returning 500 for redelivery rather than dropping a paid signup`,
                );
                return new Response("checkout_missing_tenant_or_customer", {
                    status: 500,
                });
            }

            // FAIL-LOUD (money path, same class as above): our checkout backend
            // ALWAYS sets metadata[tier] (corelink-stripe-real client.rs:630).
            // A session without it cannot be attributed to a SKU — the old
            // fallback recorded "starter" for whatever the customer actually
            // bought (a Max $149 checkout would be billing-rowed as Starter
            // $35). Return 500 so Stripe redelivers and the anomalous session
            // is visible instead of silently mispriced.
            if (typeof tierMeta !== "string" || !tierMeta) {
                console.error(
                    `[stripe-webhook] ${event.type} missing metadata[tier] ` +
                        `(session=${(obj["id"] as string | undefined) ?? "unknown"}); ` +
                        `returning 500 for redelivery rather than recording a wrong plan`,
                );
                return new Response("checkout_missing_tier", { status: 500 });
            }
            const plan = tierMeta;

            // NOTE (audit fix 3): no price↔tier cross-check is possible here —
            // the webhook session payload carries no price data (`line_items`
            // only exists on an API retrieval with `expand[]`) and we make no
            // Stripe API calls from the webhook. The cross-check runs on the
            // customer.subscription.created/updated events that immediately
            // follow, where the price IS in the payload — see
            // detectTierPriceMismatch.

            // Shared activation (audit fix 1): identical for a paid completed
            // session and for async_payment_succeeded.
            emitEvent = queueCheckoutActivation(env, requiredWrites, {
                sessionId: obj["id"] as string | undefined,
                tenantId,
                stripeCustomerId,
                stripeSubscriptionId:
                    typeof stripeSubscriptionId === "string" ? stripeSubscriptionId : null,
                plan,
                amountTotal,
                nowMs,
            });
            break;
        }

        case "checkout.session.async_payment_failed": {
            // The delayed payment for an earlier 'unpaid' completed session
            // failed. NOTHING was granted at completed-time (the
            // payment_status gate above skipped all writes), so there is
            // nothing to revoke — log for forensics and ack so Stripe stops
            // retrying. No writes, no analytics emit.
            console.log(
                `[stripe-webhook] checkout.session.async_payment_failed ` +
                    `(session=${(obj["id"] as string | undefined) ?? "unknown"}, ` +
                    `tenant=${tenantId ?? "unknown"}) — no entitlement was granted, ` +
                    `nothing to revoke; acknowledging`,
            );
            break;
        }

        case "customer.subscription.updated": {
            const stripeSubscriptionId = obj["id"] as string | undefined;
            if (!stripeSubscriptionId) break;

            // DEFENSE-IN-DEPTH price↔tier assert (audit fix 3): when the
            // subscription payload carries BOTH metadata[tier] and a
            // recognizable price, they must agree — a disagreement means the
            // checkout backend desynced the server-set tier from the price the
            // customer is actually paying, and silently preferring either side
            // could entitle the wrong SKU. Fail loud BEFORE any write (500 →
            // Stripe redelivers; the mismatch stays visible until fixed).
            {
                const mismatch = detectTierPriceMismatch(obj, env);
                if (mismatch) {
                    console.error(
                        `[stripe-webhook] ${event.type} price↔tier mismatch ` +
                            `(subscription=${stripeSubscriptionId}): metadata[tier]=` +
                            `${mismatch.metaTier} but the subscribed price maps to ` +
                            `${mismatch.priceTier}; returning 500 for redelivery rather ` +
                            `than entitling the wrong SKU`,
                    );
                    return new Response("subscription_tier_price_mismatch", { status: 500 });
                }
            }

            const rawStatus = obj["status"] as string | undefined;
            // Map Stripe subscription statuses to our internal set.
            const statusMap: Record<string, string> = {
                active: "paid",
                past_due: "past_due",
                canceled: "canceled",
                incomplete: "incomplete",
                incomplete_expired: "canceled",
                trialing: "paid",
                unpaid: "past_due",
                paused: "past_due",
            };
            // FAIL-SAFE (money path): an UNKNOWN/absent Stripe status must NOT
            // coerce to 'paid' (the old `?? "paid"` default was fail-OPEN and
            // could keep a non-paying tenant entitled). Default to 'incomplete',
            // which is a valid tenant_billing CHECK value and does NOT grant
            // access (only 'active'/'trialing' do, via the gate below).
            const billingStatus = (rawStatus && statusMap[rawStatus]) ?? "incomplete";

            const periodEndSecs = obj["current_period_end"] as number | undefined;
            const currentPeriodEndMs = typeof periodEndSecs === "number" ? periodEndSecs * 1000 : null;

            // Resolve the (possibly changed) tier from the subscription price.
            const newTier = resolveSubscriptionTier(obj, env);
            const stripeCustomerId = obj["customer"] as string | undefined;
            const grantsAccess = subscriptionStatusGrantsAccess(rawStatus);

            if (env.BILLING_DB) {
                const db = env.BILLING_DB;
                requiredWrites.push(
                    updateBillingSubscription(db, {
                        stripeSubscriptionId,
                        status: billingStatus,
                        currentPeriodEndMs,
                        nowMs,
                    }),
                );

                // (a) DUNNING-RECOVERY re-activation. A subscription that
                // returns to active/trialing WITHOUT a fresh checkout session
                // (payment-method fix + auto-retry, an operator marking an
                // invoice paid, or an incomplete→trialing resolution) fires this
                // event with grantsAccess=true but NEVER re-runs
                // checkout.session.completed — so the previously-deactivated
                // tier_selections row would strand a paying tenant at
                // subscription_state='inactive' (quota gate denies access). We
                // therefore un-deactivate the EXISTING paid row here.
                //
                // SAFETY: reactivateTierSelectionBySubscription NEVER inserts
                // (single-activation-writer invariant: checkout remains the sole
                // CREATOR) and is guarded on `tenant_billing.status != 'canceled'`
                // so a late, out-of-order updated(active) AFTER a cancel can NOT
                // resurrect a terminated subscription (re-subscribe must go
                // through checkout — mirrors the updateBillingSubscription guard).
                // Keyed by subscription id so we can join tenant_billing for that
                // guard (subscription objects always carry their own id).
                //
                // NOTE: writes flush via Promise.all (unordered), so a combined
                // recovery + price-change leaves the tier propagation below a
                // no-op IF it loses the race with this re-activation (it is gated
                // on an 'active' row) — a benign tier-LABEL staleness on the row,
                // not an access defect; reconciled by the next subscription event
                // (same posture as the F35 race note above).
                if (grantsAccess) {
                    requiredWrites.push(
                        reactivateTierSelectionBySubscription(db, { stripeSubscriptionId, nowMs }),
                    );
                }
                // (b) Propagate an in-place plan change to the entitlement tier.
                // Only when we recognise the price → tier (else leave the
                // existing tier untouched; never guess). Tier propagation is an
                // enrichment (not a safety control), so it stays customer-gated:
                // updateTierSelectionTierByCustomer keys on the customer column.
                if (typeof stripeCustomerId === "string" && stripeCustomerId && newTier) {
                    requiredWrites.push(
                        updateTierSelectionTierByCustomer(db, {
                            stripeCustomerId,
                            tier: newTier,
                        }),
                    );
                }
                // (c) Keep the access gate in sync with the Stripe status. A
                // subscription that drops to past_due/unpaid/paused/canceled here
                // (not just on a separate deleted/payment_failed event) must lose
                // entitlement.
                //
                // FAIL-SAFE: entitlement REVOCATION must NOT depend on the
                // subscription object carrying a `customer` field (it may not).
                // Prefer the customer key when present; otherwise revoke by
                // subscription id (resolved to the tenant via tenant_billing). A
                // non-granting status with neither key would be unreachable, but
                // we always have the subscription id here (guarded above).
                if (!grantsAccess) {
                    requiredWrites.push(
                        typeof stripeCustomerId === "string" && stripeCustomerId
                            ? deactivateTierSelectionByCustomer(db, { stripeCustomerId })
                            : deactivateTierSelectionBySubscription(db, {
                                  stripeSubscriptionId,
                              }),
                    );
                }

                // (d) RUNNER subscription: the SEPARATE runner subscription
                // lifecycle seeds/revokes runners_entitlement. This is the FIRST
                // event that carries the runner PRICE (checkout sessions carry
                // none), so it is where entitlement is actually seeded. The
                // cache-path writes above (a/b/c) are a clean no-op for a runner
                // subscription: updateBillingSubscription/reactivate/deactivate
                // resolve through tenant_billing (which has NO row for the runner
                // subscription id → 0 rows updated), and (b) never fires because
                // the cache resolveSubscriptionTier(runner price) is null. The
                // mismatch assert above is likewise null-null for a runner price
                // → never trips. Runner and cache prices are disjoint env sets,
                // so at most ONE of the two paths ever has work to do.
                const runnerEnt = runnerEntitlementFromSubscriptionPrice(obj, env);
                if (runnerEnt) {
                    // Track the runner subscription status in runner_billing (the
                    // tenant/plan were mapped at checkout; upsert only advances
                    // status). We resolve tenant_id from metadata when present so
                    // an out-of-order .updated seen before .completed still maps.
                    const runnerPlanMeta = runnerTierFromMetadata(obj);
                    if (tenantId && runnerPlanMeta) {
                        requiredWrites.push(
                            upsertRunnerBilling(db, {
                                runnerSubscriptionId: stripeSubscriptionId,
                                tenantId,
                                plan: runnerPlanMeta,
                                status: rawStatus ?? "unknown",
                                stripeCustomerId:
                                    typeof stripeCustomerId === "string" ? stripeCustomerId : null,
                                nowMs,
                            }),
                        );
                    } else {
                        // No tenant/plan metadata on this subscription object
                        // (the common case — checkout mapped the row already):
                        // status-only mirror update, keyed by subscription id.
                        requiredWrites.push(
                            markRunnerBillingStatusBySubscription(db, {
                                runnerSubscriptionId: stripeSubscriptionId,
                                status: rawStatus ?? "unknown",
                                nowMs,
                            }),
                        );
                    }
                    // Seed on a granting status, revoke otherwise. Both resolve
                    // the tenant through runner_billing (subscription id → tenant)
                    // and are idempotent, so a redelivery converges.
                    if (grantsAccess) {
                        requiredWrites.push(
                            upsertRunnersEntitlementBySubscription(db, {
                                runnerSubscriptionId: stripeSubscriptionId,
                                maxConcurrency: runnerEnt.maxConcurrency,
                                maxVcpuH: runnerEnt.maxVcpuH,
                                nowMs,
                            }),
                        );
                    } else {
                        requiredWrites.push(
                            revokeRunnersEntitlementBySubscription(db, {
                                runnerSubscriptionId: stripeSubscriptionId,
                            }),
                        );
                    }
                }
            }

            emitEvent = {
                id: newEventId(),
                event_name: "subscription_updated",
                tenant_id: tenantId,
                properties: {
                    stripe_subscription_id: stripeSubscriptionId,
                    status: billingStatus,
                    current_period_end_ms: currentPeriodEndMs,
                    tier: newTier,
                },
            };
            break;
        }

        case "customer.subscription.created": {
            // Minimal handler: backfill the period-end that
            // checkout.session.completed wrote as null. Pure UPDATE keyed by
            // subscription id — never inserts, never touches the access gate
            // (activation stays the sole responsibility of
            // checkout.session.completed). Idempotent on redelivery.
            const stripeSubscriptionId = obj["id"] as string | undefined;

            // DEFENSE-IN-DEPTH price↔tier assert (audit fix 3, same as the
            // .updated arm): subscription.created is the FIRST payload after
            // checkout that carries the real price, so it is the earliest
            // point a checkout-backend tier↔price desync can be caught.
            {
                const mismatch = detectTierPriceMismatch(obj, env);
                if (mismatch) {
                    console.error(
                        `[stripe-webhook] ${event.type} price↔tier mismatch ` +
                            `(subscription=${stripeSubscriptionId ?? "unknown"}): ` +
                            `metadata[tier]=${mismatch.metaTier} but the subscribed ` +
                            `price maps to ${mismatch.priceTier}; returning 500 for ` +
                            `redelivery rather than entitling the wrong SKU`,
                    );
                    return new Response("subscription_tier_price_mismatch", { status: 500 });
                }
            }

            const periodEndSecs = obj["current_period_end"] as number | undefined;
            if (
                env.BILLING_DB &&
                typeof stripeSubscriptionId === "string" &&
                stripeSubscriptionId &&
                typeof periodEndSecs === "number"
            ) {
                requiredWrites.push(
                    backfillPeriodEnd(env.BILLING_DB, {
                        stripeSubscriptionId,
                        currentPeriodEndMs: periodEndSecs * 1000,
                        nowMs,
                    }),
                );
            }

            // RUNNER subscription seed. subscription.created is the first event
            // to carry the runner PRICE, so — like .updated — it seeds
            // runners_entitlement. The backfillPeriodEnd above is a no-op for a
            // runner subscription (tenant_billing has no runner-sub row). Idempotent
            // with the .updated seed: both upsert the same caps ON CONFLICT.
            if (env.BILLING_DB && typeof stripeSubscriptionId === "string" && stripeSubscriptionId) {
                const db = env.BILLING_DB;
                const runnerEnt = runnerEntitlementFromSubscriptionPrice(obj, env);
                if (runnerEnt) {
                    const rawStatus = obj["status"] as string | undefined;
                    const grantsAccess = subscriptionStatusGrantsAccess(rawStatus);
                    const runnerPlanMeta = runnerTierFromMetadata(obj);
                    const stripeCustomerId = obj["customer"] as string | undefined;
                    // Map/refresh the runner_billing row (tenant/plan from metadata
                    // when present, else a status-only mirror update).
                    if (tenantId && runnerPlanMeta) {
                        requiredWrites.push(
                            upsertRunnerBilling(db, {
                                runnerSubscriptionId: stripeSubscriptionId,
                                tenantId,
                                plan: runnerPlanMeta,
                                status: rawStatus ?? "unknown",
                                stripeCustomerId:
                                    typeof stripeCustomerId === "string" ? stripeCustomerId : null,
                                nowMs,
                            }),
                        );
                    } else {
                        requiredWrites.push(
                            markRunnerBillingStatusBySubscription(db, {
                                runnerSubscriptionId: stripeSubscriptionId,
                                status: rawStatus ?? "unknown",
                                nowMs,
                            }),
                        );
                    }
                    if (grantsAccess) {
                        requiredWrites.push(
                            upsertRunnersEntitlementBySubscription(db, {
                                runnerSubscriptionId: stripeSubscriptionId,
                                maxConcurrency: runnerEnt.maxConcurrency,
                                maxVcpuH: runnerEnt.maxVcpuH,
                                nowMs,
                            }),
                        );
                    } else {
                        requiredWrites.push(
                            revokeRunnersEntitlementBySubscription(db, {
                                runnerSubscriptionId: stripeSubscriptionId,
                            }),
                        );
                    }
                }
            }
            break;
        }

        case "invoice.payment_failed": {
            // BLOCKING money-path gap: a tenant whose RENEWAL payment fails must
            // lose entitlement — but only on the FINAL failure, not a transient
            // first attempt. Stripe drives the subscription to `past_due`/`unpaid`
            // once dunning is exhausted; the invoice carries that status under
            // `subscription_details.subscription` / a top-level `subscription` id
            // and we gate on the invoice's reported subscription status.
            //
            // Fail-safe: if the invoice does NOT signal a terminal
            // past_due/unpaid status (e.g. Stripe will retry), we do NOTHING and
            // leave entitlement intact (a transient first-attempt failure should
            // not revoke access). We never grant access here.
            const stripeSubscriptionId =
                (obj["subscription"] as string | undefined) ??
                (((obj["subscription_details"] as Record<string, unknown> | undefined)?.[
                    "subscription"
                ]) as string | undefined);
            const stripeCustomerId = obj["customer"] as string | undefined;
            const attemptCount = obj["attempt_count"] as number | undefined;
            const nextAttempt = obj["next_payment_attempt"]; // null when Stripe has given up
            // Terminal when Stripe will not retry again. Two signals:
            //   1. `next_payment_attempt` is explicitly null AND present — the
            //      standard subscription-invoice dunning-exhausted signal.
            //   2. invoice `status === 'uncollectible'` — Stripe's definitive
            //      "given up" state, which off-cycle / manual / credit-note
            //      invoices may carry WITHOUT a `next_payment_attempt` key at
            //      all (the field-presence check (1) misses those, leaving a
            //      permanently-failed tenant entitled). Both default to
            //      NON-terminal when absent, preserving the fail-safe posture
            //      for transient first-attempt failures (we never grant here).
            const isTerminalFailure =
                ("next_payment_attempt" in obj && nextAttempt === null) ||
                obj["status"] === "uncollectible";

            if (
                env.BILLING_DB &&
                isTerminalFailure &&
                typeof stripeSubscriptionId === "string" &&
                stripeSubscriptionId
            ) {
                const db = env.BILLING_DB;
                // 1. Secondary mirror: tenant_billing.status = 'past_due'
                //    (status-only — must NOT clobber current_period_end_ms).
                requiredWrites.push(
                    updateBillingStatus(db, {
                        stripeSubscriptionId,
                        status: "past_due",
                        nowMs,
                    }),
                );
                // 2. CANONICAL gate: flip tier_selections away from 'active'.
                //
                // F34 FIX: mirror the pattern used by `customer.subscription.updated`
                // (lines above) and `customer.subscription.deleted`: prefer the
                // customer key when present; fall back to the subscription id
                // (resolved to the tenant via a correlated subquery through
                // `tenant_billing`) when `customer` is absent from the invoice
                // payload. Without this fallback, a terminal `invoice.payment_failed`
                // whose payload omits `customer` leaves `tier_selections.subscription_state`
                // as 'active', granting the tenant indefinite paid-tier access despite
                // a definitive payment failure. We always have `stripeSubscriptionId`
                // at this point (guarded above) so the fallback is always available.
                requiredWrites.push(
                    typeof stripeCustomerId === "string" && stripeCustomerId
                        ? deactivateTierSelectionByCustomer(db, {
                              stripeCustomerId,
                          })
                        : deactivateTierSelectionBySubscription(db, {
                              stripeSubscriptionId,
                          }),
                );

                // 3. RUNNER entitlement revocation. The invoice carries NO price,
                // so we CANNOT tell a runner-sub failure from a cache-sub failure
                // from the payload — this is exactly why runner_billing exists
                // (migration 0087): both writers resolve the subscription id
                // THROUGH runner_billing, so they are inherent no-ops when the id
                // is a cache subscription (the DELETE subquery / the UPDATE match
                // no runner_billing row). We therefore issue them unconditionally:
                // if this WAS a runner subscription, its entitlement is revoked
                // and its billing mirror marked past_due; otherwise, nothing
                // happens. Idempotent on redelivery (DELETE of an absent row).
                requiredWrites.push(
                    revokeRunnersEntitlementBySubscription(db, {
                        runnerSubscriptionId: stripeSubscriptionId,
                    }),
                );
                requiredWrites.push(
                    markRunnerBillingStatusBySubscription(db, {
                        runnerSubscriptionId: stripeSubscriptionId,
                        status: "past_due",
                        nowMs,
                    }),
                );
            }

            emitEvent = {
                id: newEventId(),
                event_name: "invoice_payment_failed",
                tenant_id: tenantId,
                properties: {
                    stripe_subscription_id: stripeSubscriptionId ?? null,
                    terminal: isTerminalFailure,
                    attempt_count: typeof attemptCount === "number" ? attemptCount : null,
                },
            };
            break;
        }

        case "customer.subscription.deleted": {
            const stripeSubscriptionId = obj["id"] as string | undefined;
            if (!stripeSubscriptionId) break;
            const stripeCustomerId = obj["customer"] as string | undefined;

            if (env.BILLING_DB) {
                const db = env.BILLING_DB;
                requiredWrites.push(cancelBilling(db, { stripeSubscriptionId, nowMs }));
                // CANONICAL gate: a canceled subscription must lose access AND be
                // able to re-subscribe. The old handler left
                // tier_selections.subscription_state='active', which both kept
                // the tenant entitled forever AND tripped tier_select.rs:699
                // `AlreadyActive` on any re-subscribe. Flip to 'inactive'.
                //
                // FAIL-SAFE (mirrors subscription.updated FIX-2): a cancel must
                // ALWAYS revoke entitlement, even when the deleted subscription
                // object carries no top-level `customer` field. Prefer the
                // customer key when present; otherwise revoke by subscription id
                // (resolved to the tenant via tenant_billing). We always have the
                // subscription id here (guarded above).
                requiredWrites.push(
                    typeof stripeCustomerId === "string" && stripeCustomerId
                        ? deactivateTierSelectionByCustomer(db, { stripeCustomerId })
                        : deactivateTierSelectionBySubscription(db, {
                              stripeSubscriptionId,
                          }),
                );

                // RUNNER entitlement revocation on cancel. Same disambiguation as
                // invoice.payment_failed: the deleted subscription object may
                // carry no runner-identifying price, so we resolve THROUGH
                // runner_billing (migration 0087). Both writers are inherent
                // no-ops for a cache subscription (no runner_billing row maps its
                // id) and idempotent on redelivery, so we issue them
                // unconditionally: a canceled runner subscription loses its
                // entitlement and its billing mirror is marked 'canceled';
                // a cache cancel is untouched.
                requiredWrites.push(
                    revokeRunnersEntitlementBySubscription(db, {
                        runnerSubscriptionId: stripeSubscriptionId,
                    }),
                );
                requiredWrites.push(
                    markRunnerBillingStatusBySubscription(db, {
                        runnerSubscriptionId: stripeSubscriptionId,
                        status: "canceled",
                        nowMs,
                    }),
                );
            }

            emitEvent = {
                id: newEventId(),
                event_name: "subscription_canceled",
                tenant_id: tenantId,
                properties: {
                    stripe_subscription_id: stripeSubscriptionId,
                    // Real prior plan (metadata[tier] or the price→tier map);
                    // the old hardcoded "starter" mislabeled every
                    // solo/team/pro/max cancel in analytics.
                    from_plan: resolveSubscriptionTier(obj, env) ?? "unknown",
                    to_plan: "free",
                },
            };
            break;
        }

        default:
            // Unhandled type — acknowledge without side effects so Stripe stops
            // retrying. We process only the lifecycle events above.
            break;
    }

    // --- 5. DURABILITY: AWAIT the required entitlement/billing writes (#37). --
    // These are the money path — a failure must NOT be swallowed. We await all of
    // them; if ANY throws (D1 error/throttle), we return a non-2xx (500) WITHOUT
    // claiming the event or emitting, so Stripe redelivers. The redelivery
    // re-runs the same idempotent upserts (and, because nothing was claimed on
    // this failed attempt, it will claim + emit on first success). Returning 200
    // on a failed write is exactly the silent paid-but-no-access bug this fixes.
    try {
        await Promise.all(requiredWrites);
    } catch (e: unknown) {
        console.error(
            `[stripe-webhook] entitlement/billing write failed for ${event.type} ${event.id} — returning 500 for Stripe redelivery: ${(e as Error).message}`,
        );
        return new Response("write_failed", { status: 500 });
    }

    // --- 6. PROCESS-THEN-CLAIM: only AFTER the writes commit, claim event.id. -
    // The claim row therefore exists iff a delivery SUCCEEDED, so the claim
    // outcome makes the non-idempotent analytics emit exactly-once on the first
    // successful delivery:
    //   - first SUCCESSFUL delivery   → claim newly inserted → emit.
    //   - retry after a prior SUCCESS → PK conflict (duplicate) → SKIP emit.
    //   - retry after a prior FAILURE → no prior claim (the failed attempt 500'd
    //                                   before claiming) → claim now + emit.
    // claim_error (the claim INSERT itself threw) is fail-safe → treat as first
    // (emit): the writes already succeeded, so a 200 is correct, and we never
    // drop a genuine first revenue emit. (BILLING_DB is guaranteed bound here —
    // the handler fails closed with 503 at the top when it is missing; the
    // check below is TS narrowing.)
    let isFirstDelivery = true;
    if (event.id && env.BILLING_DB) {
        const claim = await claimWebhookEvent(env.BILLING_DB, {
            eventId: event.id,
            eventType: event.type,
            nowMs,
            correlationId: `stripe_webhook:${event.id}`,
        });
        isFirstDelivery = claim !== "duplicate";
    }

    // --- 7. Emit analytics ONLY on the first successful delivery. The emit is
    // best-effort: it is NOT part of the money-path success contract, so an emit
    // failure must NOT fail the webhook (catch + log; the event stays claimed and
    // we still return 200). The writes succeeding + 200 is the success contract.
    if (emitEvent && isFirstDelivery) {
        ctx.waitUntil(
            emit(env, emitEvent).catch((e: unknown) => {
                console.warn(
                    `[stripe-webhook] analytics emit failed (non-fatal): ${(e as Error).message}`,
                );
            }),
        );
    }

    return new Response("ok", { status: 200 });
}
