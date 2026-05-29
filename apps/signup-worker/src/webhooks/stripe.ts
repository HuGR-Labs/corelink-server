// Stripe webhook handler — self-serve Checkout subscription lifecycle.
//
// Stream 2.10 (BILLINGSTEP-WIRE-CHECKOUT): verifies Stripe webhook signature
// then mutates the `tenant_billing` D1 table on the three lifecycle events that
// matter for self-serve billing:
//
//   checkout.session.completed    → INSERT / UPDATE tenant_billing (paid)
//   customer.subscription.updated → UPDATE period_end + status
//   customer.subscription.deleted → UPDATE status = 'canceled'
//
// Each arm also emits an analytics event to the analytics-worker.
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
}

// Minimal D1 interface — keeps unit tests independent of @cloudflare/workers-types.
interface D1PreparedStatement {
    bind(...values: unknown[]): D1PreparedStatement;
    run(): Promise<unknown>;
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

    switch (event.type) {
        case "checkout.session.completed": {
            // Extract Stripe IDs from the session object.
            const stripeCustomerId = obj["customer"] as string | null | undefined;
            const stripeSubscriptionId = obj["subscription"] as string | null | undefined;
            const planMeta = (obj["metadata"] as Record<string, unknown> | undefined)?.["plan"];
            const plan = typeof planMeta === "string" ? planMeta : "starter";
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
                }
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
            const billingStatus = (rawStatus && statusMap[rawStatus]) ?? "paid";

            const periodEndSecs = obj["current_period_end"] as number | undefined;
            const currentPeriodEndMs = typeof periodEndSecs === "number" ? periodEndSecs * 1000 : null;

            if (env.BILLING_DB) {
                ctx.waitUntil(
                    updateBillingSubscription(env.BILLING_DB, {
                        stripeSubscriptionId,
                        status: billingStatus,
                        currentPeriodEndMs,
                        nowMs,
                    }).catch((e: unknown) => {
                        console.warn(`[stripe-webhook] D1 subscription update failed: ${(e as Error).message}`);
                    }),
                );
            }

            ctx.waitUntil(
                emit(env, {
                    id: newEventId(),
                    event_name: "subscription_updated",
                    tenant_id: tenantId,
                    properties: {
                        stripe_subscription_id: stripeSubscriptionId,
                        status: billingStatus,
                        current_period_end_ms: currentPeriodEndMs,
                    },
                }),
            );
            break;
        }

        case "customer.subscription.deleted": {
            const stripeSubscriptionId = obj["id"] as string | undefined;
            if (!stripeSubscriptionId) break;

            if (env.BILLING_DB) {
                ctx.waitUntil(
                    cancelBilling(env.BILLING_DB, { stripeSubscriptionId, nowMs }).catch((e: unknown) => {
                        console.warn(`[stripe-webhook] D1 cancel failed: ${(e as Error).message}`);
                    }),
                );
            }

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
            break;
        }

        default:
            // Unhandled type — acknowledge without side effects so Stripe stops
            // retrying. We process only the three lifecycle events above.
            break;
    }

    return new Response("ok", { status: 200 });
}
