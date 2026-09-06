/**
 * Stripe webhook handler tests — Stream 2.10 (BILLINGSTEP-WIRE-CHECKOUT).
 *
 * Tests the three lifecycle arms:
 *   - Signature verification (valid / invalid / replay / missing header)
 *   - checkout.session.completed → D1 upsert + analytics
 *   - customer.subscription.updated → D1 update + analytics
 *   - customer.subscription.deleted → D1 cancel + analytics
 *   - Unknown event types → 200 with no side effects
 *   - Missing BILLING_DB → 503 fail-closed (Stripe retries; no silent ack)
 *   - payment_status activation gate (unpaid completed → no writes;
 *     async_payment_succeeded → activates; async_payment_failed → no writes)
 *   - price↔tier defense-in-depth assert on subscription.created/updated
 *   - terminal-state guard: a 'canceled' tenant_billing row never resurrects
 */

import { describe, it, expect, vi, beforeEach, type Mock } from "vitest";
import { verifyStripeSignature, handleStripeWebhook } from "../src/webhooks/stripe.js";
import type { StripeWebhookEnv } from "../src/webhooks/stripe.js";

// ---------------------------------------------------------------------------
// Signature verification helpers
// ---------------------------------------------------------------------------

/**
 * Build a valid Stripe-Signature header for the given secret + body.
 * Mirrors the real Stripe algorithm: HMAC-SHA256 over `${ts}.${body}`.
 */
async function buildStripeSignature(
    secret: string,
    body: string,
    timestampSec?: number,
): Promise<string> {
    const ts = timestampSec ?? Math.floor(Date.now() / 1000);
    // Real Stripe scheme: the HMAC key is the FULL `whsec_…` secret string's
    // UTF-8 bytes (prefix included, never base64-decoded). Must mirror the
    // production `decodeWebhookSecret`, else the test signs with a scheme that
    // Stripe never uses and cannot catch a broken verifier (this happened).
    const secretBytes = new TextEncoder().encode(secret);
    const key = await crypto.subtle.importKey(
        "raw",
        secretBytes,
        { name: "HMAC", hash: "SHA-256" },
        false,
        ["sign"],
    );
    const toSign = new TextEncoder().encode(`${ts}.${body}`);
    const sigBytes = new Uint8Array(await crypto.subtle.sign("HMAC", key, toSign));
    const hex = Array.from(sigBytes)
        .map((b) => b.toString(16).padStart(2, "0"))
        .join("");
    return `t=${ts},v1=${hex}`;
}

// Stable test secret (base64 of 32 zero-bytes, prefixed with whsec_).
const TEST_SECRET = "whsec_" + btoa(String.fromCharCode(...new Array(32).fill(0)));

// ---------------------------------------------------------------------------
// verifyStripeSignature unit tests
// ---------------------------------------------------------------------------

describe("handleStripeWebhook", () => {
    it("durability: emit FAILURE is non-fatal → handler still returns 200 (analytics not money-path)", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_emit_fail",
            type: "checkout.session.completed",
            data: {
                object: {
                    customer: "cus_emitfail",
                    subscription: "sub_emitfail",
                    amount_total: 4900,
                    payment_status: "paid",
                    metadata: { tenant_id: "tenant_emitfail", tier: "starter" },
                },
            },
        };
        // The analytics fetch throws — emit must NOT fail the webhook.
        const fetchSpy = vi.mocked(globalThis.fetch);
        fetchSpy.mockRejectedValueOnce(new Error("ANALYTICS_DOWN"));

        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        // Writes succeeded + claim made → 200 despite the emit failure.
        expect(res.status).toBe(200);
        expect(
            db.runCalls.find((c) => c.sql.includes("INSERT INTO tenant_billing")),
        ).toBeDefined();
        // The event stays claimed (a failed emit does not un-claim it).
        expect(
            db.runCalls.find((c) =>
                c.sql.includes("INSERT OR IGNORE INTO stripe_webhook_events_processed"),
            ),
        ).toBeDefined();
    });

    // ------------------------------------------------------------------
    // GAP #1: invoice.payment_failed → revoke entitlement on TERMINAL failure
    // ------------------------------------------------------------------

    it("invoice.payment_failed (terminal: next_payment_attempt=null) → tier_selections deactivated + billing past_due", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_pf_terminal",
            type: "invoice.payment_failed",
            data: {
                object: {
                    subscription: "sub_pf",
                    customer: "cus_pf",
                    attempt_count: 4,
                    next_payment_attempt: null, // Stripe has given up → terminal
                    metadata: { tenant_id: "tenant_pf" },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        expect(res.status).toBe(200);

        // tier_selections flipped away from 'active' (the canonical gate).
        const deact = db.runCalls.find(
            (c) => c.sql.includes("UPDATE tier_selections") && c.sql.includes("'inactive'"),
        );
        expect(deact).toBeDefined();
        // H1: revocation is keyed by the SPECIFIC subscription (via tenant_billing),
        // never by the shared customer.
        expect(deact!.params).toContain("sub_pf");
        expect(deact!.sql).toContain("tenant_billing");

        // tenant_billing status set to past_due (status-only update).
        const billing = db.runCalls.find(
            (c) => c.sql.includes("UPDATE tenant_billing") && c.params.includes("past_due"),
        );
        expect(billing).toBeDefined();
        // Must NOT have written current_period_end_ms (status-only SQL).
        expect(billing!.sql).not.toContain("current_period_end_ms");
    });

    it("invoice.payment_failed (transient: next_payment_attempt set) → NO entitlement change (fail-safe)", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_pf_transient",
            type: "invoice.payment_failed",
            data: {
                object: {
                    subscription: "sub_pf2",
                    customer: "cus_pf2",
                    attempt_count: 1,
                    next_payment_attempt: Math.floor(nowMs / 1000) + 3 * 24 * 3600, // will retry
                    metadata: { tenant_id: "tenant_pf2" },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        expect(res.status).toBe(200);

        // No tier_selections nor tenant_billing mutation on a transient failure.
        expect(db.runCalls.find((c) => c.sql.includes("UPDATE tier_selections"))).toBeUndefined();
        expect(db.runCalls.find((c) => c.sql.includes("UPDATE tenant_billing"))).toBeUndefined();
    });

    it("invoice.payment_failed (uncollectible, NO next_payment_attempt key) → tier_selections deactivated (REV-S5)", async () => {
        // REV-S5: off-cycle / manual / credit-note invoices that Stripe marks
        // `status: 'uncollectible'` may omit `next_payment_attempt` entirely. The
        // old field-presence-only check left such a definitively-failed invoice as
        // non-terminal → tenant kept paid access indefinitely. The status-based
        // fallback now treats `uncollectible` as terminal.
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_pf_uncollectible",
            type: "invoice.payment_failed",
            data: {
                object: {
                    subscription: "sub_pf3",
                    customer: "cus_pf3",
                    attempt_count: 2,
                    status: "uncollectible", // definitively failed; NO next_payment_attempt key
                    metadata: { tenant_id: "tenant_pf3" },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        expect(res.status).toBe(200);

        const deact = db.runCalls.find(
            (c) => c.sql.includes("UPDATE tier_selections") && c.sql.includes("'inactive'"),
        );
        expect(deact).toBeDefined();
        // H1: keyed by subscription id (via tenant_billing), not the shared customer.
        expect(deact!.params).toContain("sub_pf3");
        expect(deact!.sql).toContain("tenant_billing");
        expect(
            db.runCalls.find(
                (c) => c.sql.includes("UPDATE tenant_billing") && c.params.includes("past_due"),
            ),
        ).toBeDefined();
    });

    // ------------------------------------------------------------------
    // REV-S5: dunning RECOVERY — subscription.updated back to active/trialing
    // must RE-ACTIVATE the access gate (no fresh checkout fires)
    // ------------------------------------------------------------------

    it("customer.subscription.updated (recovery: status=active) → tier_selections RE-activated by customer (REV-S5)", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const periodEndSec = Math.floor(nowMs / 1000) + 30 * 24 * 3600;
        const event = {
            id: "evt_sub_recovery",
            type: "customer.subscription.updated",
            data: {
                object: {
                    id: "sub_recovery",
                    status: "active", // dunning recovered (payment method fixed / invoice marked paid)
                    current_period_end: periodEndSec,
                    customer: "cus_recovery",
                    metadata: { tenant_id: "tenant_recovery" },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        expect(res.status).toBe(200);

        // The gate is flipped BACK to 'active' via a bare UPDATE keyed by
        // subscription id, guarded on tenant_billing.status != 'canceled' (never
        // an INSERT, so the single-activation-writer invariant holds).
        const react = db.runCalls.find(
            (c) =>
                c.sql.includes("UPDATE tier_selections") &&
                c.sql.includes("'active'") &&
                !c.sql.includes("'inactive'"),
        );
        expect(react).toBeDefined();
        expect(react!.params).toContain("sub_recovery");
        // Guarded on a non-canceled billing row (terminal-cancel protection).
        expect(react!.sql).toContain("status != 'canceled'");
        // It must NOT be an INSERT/activation upsert (that would create a row).
        expect(react!.sql).not.toContain("INSERT INTO tier_selections");
        // And it must NOT have deactivated (active status grants access).
        expect(
            db.runCalls.find(
                (c) => c.sql.includes("UPDATE tier_selections") && c.sql.includes("'inactive'"),
            ),
        ).toBeUndefined();
    });

    it("customer.subscription.updated (status=past_due) → NO re-activation, gate deactivated (REV-S5 guard)", async () => {
        // The recovery branch must be gated on grantsAccess — a non-granting
        // status must NOT re-activate; it must deactivate.
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_sub_pastdue",
            type: "customer.subscription.updated",
            data: {
                object: {
                    id: "sub_pastdue",
                    status: "past_due",
                    customer: "cus_pastdue",
                    metadata: { tenant_id: "tenant_pastdue" },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        expect(res.status).toBe(200);

        // No re-activation write (would set state='active').
        expect(
            db.runCalls.find(
                (c) =>
                    c.sql.includes("UPDATE tier_selections") &&
                    c.sql.includes("'active'") &&
                    !c.sql.includes("'inactive'"),
            ),
        ).toBeUndefined();
        // Deactivation DID run (past_due does not grant access).
        expect(
            db.runCalls.find(
                (c) => c.sql.includes("UPDATE tier_selections") && c.sql.includes("'inactive'"),
            ),
        ).toBeDefined();
    });

    // ------------------------------------------------------------------
    // GAP #2: customer.subscription.deleted → tier_selections → 'inactive'
    // ------------------------------------------------------------------

    it("customer.subscription.deleted → tier_selections.subscription_state set to 'inactive'", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_del_gate",
            type: "customer.subscription.deleted",
            data: {
                object: {
                    id: "sub_del_gate",
                    customer: "cus_del_gate",
                    metadata: { tenant_id: "tenant_del_gate" },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        expect(res.status).toBe(200);

        // Canceled → tenant_billing.status='canceled' (existing behaviour).
        expect(db.runCalls.find((c) => c.sql.includes("'canceled'"))).toBeDefined();
        // NEW: tier_selections flipped to 'inactive' so access is revoked AND a
        // re-subscribe is allowed (not left at 'active').
        const deact = db.runCalls.find(
            (c) => c.sql.includes("UPDATE tier_selections") && c.sql.includes("'inactive'"),
        );
        expect(deact).toBeDefined();
        // H1: keyed by the SPECIFIC subscription (via tenant_billing), never the
        // shared per-tenant customer.
        expect(deact!.params).toContain("sub_del_gate");
        expect(deact!.sql).toContain("tenant_billing");
    });

    it("customer.subscription.deleted with NO customer field → gate STILL revoked by subscription id (fail-safe)", async () => {
        // #36 FIX-2 (mirror of the subscription.updated fallback): a cancel must
        // always revoke entitlement, even when the deleted subscription object
        // carries no top-level `customer`. Revocation falls back to keying by
        // subscription id (resolved to the tenant via tenant_billing).
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_del_no_customer",
            type: "customer.subscription.deleted",
            data: {
                object: {
                    id: "sub_del_no_customer",
                    // NOTE: no `customer` field — the bug scenario.
                    metadata: { tenant_id: "tenant_del_no_customer" },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        expect(res.status).toBe(200);

        // billing still canceled.
        expect(db.runCalls.find((c) => c.sql.includes("'canceled'"))).toBeDefined();
        // Gate deactivated keyed by subscription id (via tenant_billing subquery),
        // NOT by customer (which is absent).
        const deact = db.runCalls.find(
            (c) =>
                c.sql.includes("UPDATE tier_selections") &&
                c.sql.includes("'inactive'") &&
                c.sql.includes("tenant_billing"),
        );
        expect(deact).toBeDefined();
        expect(deact!.params).toContain("sub_del_no_customer");
    });

    // ------------------------------------------------------------------
    // GAP #3a: subscription.updated unknown status → NOT coerced to 'paid'
    // ------------------------------------------------------------------

    it("customer.subscription.updated with UNKNOWN status → billing not 'paid' (fail-safe) + gate deactivated", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_unknown_status",
            type: "customer.subscription.updated",
            data: {
                object: {
                    id: "sub_unknown",
                    customer: "cus_unknown",
                    status: "some_future_stripe_status",
                    metadata: { tenant_id: "tenant_unknown" },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        expect(res.status).toBe(200);

        const billing = db.runCalls.find((c) => c.sql.includes("UPDATE tenant_billing"));
        expect(billing).toBeDefined();
        // Old fail-open bug coerced unknown → 'paid'. Must NOT do that.
        expect(billing!.params).not.toContain("paid");
        expect(billing!.params).toContain("incomplete");
        // Unknown status does not grant access → gate deactivated.
        expect(
            db.runCalls.find(
                (c) => c.sql.includes("UPDATE tier_selections") && c.sql.includes("'inactive'"),
            ),
        ).toBeDefined();
    });

    // ------------------------------------------------------------------
    // GAP #3b: subscription.updated plan change → propagate new tier
    // ------------------------------------------------------------------

    it("customer.subscription.updated downgrade (price change) → tier_selections.tier updated to new tier", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_downgrade",
            type: "customer.subscription.updated",
            data: {
                object: {
                    id: "sub_downgrade",
                    customer: "cus_downgrade",
                    status: "active",
                    current_period_end: Math.floor(nowMs / 1000) + 30 * 24 * 3600,
                    items: { data: [{ price: { id: "price_starter_xxx" } }] }, // downgraded to starter
                    metadata: { tenant_id: "tenant_downgrade" },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        expect(res.status).toBe(200);

        // tier propagated to tier_selections.tier = 'starter'.
        const tierUpdate = db.runCalls.find(
            (c) => c.sql.includes("UPDATE tier_selections") && c.sql.includes("SET tier"),
        );
        expect(tierUpdate).toBeDefined();
        expect(tierUpdate!.params).toContain("starter");
        expect(tierUpdate!.params).toContain("cus_downgrade");
        // Active status → no deactivation (gate stays as-is).
        expect(
            db.runCalls.find(
                (c) => c.sql.includes("UPDATE tier_selections") && c.sql.includes("'inactive'"),
            ),
        ).toBeUndefined();
    });

    // ------------------------------------------------------------------
    // F35: updateTierSelectionTierByCustomer must not update inactive rows.
    // When a `customer.subscription.updated` event simultaneously cancels AND
    // changes the price (e.g. status='canceled', new price→'max'), both the tier
    // update and the deactivation write run concurrently via Promise.all. Without
    // the `AND subscription_state = 'active'` filter, the tier update writes a
    // stale tier onto the now-inactive row. The filter makes it a no-op.
    // ------------------------------------------------------------------

    it("F35: subscription.updated canceled with a new tier → tier UPDATE SQL carries AND subscription_state='active' filter", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_f35_cancel_with_tier",
            type: "customer.subscription.updated",
            data: {
                object: {
                    id: "sub_f35",
                    customer: "cus_f35",
                    status: "canceled", // non-granting → deactivateTierSelectionBySubscription fires
                    current_period_end: Math.floor(nowMs / 1000) + 30 * 24 * 3600,
                    // Price maps to 'max' → tier update would run (newTier non-null)
                    items: { data: [{ price: { id: "price_max_www" } }] },
                    metadata: { tenant_id: "tenant_f35" },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        expect(res.status).toBe(200);

        // The deactivation write always runs (access gate correctness).
        const deact = db.runCalls.find(
            (c) => c.sql.includes("UPDATE tier_selections") && c.sql.includes("'inactive'"),
        );
        expect(deact).toBeDefined();

        // The tier update SQL MUST carry the active-only filter (F35 fix).
        const tierUpdate = db.runCalls.find(
            (c) => c.sql.includes("UPDATE tier_selections") && c.sql.includes("SET tier"),
        );
        expect(tierUpdate).toBeDefined();
        expect(tierUpdate!.sql).toContain("subscription_state = 'active'");
        expect(tierUpdate!.params).toContain("max");
        expect(tierUpdate!.params).toContain("cus_f35");
    });

    it("customer.subscription.updated with UNKNOWN price → tier left untouched (never guess)", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_unknown_price",
            type: "customer.subscription.updated",
            data: {
                object: {
                    id: "sub_unknown_price",
                    customer: "cus_unknown_price",
                    status: "active",
                    items: { data: [{ price: { id: "price_not_in_our_map" } }] },
                    metadata: { tenant_id: "t_up" },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        expect(res.status).toBe(200);
        // No SET tier write when the price maps to no known tier.
        expect(
            db.runCalls.find((c) => c.sql.includes("UPDATE tier_selections") && c.sql.includes("SET tier")),
        ).toBeUndefined();
    });

    // ------------------------------------------------------------------
    // GAP #4: subscription.created → backfill period-end
    // ------------------------------------------------------------------

    it("customer.subscription.created → backfills tenant_billing.current_period_end_ms", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const periodEndSec = Math.floor(nowMs / 1000) + 30 * 24 * 3600;
        const event = {
            id: "evt_created",
            type: "customer.subscription.created",
            data: {
                object: {
                    id: "sub_created",
                    customer: "cus_created",
                    current_period_end: periodEndSec,
                    metadata: { tenant_id: "tenant_created" },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        expect(res.status).toBe(200);

        const backfill = db.runCalls.find(
            (c) => c.sql.includes("UPDATE tenant_billing") && c.sql.includes("current_period_end_ms"),
        );
        expect(backfill).toBeDefined();
        expect(backfill!.params).toContain(periodEndSec * 1000);
        expect(backfill!.params).toContain("sub_created");
    });

    // ------------------------------------------------------------------
    // FIX 3: the activation gate write itself is asserted (not just
    // tenant_billing). checkout.session.completed must write
    // tier_selections with subscription_state='active' AND a non-null
    // subscription_started_at_ms — both the INSERT arm and (FIX 1) the
    // ON CONFLICT re-activation arm.
    // ------------------------------------------------------------------

    it("checkout.session.completed → tier_selections write sets state='active' AND a non-null subscription_started_at_ms", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_activation_gate",
            type: "checkout.session.completed",
            data: {
                object: {
                    customer: "cus_act",
                    subscription: "sub_act",
                    amount_total: 4900,
                    payment_status: "paid",
                    metadata: { tenant_id: "tenant_act", tier: "starter" },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        expect(res.status).toBe(200);

        const activation = db.runCalls.find(
            (c) =>
                c.sql.includes("INSERT INTO tier_selections") &&
                c.sql.includes("ON CONFLICT"),
        );
        expect(activation).toBeDefined();
        // INSERT arm hardcodes 'active' in the VALUES; the DO UPDATE SET also
        // forces subscription_state = 'active'.
        expect(activation!.sql).toContain("'active'");
        expect(activation!.sql).toContain("subscription_state");
        // The re-activation arm MUST rewrite subscription_started_at_ms (FIX 1):
        // the DO UPDATE SET column appears so a returning customer's NULL'd
        // timestamp is replaced. (Before FIX 1 the UPDATE never touched it.)
        const updateClause = activation!.sql.slice(activation!.sql.indexOf("DO UPDATE"));
        expect(updateClause).toContain("subscription_started_at_ms");
        // A non-null activation timestamp (nowMs-range) was bound.
        const tsParam = activation!.params.find(
            (p) => typeof p === "number" && p >= nowMs - 5000 && p <= nowMs + 5000,
        );
        expect(tsParam).toBeDefined();
        expect(tsParam).not.toBeNull();
    });

    // ------------------------------------------------------------------
    // The free seed row must not block a paid activation.
    //
    // Signup seeds EVERY tenant with tier_selections('free','active'). On the
    // normal path the container flips the row to 'pending_checkout' before
    // Checkout, so the `<> 'active'` guard passes. But this UPSERT deliberately
    // doubles as the fallback for "the container's pending_checkout persist was
    // lost" — and with a bare `<> 'active'` guard that fallback silently does
    // NOTHING for a free tenant (their row is already 'active'): Stripe took the
    // money and the tenant stays on free. The guard must therefore let a paid
    // activation overwrite a FREE active row, while still protecting a PAID one.
    // ------------------------------------------------------------------
    it("checkout.session.completed → activation guard exempts a free/active row (lost-persist fallback)", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_free_upgrade",
            type: "checkout.session.completed",
            data: {
                object: {
                    customer: "cus_free_up",
                    subscription: "sub_free_up",
                    amount_total: 4900,
                    payment_status: "paid",
                    metadata: { tenant_id: "tenant_free_up", tier: "pro" },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        expect(res.status).toBe(200);

        const activation = db.runCalls.find(
            (c) =>
                c.sql.includes("INSERT INTO tier_selections") &&
                c.sql.includes("ON CONFLICT"),
        );
        expect(activation).toBeDefined();
        const whereClause = activation!.sql.slice(activation!.sql.indexOf("DO UPDATE"));
        // Still refuses to clobber an already-active PAID subscription…
        expect(whereClause).toContain("subscription_state <> 'active'");
        // …but a free row is an activation, not a subscription, so it is exempt.
        expect(whereClause).toContain("tier = 'free'");
    });

    // ------------------------------------------------------------------
    // FIX 4: re-subscribe sequence. A prior cancel/payment-failure NULLs
    // subscription_started_at_ms; the SECOND checkout hits the ON CONFLICT
    // re-activation branch, which MUST write a fresh non-null timestamp or
    // the 0039 subscription_started_when_active CHECK throws and the paying
    // re-subscriber is locked out. This asserts the re-activation upsert
    // carries the timestamp rewrite (would fail before FIX 1).
    // ------------------------------------------------------------------

    it("re-subscribe (checkout → subscription.deleted → checkout) → second activation rewrites a non-null subscription_started_at_ms", async () => {
        const db = fakeDb();
        const t0 = Date.now();
        const checkout = (id: string) => ({
            id,
            type: "checkout.session.completed",
            data: {
                object: {
                    customer: "cus_resub",
                    subscription: "sub_resub",
                    amount_total: 4900,
                    payment_status: "paid",
                    metadata: { tenant_id: "tenant_resub", tier: "starter" },
                },
            },
        });

        // 1) Initial checkout → active.
        let req = await makeStripeRequest(checkout("evt_resub_1"), TEST_SECRET, t0);
        expect((await handleStripeWebhook(req, baseEnv(db), fakeCtx())).status).toBe(200);

        // 2) subscription.deleted → deactivate (NULLs subscription_started_at_ms).
        const del = {
            id: "evt_resub_del",
            type: "customer.subscription.deleted",
            data: { object: { id: "sub_resub", customer: "cus_resub", metadata: { tenant_id: "tenant_resub" } } },
        };
        req = await makeStripeRequest(del, TEST_SECRET, t0 + 1000);
        expect((await handleStripeWebhook(req, baseEnv(db), fakeCtx())).status).toBe(200);
        // The deactivation NULLs the timestamp (the condition FIX 1 must survive).
        const deact = db.runCalls.find(
            (c) => c.sql.includes("UPDATE tier_selections") && c.sql.includes("'inactive'"),
        );
        expect(deact).toBeDefined();
        expect(deact!.sql).toContain("subscription_started_at_ms = NULL");

        // 3) Second checkout → ON CONFLICT re-activation.
        // The signature timestamp t1 only authenticates the request; the
        // activation row's subscription_started_at_ms is bound from the
        // handler's OWN Date.now(), not from t1. Anchor the assertion window on
        // the real wall clock spanning the handler call (capture before/after)
        // so it is not fragile w.r.t. the signature timestamp.
        const t1 = t0 + 2000;
        req = await makeStripeRequest(checkout("evt_resub_2"), TEST_SECRET, t1);
        const wallBefore = Date.now();
        expect((await handleStripeWebhook(req, baseEnv(db), fakeCtx())).status).toBe(200);
        const wallAfter = Date.now();

        const activations = db.runCalls.filter(
            (c) => c.sql.includes("INSERT INTO tier_selections") && c.sql.includes("ON CONFLICT"),
        );
        expect(activations).toHaveLength(2);
        const second = activations[1]!;
        // The re-activation UPDATE arm rewrites the timestamp to a fresh,
        // non-null value — satisfying the CHECK after a prior NULLing.
        const updateClause = second.sql.slice(second.sql.indexOf("DO UPDATE"));
        expect(updateClause).toContain("subscription_started_at_ms");
        const tsParam = second.params.find(
            (p) => typeof p === "number" && p >= wallBefore && p <= wallAfter,
        );
        expect(tsParam).toBeDefined();
        expect(tsParam).not.toBeNull();
    });

    // ------------------------------------------------------------------
    // FIX 2: subscription.updated with a non-granting status and NO
    // `customer` field must STILL revoke the access gate (fail-safe) —
    // keyed by subscription id via tenant_billing. The old code nested the
    // deactivation under `if (customer)`, so it fail-OPENed.
    // ------------------------------------------------------------------

    it("customer.subscription.updated canceled with NO customer field → gate deactivated by subscription id", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_no_customer_cancel",
            type: "customer.subscription.updated",
            data: {
                object: {
                    id: "sub_no_customer",
                    status: "canceled",
                    // NOTE: no `customer` field — the bug scenario.
                    metadata: { tenant_id: "tenant_no_customer" },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        expect(res.status).toBe(200);

        // The gate IS deactivated, keyed by the subscription id (resolved via
        // tenant_billing), even though `customer` was absent.
        const deact = db.runCalls.find(
            (c) =>
                c.sql.includes("UPDATE tier_selections") &&
                c.sql.includes("'inactive'") &&
                c.sql.includes("tenant_billing"),
        );
        expect(deact).toBeDefined();
        expect(deact!.params).toContain("sub_no_customer");
    });

    // ------------------------------------------------------------------
    // AUDIT FIX 1: payment_status activation gate. checkout.session.completed
    // fires with payment_status='unpaid' for async payment methods (SEPA,
    // ACH, …) — the old handler activated entitlement without reading it,
    // granting access for money that may never arrive. Activation is now
    // gated on paid/no_payment_required; async payments activate via
    // checkout.session.async_payment_succeeded.
    // ------------------------------------------------------------------

    it("checkout.session.completed with payment_status=unpaid → 200, ZERO writes, no emit (entitlement comes later)", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_unpaid_1",
            type: "checkout.session.completed",
            data: {
                object: {
                    id: "cs_unpaid_1",
                    customer: "cus_unpaid",
                    subscription: "sub_unpaid",
                    amount_total: 4900,
                    payment_status: "unpaid", // async payment method still pending
                    metadata: { tenant_id: "tenant_unpaid", tier: "starter" },
                },
            },
        };
        const fetchSpy = vi.mocked(globalThis.fetch);
        fetchSpy.mockClear();
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        // 200: the event is valid, we just take no action yet — activation
        // arrives via async_payment_succeeded (or never, via …failed).
        expect(res.status).toBe(200);
        // ZERO tenant_billing / tier_selections writes — nothing granted.
        expect(
            db.runCalls.find(
                (c) => c.sql.includes("tenant_billing") || c.sql.includes("tier_selections"),
            ),
        ).toBeUndefined();
        // In fact zero D1 statements at all (we return before the claim too).
        expect(db.runCalls).toHaveLength(0);
        // No paid_subscription_started emit — no MRR started yet.
        expect(fetchSpy).not.toHaveBeenCalled();
    });

    it("checkout.session.completed with payment_status=no_payment_required → activates (trial/100%-coupon checkouts)", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_npr_1",
            type: "checkout.session.completed",
            data: {
                object: {
                    id: "cs_npr_1",
                    customer: "cus_npr",
                    subscription: "sub_npr",
                    amount_total: 0,
                    payment_status: "no_payment_required",
                    metadata: { tenant_id: "tenant_npr", tier: "starter" },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        expect(res.status).toBe(200);
        expect(
            db.runCalls.find((c) => c.sql.includes("INSERT INTO tenant_billing")),
        ).toBeDefined();
        expect(
            db.runCalls.find((c) => c.sql.includes("INSERT INTO tier_selections")),
        ).toBeDefined();
    });

});
