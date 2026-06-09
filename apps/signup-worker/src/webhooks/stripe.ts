// Stripe webhook handler — self-serve Checkout subscription lifecycle.
//
// Stream 2.10 (BILLINGSTEP-WIRE-CHECKOUT): verifies Stripe webhook signature
// then mutates the `tenant_billing` D1 table on the three lifecycle events that
// matter for self-serve billing:
//
//   checkout.session.completed    → INSERT / UPDATE tenant_billing (paid) +
//                                   activate tier_selections (access gate)
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
// IDEMPOTENCY NOTE (Stripe redelivers; #34/#36, option (b) re-architecture):
// every D1 mutation here is a safe upsert or a guarded/no-op UPDATE, so a Stripe
// redelivery can ALWAYS re-run them harmlessly. The ONLY non-idempotent side
// effects are the analytics emits (e.g. `paid_subscription_started` MRR), which
// would double-count on a redelivery. We therefore claim `event.id` durably at
// the TOP of the handler (after signature verification, before any mutation/emit)
// against `stripe_webhook_events_processed` (migration 0044) via INSERT OR
// IGNORE on the event_id PRIMARY KEY, and use the claim outcome to gate ONLY the
// emits — NOT the writes. A first delivery (changes=1) processes + emits; a
// redelivery (changes=0, PK conflict) STILL runs the idempotent entitlement/
// billing writes (so a Stripe redelivery safely re-converges the D1 state — no
// entitlement loss) but SKIPS the analytics emit(s) so MRR is not double-counted.
//
// CRITICAL (why we do NOT whole-handler short-circuit on a duplicate): the
// entitlement writes (upsertBillingPaid / activatePaidTierSelection / …) are
// dispatched via ctx.waitUntil and are not awaited before we return 200. If we
// short-circuited the whole handler on a duplicate and a prior delivery's write
// had failed (Stripe got its 200 and will never redeliver again, or redelivers
// and we early-return), the customer would have paid with NO access and no
// recovery path. Gating only the emit keeps every redelivery re-running the
// idempotent writes, which is the recovery path. See `claimWebhookEvent` for the
// claim-then-process fail-safe ordering.
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
    STRIPE_PRICE_ID_STARTER?: string;
    STRIPE_PRICE_ID_TEAM?: string;
    STRIPE_PRICE_ID_PRO?: string;
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
 * Decode a Stripe webhook secret. Stripe secrets are `whsec_<base64>`;
 * we return the raw bytes for HMAC.
 */
function decodeWebhookSecret(secret: string): Uint8Array | null {
    if (!secret.startsWith("whsec_")) return null;
    try {
        const b64 = secret.slice("whsec_".length);
        return Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
    } catch {
        return null;
    }
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
type PaidTier = "starter" | "team" | "pro";

/** Coerce an arbitrary string to a known paid tier, or null. */
function asPaidTier(raw: unknown): PaidTier | null {
    return raw === "starter" || raw === "team" || raw === "pro" ? raw : null;
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
        [env.STRIPE_PRICE_ID_STARTER, "starter"],
        [env.STRIPE_PRICE_ID_TEAM, "team"],
        [env.STRIPE_PRICE_ID_PRO, "pro"],
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
 * Whether a Stripe subscription `status` means the tenant still has entitlement.
 * `active` and `trialing` keep access; everything else (past_due, unpaid,
 * canceled, incomplete, incomplete_expired, paused) loses it. Fail-safe: an
 * unknown/absent status is treated as NOT entitled.
 */
function subscriptionStatusGrantsAccess(rawStatus: unknown): boolean {
    return rawStatus === "active" || rawStatus === "trialing";
}

// ---------------------------------------------------------------------------
// Idempotency claim (#34/#36): dedup on Stripe event.id
// ---------------------------------------------------------------------------

/** Event types this handler actually dispatches side effects for. */
const HANDLED_EVENT_TYPES = new Set<string>([
    "checkout.session.completed",
    "customer.subscription.updated",
    "customer.subscription.created",
    "invoice.payment_failed",
    "customer.subscription.deleted",
]);

/** Outcome of an idempotency claim against `stripe_webhook_events_processed`. */
type ClaimResult =
    | "claimed" // newly inserted — this delivery is the FIRST: process + emit.
    | "duplicate" // PK conflict — already processed: process (idempotent writes), SKIP emit.
    | "claim_error"; // the claim INSERT itself failed (D1 error) — fail-safe → treat as first.

/**
 * Durably claim a Stripe `event.id` BEFORE any mutation/emit (migration 0044
 * `stripe_webhook_events_processed`). The claim outcome gates ONLY the
 * non-idempotent analytics emit — option (b): the idempotent entitlement/billing
 * writes ALWAYS run regardless of the claim result.
 *
 * Uses `INSERT OR IGNORE` on the `event_id` PRIMARY KEY and inspects
 * `meta.changes`:
 *   - changes === 1 → row newly inserted → "claimed" (first delivery: emit).
 *   - changes === 0 → PK conflict → "duplicate" (Stripe retry: SKIP emit).
 *
 * Ordering / fail-safe (claim-THEN-process): we claim first, then dispatch the
 * (already-idempotent) D1 upserts; we emit only when this is the first delivery.
 *
 * We DO NOT whole-handler short-circuit on a duplicate. The D1 writes here are
 * dispatched fire-and-forget (ctx.waitUntil) and not awaited before the 200, so
 * a prior delivery's write could have failed AFTER the claim row committed. If we
 * short-circuited a redelivery, that failed entitlement write would never be
 * retried (Stripe stops at the 200) → customer paid, no access, no recovery. By
 * letting every redelivery re-run the idempotent writes, the redelivery IS the
 * recovery path; we merely gate the emit so MRR is not double-counted.
 *
 * If the claim INSERT itself throws (D1 unavailable), we return "claim_error"
 * and the caller treats it as a FIRST delivery (process + emit): losing a
 * first-time entitlement / revenue event is strictly worse than a possible
 * double analytics count. We never drop a genuine first delivery.
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
             WHERE stripe_subscription_id = ?4`,
        )
        .bind(opts.status, opts.currentPeriodEndMs, opts.nowMs, opts.stripeSubscriptionId)
        .run();
}

/**
 * Update ONLY `tenant_billing.status` (leave period-end + ids untouched), keyed
 * by subscription id. Used on invoice.payment_failed so a dunning failure marks
 * the row 'past_due' without clobbering the existing current_period_end_ms.
 * Idempotent on redelivery.
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
             WHERE stripe_subscription_id = ?3`,
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
 * Propagate an in-place plan change (Stripe price swap on the SAME subscription)
 * to `tier_selections.tier`. checkout.session.completed is the only OTHER writer
 * of `tier`, so without this a tenant who upgrades/downgrades inside Stripe keeps
 * their old entitlement tier forever.
 *
 * Bare UPDATE keyed by customer; only touches the `tier` column so it never
 * resurrects a canceled/past_due subscription_state. Idempotent (writing the
 * same tier twice is a no-op). Never called with an unknown tier (caller gates
 * on a non-null PaidTier — fail-safe).
 */
async function updateTierSelectionTierByCustomer(
    db: D1DatabaseLike,
    opts: { stripeCustomerId: string; tier: PaidTier },
): Promise<void> {
    await db
        .prepare(
            `UPDATE tier_selections
             SET tier = ?1
             WHERE stripe_customer_id = ?2`,
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

    // --- 4. Idempotency claim BEFORE any mutation/emit (#34/#36, option b). -
    // Stripe redelivers; the analytics emits (e.g. paid_subscription_started
    // MRR) are not individually idempotent, so a redelivery would double-count.
    // We claim event.id durably (migration 0044) and use the outcome to gate
    // ONLY the emit. The idempotent entitlement/billing writes ALWAYS run (even
    // on a redelivery) — they are upserts/guarded-UPDATEs, so re-running them is
    // the recovery path for a prior delivery's failed (fire-and-forget) write;
    // we must NOT whole-handler short-circuit a duplicate or a failed
    // entitlement write would be lost forever (Stripe stops at the 200 →
    // customer paid, no access, no recovery). Claim-then-process; see
    // claimWebhookEvent for the fail-safe ordering. Only attempted when a DB is
    // configured — without BILLING_DB there is no durable dedup store (graceful
    // degradation), in which case we treat every delivery as first (emit).
    let isFirstDelivery = true;
    if (event.id && env.BILLING_DB) {
        const claim = await claimWebhookEvent(env.BILLING_DB, {
            eventId: event.id,
            eventType: event.type,
            nowMs,
            correlationId: `stripe_webhook:${event.id}`,
        });
        // "duplicate" → already emitted on a prior delivery: run the idempotent
        // writes again but SKIP the emit (no double MRR). "claimed" → first
        // delivery: emit. "claim_error" → fail-safe: treat as first (emit)
        // rather than risk dropping a genuine first delivery's analytics.
        isFirstDelivery = claim !== "duplicate";
    }

    switch (event.type) {
        case "checkout.session.completed": {
            // Extract Stripe IDs from the session object.
            const stripeCustomerId = obj["customer"] as string | null | undefined;
            const stripeSubscriptionId = obj["subscription"] as string | null | undefined;
            // The checkout session sets metadata[tier] (corelink-stripe-real
            // client.rs:630) — there is NO metadata[plan]. Read the real tier so
            // the billing row + the tier_selections activation record the right
            // plan. (Previously this read [plan], which is always absent, and
            // silently recorded "starter" for every team/pro customer.)
            const tierMeta = (obj["metadata"] as Record<string, unknown> | undefined)?.["tier"];
            const plan = typeof tierMeta === "string" ? tierMeta : "starter";
            // checkout.session does not carry current_period_end — that lives on
            // the subscription object. We leave it null here; it will be filled by
            // the subsequent customer.subscription.created / updated event Stripe
            // fires immediately after. Period-end is non-critical for activation.
            const amountTotal = obj["amount_total"] as number | undefined;
            const clerkUserId = clerkUserIdFromMetadata(obj);
            void clerkUserId; // recorded in metadata for audit, not needed here

            if (tenantId && typeof stripeCustomerId === "string" && stripeCustomerId) {
                if (env.BILLING_DB) {
                    ctx.waitUntil(
                        upsertBillingPaid(env.BILLING_DB, {
                            tenantId,
                            stripeCustomerId,
                            stripeSubscriptionId: typeof stripeSubscriptionId === "string" ? stripeSubscriptionId : null,
                            plan,
                            currentPeriodEndMs: null,
                            nowMs,
                        }).catch((e: unknown) => {
                            console.warn(`[stripe-webhook] D1 upsert failed: ${(e as Error).message}`);
                        }),
                    );
                    // GAP-6: reconcile the canonical subscription FSM the money
                    // path reads. Only a real paid tier may flip to 'active'
                    // (free never reaches Stripe; enterprise uses the inquiry
                    // form). An unrecognised tier is left un-activated rather
                    // than written with a bogus value.
                    const paidTier =
                        plan === "starter" || plan === "team" || plan === "pro" ? plan : null;
                    if (paidTier) {
                        ctx.waitUntil(
                            activatePaidTierSelection(env.BILLING_DB, {
                                tenantId,
                                tier: paidTier,
                                stripeCustomerId,
                                nowMs,
                                correlationId: `stripe_checkout:${(obj["id"] as string | undefined) ?? stripeCustomerId}`,
                            }).catch((e: unknown) => {
                                console.warn(
                                    `[stripe-webhook] tier_selections activation failed: ${(e as Error).message}`,
                                );
                            }),
                        );
                    }
                }
                // Emit ONLY on the first delivery — the writes above ran
                // unconditionally (idempotent), but paid_subscription_started
                // carries non-idempotent MRR that must not double-count on a
                // Stripe redelivery.
                if (isFirstDelivery) {
                    ctx.waitUntil(
                        emit(env, {
                            id: newEventId(),
                            event_name: "paid_subscription_started",
                            tenant_id: tenantId,
                            properties: {
                                plan,
                                mrr_usd: centsToUsd(amountTotal),
                                stripe_customer_id: stripeCustomerId,
                                stripe_subscription_id: stripeSubscriptionId ?? null,
                            },
                        }),
                    );
                }
            }
            break;
        }

        case "customer.subscription.updated": {
            const stripeSubscriptionId = obj["id"] as string | undefined;
            if (!stripeSubscriptionId) break;

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
                ctx.waitUntil(
                    updateBillingSubscription(db, {
                        stripeSubscriptionId,
                        status: billingStatus,
                        currentPeriodEndMs,
                        nowMs,
                    }).catch((e: unknown) => {
                        console.warn(`[stripe-webhook] D1 subscription update failed: ${(e as Error).message}`);
                    }),
                );

                // (a) Propagate an in-place plan change to the entitlement tier.
                // Only when we recognise the price → tier (else leave the
                // existing tier untouched; never guess). Tier propagation is an
                // enrichment (not a safety control), so it stays customer-gated:
                // updateTierSelectionTierByCustomer keys on the customer column.
                if (typeof stripeCustomerId === "string" && stripeCustomerId && newTier) {
                    ctx.waitUntil(
                        updateTierSelectionTierByCustomer(db, {
                            stripeCustomerId,
                            tier: newTier,
                        }).catch((e: unknown) => {
                            console.warn(
                                `[stripe-webhook] tier_selections tier update failed: ${(e as Error).message}`,
                            );
                        }),
                    );
                }
                // (b) Keep the access gate in sync with the Stripe status. A
                // subscription that drops to past_due/unpaid/paused/canceled here
                // (not just on a separate deleted/payment_failed event) must lose
                // entitlement. We do NOT flip back to 'active' on this event —
                // activation only happens via checkout.session.completed (single
                // activation writer).
                //
                // FAIL-SAFE: entitlement REVOCATION must NOT depend on the
                // subscription object carrying a `customer` field (it may not).
                // Prefer the customer key when present; otherwise revoke by
                // subscription id (resolved to the tenant via tenant_billing). A
                // non-granting status with neither key would be unreachable, but
                // we always have the subscription id here (guarded above).
                if (!grantsAccess) {
                    const deactivate =
                        typeof stripeCustomerId === "string" && stripeCustomerId
                            ? deactivateTierSelectionByCustomer(db, { stripeCustomerId })
                            : deactivateTierSelectionBySubscription(db, {
                                  stripeSubscriptionId,
                              });
                    ctx.waitUntil(
                        deactivate.catch((e: unknown) => {
                            console.warn(
                                `[stripe-webhook] tier_selections deactivate failed: ${(e as Error).message}`,
                            );
                        }),
                    );
                }
            }

            // Emit only on first delivery (writes above ran unconditionally).
            if (isFirstDelivery) {
                ctx.waitUntil(
                    emit(env, {
                        id: newEventId(),
                        event_name: "subscription_updated",
                        tenant_id: tenantId,
                        properties: {
                            stripe_subscription_id: stripeSubscriptionId,
                            status: billingStatus,
                            current_period_end_ms: currentPeriodEndMs,
                            tier: newTier,
                        },
                    }),
                );
            }
            break;
        }

        case "customer.subscription.created": {
            // Minimal handler: backfill the period-end that
            // checkout.session.completed wrote as null. Pure UPDATE keyed by
            // subscription id — never inserts, never touches the access gate
            // (activation stays the sole responsibility of
            // checkout.session.completed). Idempotent on redelivery.
            const stripeSubscriptionId = obj["id"] as string | undefined;
            const periodEndSecs = obj["current_period_end"] as number | undefined;
            if (
                env.BILLING_DB &&
                typeof stripeSubscriptionId === "string" &&
                stripeSubscriptionId &&
                typeof periodEndSecs === "number"
            ) {
                ctx.waitUntil(
                    backfillPeriodEnd(env.BILLING_DB, {
                        stripeSubscriptionId,
                        currentPeriodEndMs: periodEndSecs * 1000,
                        nowMs,
                    }).catch((e: unknown) => {
                        console.warn(`[stripe-webhook] period-end backfill failed: ${(e as Error).message}`);
                    }),
                );
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
            // Terminal when Stripe will not retry again (next_payment_attempt is
            // explicitly null and present in the payload).
            const isTerminalFailure =
                "next_payment_attempt" in obj && nextAttempt === null;

            if (
                env.BILLING_DB &&
                isTerminalFailure &&
                typeof stripeSubscriptionId === "string" &&
                stripeSubscriptionId
            ) {
                const db = env.BILLING_DB;
                // 1. Secondary mirror: tenant_billing.status = 'past_due'
                //    (status-only — must NOT clobber current_period_end_ms).
                ctx.waitUntil(
                    updateBillingStatus(db, {
                        stripeSubscriptionId,
                        status: "past_due",
                        nowMs,
                    }).catch((e: unknown) => {
                        console.warn(`[stripe-webhook] payment_failed billing update failed: ${(e as Error).message}`);
                    }),
                );
                // 2. CANONICAL gate: flip tier_selections away from 'active'.
                if (typeof stripeCustomerId === "string" && stripeCustomerId) {
                    ctx.waitUntil(
                        deactivateTierSelectionByCustomer(db, {
                            stripeCustomerId,
                        }).catch((e: unknown) => {
                            console.warn(
                                `[stripe-webhook] payment_failed deactivate failed: ${(e as Error).message}`,
                            );
                        }),
                    );
                }
            }

            // Emit only on first delivery (writes above ran unconditionally).
            if (isFirstDelivery) {
                ctx.waitUntil(
                    emit(env, {
                        id: newEventId(),
                        event_name: "invoice_payment_failed",
                        tenant_id: tenantId,
                        properties: {
                            stripe_subscription_id: stripeSubscriptionId ?? null,
                            terminal: isTerminalFailure,
                            attempt_count: typeof attemptCount === "number" ? attemptCount : null,
                        },
                    }),
                );
            }
            break;
        }

        case "customer.subscription.deleted": {
            const stripeSubscriptionId = obj["id"] as string | undefined;
            if (!stripeSubscriptionId) break;
            const stripeCustomerId = obj["customer"] as string | undefined;

            if (env.BILLING_DB) {
                const db = env.BILLING_DB;
                ctx.waitUntil(
                    cancelBilling(db, { stripeSubscriptionId, nowMs }).catch((e: unknown) => {
                        console.warn(`[stripe-webhook] D1 cancel failed: ${(e as Error).message}`);
                    }),
                );
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
                const deactivate =
                    typeof stripeCustomerId === "string" && stripeCustomerId
                        ? deactivateTierSelectionByCustomer(db, { stripeCustomerId })
                        : deactivateTierSelectionBySubscription(db, {
                              stripeSubscriptionId,
                          });
                ctx.waitUntil(
                    deactivate.catch((e: unknown) => {
                        console.warn(
                            `[stripe-webhook] cancel deactivate failed: ${(e as Error).message}`,
                        );
                    }),
                );
            }

            // Emit only on first delivery (writes above ran unconditionally).
            if (isFirstDelivery) {
                ctx.waitUntil(
                    emit(env, {
                        id: newEventId(),
                        event_name: "subscription_canceled",
                        tenant_id: tenantId,
                        properties: {
                            stripe_subscription_id: stripeSubscriptionId,
                            from_plan: "starter",
                            to_plan: "free",
                        },
                    }),
                );
            }
            break;
        }

        default:
            // Unhandled type — acknowledge without side effects so Stripe stops
            // retrying. We process only the three lifecycle events above.
            break;
    }

    return new Response("ok", { status: 200 });
}
