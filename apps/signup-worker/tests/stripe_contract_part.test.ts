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

describe("tierFromSubscriptionPrice / detectTierPriceMismatch", () => {
    beforeEach(() => {
        vi.spyOn(globalThis, "fetch").mockResolvedValue({
            ok: true,
            status: 200,
            text: async () => "",
        } as unknown as Response);
    });

    // ------------------------------------------------------------------
    // Reverse-map: given STRIPE_PRICE_ID_<TIER> env + a subscription whose
    // items.data[0].price.id matches, the correct tier is resolved and
    // propagated to tier_selections (no metadata[tier] present — the price
    // alone decides).
    // ------------------------------------------------------------------

    it("reverse-map: STRIPE_PRICE_ID_STARTER env → price match resolves tier 'starter'", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const periodEndSec = Math.floor(nowMs / 1000) + 30 * 24 * 3600;
        const event = {
            id: "evt_price_starter_focused_1",
            type: "customer.subscription.updated",
            data: {
                object: {
                    id: "sub_price_starter_f",
                    customer: "cus_price_starter_f",
                    status: "active",
                    current_period_end: periodEndSec,
                    // No metadata[tier] — tierFromSubscriptionPrice alone resolves it.
                    metadata: { tenant_id: "tenant_price_starter_f" },
                    items: { data: [{ price: { id: "price_starter_xxx" } }] },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        expect((await handleStripeWebhook(req, baseEnv(db), fakeCtx())).status).toBe(200);

        const propagation = db.runCalls.find(
            (c) => c.sql.includes("UPDATE tier_selections") && c.params.includes("starter"),
        );
        expect(propagation).toBeDefined();
        expect(propagation!.params).toContain("cus_price_starter_f");
    });

    it("reverse-map: STRIPE_PRICE_ID_TEAM env → price match resolves tier 'team'", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const periodEndSec = Math.floor(nowMs / 1000) + 30 * 24 * 3600;
        const event = {
            id: "evt_price_team_focused_1",
            type: "customer.subscription.updated",
            data: {
                object: {
                    id: "sub_price_team_f",
                    customer: "cus_price_team_f",
                    status: "active",
                    current_period_end: periodEndSec,
                    metadata: { tenant_id: "tenant_price_team_f" },
                    items: { data: [{ price: { id: "price_team_yyy" } }] },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        expect((await handleStripeWebhook(req, baseEnv(db), fakeCtx())).status).toBe(200);

        const propagation = db.runCalls.find(
            (c) => c.sql.includes("UPDATE tier_selections") && c.params.includes("team"),
        );
        expect(propagation).toBeDefined();
        expect(propagation!.params).toContain("cus_price_team_f");
    });

    it("reverse-map: STRIPE_PRICE_ID_PRO env → price match resolves tier 'pro'", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const periodEndSec = Math.floor(nowMs / 1000) + 30 * 24 * 3600;
        const event = {
            id: "evt_price_pro_focused_1",
            type: "customer.subscription.updated",
            data: {
                object: {
                    id: "sub_price_pro_f",
                    customer: "cus_price_pro_f",
                    status: "active",
                    current_period_end: periodEndSec,
                    metadata: { tenant_id: "tenant_price_pro_f" },
                    items: { data: [{ price: { id: "price_pro_zzz" } }] },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        expect((await handleStripeWebhook(req, baseEnv(db), fakeCtx())).status).toBe(200);

        const propagation = db.runCalls.find(
            (c) => c.sql.includes("UPDATE tier_selections") && c.params.includes("pro"),
        );
        expect(propagation).toBeDefined();
        expect(propagation!.params).toContain("cus_price_pro_f");
    });

    // ------------------------------------------------------------------
    // detectTierPriceMismatch: when metadata[tier] and the price-resolved tier
    // both resolve AND disagree, the handler fails loud (500) and makes no
    // writes — never picking a winner.
    // ------------------------------------------------------------------

    it("detectTierPriceMismatch: metadata[tier]=team but price maps to solo → 500 subscription_tier_price_mismatch, no writes", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_mismatch_team_solo_f",
            type: "customer.subscription.updated",
            data: {
                object: {
                    id: "sub_mismatch_ts_f",
                    customer: "cus_mismatch_ts_f",
                    status: "active",
                    current_period_end: Math.floor(nowMs / 1000) + 30 * 24 * 3600,
                    // metadata says TEAM but the subscribed price resolves to SOLO.
                    metadata: { tenant_id: "tenant_mismatch_ts_f", tier: "team" },
                    items: { data: [{ price: { id: "price_solo_aaa" } }] },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        expect(res.status).toBe(500);
        expect(await res.text()).toBe("subscription_tier_price_mismatch");
        // No billing/tier write — conflict: never pick a winner.
        expect(
            db.runCalls.find(
                (c) => c.sql.includes("tenant_billing") || c.sql.includes("tier_selections"),
            ),
        ).toBeUndefined();
    });

    it("detectTierPriceMismatch: metadata[tier]=team agrees with STRIPE_PRICE_ID_TEAM price → processes normally (no false positive)", async () => {
        // detectTierPriceMismatch returns null when both signals agree —
        // confirm that agreeing team/team subscriptions are not blocked.
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_match_team_f",
            type: "customer.subscription.updated",
            data: {
                object: {
                    id: "sub_match_team_f",
                    customer: "cus_match_team_f",
                    status: "active",
                    current_period_end: Math.floor(nowMs / 1000) + 30 * 24 * 3600,
                    metadata: { tenant_id: "tenant_match_team_f", tier: "team" },
                    items: { data: [{ price: { id: "price_team_yyy" } }] },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        expect(res.status).toBe(200);
        const update = db.runCalls.find((c) => c.sql.includes("UPDATE tenant_billing"));
        expect(update).toBeDefined();
        expect(update!.params).toContain("paid");
        // Tier propagated to the consistent tier.
        const tierUpdate = db.runCalls.find(
            (c) => c.sql.includes("UPDATE tier_selections") && c.sql.includes("SET tier"),
        );
        expect(tierUpdate).toBeDefined();
        expect(tierUpdate!.params).toContain("team");
    });
});
