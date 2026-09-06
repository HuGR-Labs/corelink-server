/** Stripe webhook adapter: verify raw signatures, then apply idempotent billing writes. */

import { verifyStripeSignature } from "./stripe_signature.js";
export { verifyStripeSignature } from "./stripe_signature.js";
import type { AnalyticsEmitEnv } from "../lib/analytics-server";
import { emit, newEventId } from "../lib/analytics-server";
// SINGLE SOURCE OF TRUTH for the dispatched event set (see handled-stripe-events.json).
// The SAME file drives the live Stripe endpoint's enabled_events via
// scripts/ops/stripe-reconcile-webhook-events.sh, so the runtime allowlist and the
// dashboard subscription can never drift. Edit the JSON, not a literal here.
import handledStripeEvents from "./handled-stripe-events.json";
import {
  centsToUsd,
  checkoutCreatedAtMs,
  clerkUserIdFromMetadata,
  detectTierPriceMismatch,
  resolveSubscriptionTier,
  runnerEntitlementFromSubscriptionPrice,
  runnerTierFromMetadata,
  subscriptionStatusGrantsAccess,
  tenantIdFromMetadata,
  tierFromMetadata,
} from "./stripe_contract.js";
import {
  activatePaidTierSelection,
  backfillPeriodEnd,
  cancelBilling,
  claimWebhookEvent,
  deactivateTierSelectionBySubscription,
  expirePendingCheckout,
  markRunnerBillingStatusBySubscription,
  queueCheckoutActivation,
  reactivateTierSelectionBySubscription,
  revokeRunnersEntitlementBySubscription,
  updateBillingStatus,
  updateBillingSubscription,
  updateTierSelectionTierByCustomer,
  upsertBillingPaid,
  upsertRunnerBilling,
  upsertRunnersEntitlementBySubscription,
  upsertRunnersEntitlementByTenant,
} from "./stripe_persistence.js";
import type { D1DatabaseLike } from "./billing_checkout";

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
// Metadata helpers
// ---------------------------------------------------------------------------

/** Extract tenant_id from Stripe metadata. Returns null if absent. */
export const HANDLED_EVENT_TYPES = new Set<string>(handledStripeEvents.enabled_events);

/** Outcome of an idempotency claim against `stripe_webhook_events_processed`. */
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
    const billingDb = env.BILLING_DB;

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
    const requiredWrites: Array<() => Promise<void>> = [];
    let emitEvent: Parameters<typeof emit>[1] | null = null;

    switch (event.type) {
        case "checkout.session.expired": {
            const sessionId = obj["id"];
            if (!tenantId || typeof sessionId !== "string" || !sessionId) {
                return new Response("checkout_expired_missing_identity", { status: 500 });
            }
            // Runner checkouts never occupy the cache ownership ledger; their
            // dedicated runner webhook path has no cache row to release.
            const expiredTier = tierFromMetadata(obj);
            if (!expiredTier) break;
            requiredWrites.push(() => expirePendingCheckout(billingDb, {
                tenantId,
                tier: expiredTier,
                sessionId,
                stripeCustomerId:
                    typeof obj["customer"] === "string" ? obj["customer"] : "",
                checkoutCreatedAtMs: checkoutCreatedAtMs(obj),
            }));
            break;
        }
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
                requiredWrites.push(() => upsertRunnerBilling(billingDb, {
                        runnerSubscriptionId: stripeSubscriptionId,
                        tenantId,
                        plan: runnerTier,
                        status: "active",
                        stripeCustomerId,
                        nowMs,
                    }));
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
                checkoutCreatedAtMs: checkoutCreatedAtMs(obj),
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
                requiredWrites.push(() => updateBillingSubscription(db, {
                        stripeSubscriptionId,
                        status: billingStatus,
                        currentPeriodEndMs,
                        nowMs,
                    }));

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
                // The queue flushes in declaration order, so recovery precedes
                // any dependent tier-label propagation without an unordered
                // Promise.all race.
                if (grantsAccess) {
                    requiredWrites.push(() =>
                        reactivateTierSelectionBySubscription(db, { stripeSubscriptionId, nowMs }),
                    );
                }
                // (b) Propagate an in-place plan change to the entitlement tier.
                // Only when we recognise the price → tier (else leave the
                // existing tier untouched; never guess). Tier propagation is an
                // enrichment (not a safety control), so it stays customer-gated:
                // updateTierSelectionTierByCustomer keys on the customer column.
                if (typeof stripeCustomerId === "string" && stripeCustomerId && newTier) {
                    requiredWrites.push(() => updateTierSelectionTierByCustomer(db, {
                            stripeCustomerId,
                            tier: newTier,
                        }));
                }
                // (c) Keep the access gate in sync with the Stripe status. A
                // subscription that drops to past_due/unpaid/paused/canceled here
                // (not just on a separate deleted/payment_failed event) must lose
                // entitlement.
                //
                // H1 FIX (money/GDPR): revoke ONLY the tier_selection tied to THIS
                // subscription — resolved to the tenant via `tenant_billing`, the
                // one-row-per-tenant CACHE billing map. The old code preferred a
                // customer-keyed revoke, which flipped EVERY tier_selections row
                // sharing this tenant's SINGLE Stripe customer (checkout reuses one
                // `cus_…` per tenant across the cache AND runner subscriptions). A
                // RUNNER subscription dropping to a non-granting status therefore
                // silently revoked the tenant's ACTIVE cache tier. `tenant_billing`
                // never holds a runner subscription id (runner subs live in
                // `runner_billing`), so keying by subscription id makes a runner-sub
                // event a clean no-op on the cache row while a cache-sub event still
                // revokes the cache tier. The subscription id is always present
                // (guarded above) and is the strictly-more-robust key — the customer
                // field is the one a subscription payload may omit.
                if (!grantsAccess) {
                    requiredWrites.push(() => deactivateTierSelectionBySubscription(db, {
                            stripeSubscriptionId,
                        }));
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
                        requiredWrites.push(() => upsertRunnerBilling(db, {
                                runnerSubscriptionId: stripeSubscriptionId,
                                tenantId,
                                plan: runnerPlanMeta,
                                status: rawStatus ?? "unknown",
                                stripeCustomerId:
                                    typeof stripeCustomerId === "string" ? stripeCustomerId : null,
                                nowMs,
                            }));
                    } else {
                        // No tenant/plan metadata on this subscription object
                        // (the common case — checkout mapped the row already):
                        // status-only mirror update, keyed by subscription id.
                        requiredWrites.push(() => markRunnerBillingStatusBySubscription(db, {
                                runnerSubscriptionId: stripeSubscriptionId,
                                status: rawStatus ?? "unknown",
                                nowMs,
                            }));
                    }
                    // Seed on a granting status, revoke otherwise. Both resolve
                    // the tenant through runner_billing (subscription id → tenant)
                    // and are idempotent, so a redelivery converges.
                    if (grantsAccess) {
                        // Seed by tenant id DIRECTLY when we hold it (from the
                        // subscription metadata) — race-free. The subscription-
                        // correlated variant `SELECT`s FROM `runner_billing`, which
                        // is now executed before the next queued write, so it
                        // cannot lose the race and silently
                        // seed 0 rows — a paying customer with no capacity. Fall
                        // back to correlation only on the no-metadata path, where
                        // `runner_billing` was mapped by a prior event.
                        requiredWrites.push(() =>
                            tenantId
                                ? upsertRunnersEntitlementByTenant(db, {
                                      tenantId,
                                      maxConcurrency: runnerEnt.maxConcurrency,
                                      maxVcpuH: runnerEnt.maxVcpuH,
                                      nowMs,
                                  })
                                : upsertRunnersEntitlementBySubscription(db, {
                                      runnerSubscriptionId: stripeSubscriptionId,
                                      maxConcurrency: runnerEnt.maxConcurrency,
                                      maxVcpuH: runnerEnt.maxVcpuH,
                                      nowMs,
                                  }),
                        );
                    } else {
                        requiredWrites.push(() => revokeRunnersEntitlementBySubscription(db, {
                                runnerSubscriptionId: stripeSubscriptionId,
                            }));
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
                const db = env.BILLING_DB;
                requiredWrites.push(() => backfillPeriodEnd(db, {
                        stripeSubscriptionId,
                        currentPeriodEndMs: periodEndSecs * 1000,
                        nowMs,
                    }));
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
                        requiredWrites.push(() => upsertRunnerBilling(db, {
                                runnerSubscriptionId: stripeSubscriptionId,
                                tenantId,
                                plan: runnerPlanMeta,
                                status: rawStatus ?? "unknown",
                                stripeCustomerId:
                                    typeof stripeCustomerId === "string" ? stripeCustomerId : null,
                                nowMs,
                            }));
                    } else {
                        requiredWrites.push(() => markRunnerBillingStatusBySubscription(db, {
                                runnerSubscriptionId: stripeSubscriptionId,
                                status: rawStatus ?? "unknown",
                                nowMs,
                            }));
                    }
                    if (grantsAccess) {
                        // Seed by tenant id DIRECTLY when we hold it (from the
                        // subscription metadata) — race-free. The subscription-
                        // correlated variant `SELECT`s FROM `runner_billing`, which
                        // is now executed before the next queued write, so it
                        // cannot lose the race and silently
                        // seed 0 rows — a paying customer with no capacity. Fall
                        // back to correlation only on the no-metadata path, where
                        // `runner_billing` was mapped by a prior event.
                        requiredWrites.push(() =>
                            tenantId
                                ? upsertRunnersEntitlementByTenant(db, {
                                      tenantId,
                                      maxConcurrency: runnerEnt.maxConcurrency,
                                      maxVcpuH: runnerEnt.maxVcpuH,
                                      nowMs,
                                  })
                                : upsertRunnersEntitlementBySubscription(db, {
                                      runnerSubscriptionId: stripeSubscriptionId,
                                      maxConcurrency: runnerEnt.maxConcurrency,
                                      maxVcpuH: runnerEnt.maxVcpuH,
                                      nowMs,
                                  }),
                        );
                    } else {
                        requiredWrites.push(() => revokeRunnersEntitlementBySubscription(db, {
                                runnerSubscriptionId: stripeSubscriptionId,
                            }));
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
                requiredWrites.push(() => updateBillingStatus(db, {
                        stripeSubscriptionId,
                        status: "past_due",
                        nowMs,
                    }));
                // 2. CANONICAL gate: flip tier_selections away from 'active'.
                //
                // H1 FIX (money/GDPR): revoke ONLY the tier_selection tied to THIS
                // subscription — resolved to the tenant via a correlated subquery
                // through `tenant_billing` (the one-row-per-tenant CACHE billing
                // map). The old code preferred a customer-keyed revoke, which flipped
                // EVERY tier_selections row sharing this tenant's SINGLE Stripe
                // customer (checkout reuses one `cus_…` per tenant across the cache
                // AND runner subscriptions). A terminal `invoice.payment_failed` on a
                // RUNNER subscription therefore silently revoked the tenant's ACTIVE
                // cache tier. `tenant_billing` never holds a runner subscription id
                // (runner subs live in `runner_billing`, revoked separately below),
                // so keying by subscription id makes a runner-sub failure a clean
                // no-op on the cache row while a cache-sub failure still revokes the
                // cache tier. We always have `stripeSubscriptionId` here (guarded
                // above); the invoice payload's `customer` field, by contrast, may be
                // absent — so the subscription key is both more precise and more
                // robust.
                requiredWrites.push(() => deactivateTierSelectionBySubscription(db, {
                        stripeSubscriptionId,
                    }));

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
                requiredWrites.push(() => revokeRunnersEntitlementBySubscription(db, {
                        runnerSubscriptionId: stripeSubscriptionId,
                    }));
                requiredWrites.push(() => markRunnerBillingStatusBySubscription(db, {
                        runnerSubscriptionId: stripeSubscriptionId,
                        status: "past_due",
                        nowMs,
                    }));
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

            if (env.BILLING_DB) {
                const db = env.BILLING_DB;
                requiredWrites.push(() => cancelBilling(db, { stripeSubscriptionId, nowMs }));
                // CANONICAL gate: a canceled subscription must lose access AND be
                // able to re-subscribe. The old handler left
                // tier_selections.subscription_state='active', which both kept
                // the tenant entitled forever AND tripped tier_select.rs:699
                // `AlreadyActive` on any re-subscribe. Flip to 'inactive'.
                //
                // H1 FIX (money/GDPR): revoke ONLY the tier_selection tied to THIS
                // subscription — resolved to the tenant via `tenant_billing` (the
                // one-row-per-tenant CACHE billing map). The old code preferred a
                // customer-keyed revoke, which flipped EVERY tier_selections row
                // sharing this tenant's SINGLE Stripe customer (checkout reuses one
                // `cus_…` per tenant across the cache AND runner subscriptions), so
                // cancelling an unrelated RUNNER add-on silently revoked the tenant's
                // ACTIVE cache tier — a paying cache customer losing access. A runner
                // subscription id is never in `tenant_billing` (runner subs live in
                // `runner_billing`, revoked separately below), so keying by
                // subscription id makes a runner-sub cancel a clean no-op on the
                // cache row while a cache-sub cancel still revokes the cache tier. The
                // subscription id is always present (guarded above) and is the
                // strictly-more-robust key — the deleted subscription object may
                // carry no top-level `customer` field.
                requiredWrites.push(() => deactivateTierSelectionBySubscription(db, {
                        stripeSubscriptionId,
                    }));

                // RUNNER entitlement revocation on cancel. Same disambiguation as
                // invoice.payment_failed: the deleted subscription object may
                // carry no runner-identifying price, so we resolve THROUGH
                // runner_billing (migration 0087). Both writers are inherent
                // no-ops for a cache subscription (no runner_billing row maps its
                // id) and idempotent on redelivery, so we issue them
                // unconditionally: a canceled runner subscription loses its
                // entitlement and its billing mirror is marked 'canceled';
                // a cache cancel is untouched.
                requiredWrites.push(() => revokeRunnersEntitlementBySubscription(db, {
                        runnerSubscriptionId: stripeSubscriptionId,
                    }));
                requiredWrites.push(() => markRunnerBillingStatusBySubscription(db, {
                        runnerSubscriptionId: stripeSubscriptionId,
                        status: "canceled",
                        nowMs,
                    }));
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
    // These are the money path — a failure must NOT be swallowed. We execute
    // them in declaration order; billing ownership therefore commits before its
    // dependent tier activation, and runner mapping commits before entitlement.
    // If ANY throws (D1 error/throttle), we return a non-2xx (500) WITHOUT
    // claiming the event or emitting, so Stripe redelivers. The redelivery
    // re-runs the same idempotent upserts (and, because nothing was claimed on
    // this failed attempt, it will claim + emit on first success). Returning 200
    // on a failed write is exactly the silent paid-but-no-access bug this fixes.
    try {
        for (const write of requiredWrites) {
            await write();
        }
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
