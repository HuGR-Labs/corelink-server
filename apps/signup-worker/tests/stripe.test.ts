/**
 * Stripe webhook handler tests — Stream 2.10 (BILLINGSTEP-WIRE-CHECKOUT).
 *
 * Tests the three lifecycle arms:
 *   - Signature verification (valid / invalid / replay / missing header)
 *   - checkout.session.completed → D1 upsert + analytics
 *   - customer.subscription.updated → D1 update + analytics
 *   - customer.subscription.deleted → D1 cancel + analytics
 *   - Unknown event types → 200 with no side effects
 *   - Missing BILLING_DB → graceful degradation (still 200)
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
    const rawSecret = secret.startsWith("whsec_") ? secret.slice("whsec_".length) : secret;
    const secretBytes = Uint8Array.from(atob(rawSecret), (c) => c.charCodeAt(0));
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

describe("verifyStripeSignature", () => {
    it("returns true for a valid signature at current timestamp", async () => {
        const body = JSON.stringify({ id: "evt_test", type: "checkout.session.completed" });
        const nowMs = Date.now();
        const tsSec = Math.floor(nowMs / 1000);
        const sig = await buildStripeSignature(TEST_SECRET, body, tsSec);
        const result = await verifyStripeSignature(body, sig, TEST_SECRET, nowMs);
        expect(result).toBe(true);
    });

    it("returns false when signature header is null", async () => {
        const body = "{}";
        const result = await verifyStripeSignature(body, null, TEST_SECRET);
        expect(result).toBe(false);
    });

    it("returns false for a tampered body", async () => {
        const body = JSON.stringify({ id: "evt_real" });
        const nowMs = Date.now();
        const tsSec = Math.floor(nowMs / 1000);
        const sig = await buildStripeSignature(TEST_SECRET, body, tsSec);
        const tamperedBody = JSON.stringify({ id: "evt_fake" });
        const result = await verifyStripeSignature(tamperedBody, sig, TEST_SECRET, nowMs);
        expect(result).toBe(false);
    });

    it("returns false for a replay beyond 5-minute window", async () => {
        const body = JSON.stringify({ id: "evt_old" });
        const nowMs = Date.now();
        // Timestamp 6 minutes in the past.
        const staleTsSec = Math.floor(nowMs / 1000) - 6 * 60;
        const sig = await buildStripeSignature(TEST_SECRET, body, staleTsSec);
        const result = await verifyStripeSignature(body, sig, TEST_SECRET, nowMs);
        expect(result).toBe(false);
    });

    it("returns false for a wrong secret", async () => {
        const body = JSON.stringify({ id: "evt_test" });
        const nowMs = Date.now();
        const tsSec = Math.floor(nowMs / 1000);
        const wrongSecret = "whsec_" + btoa(String.fromCharCode(...new Array(32).fill(1)));
        const sig = await buildStripeSignature(wrongSecret, body, tsSec);
        const result = await verifyStripeSignature(body, sig, TEST_SECRET, nowMs);
        expect(result).toBe(false);
    });

    it("returns false for a malformed signature header", async () => {
        const body = "{}";
        const result = await verifyStripeSignature(body, "not_a_valid_sig_header", TEST_SECRET);
        expect(result).toBe(false);
    });

    it("returns false if secret lacks whsec_ prefix", async () => {
        const body = "{}";
        const sig = "t=12345,v1=abc";
        const result = await verifyStripeSignature(body, sig, "bare_secret_no_prefix");
        expect(result).toBe(false);
    });
});

// ---------------------------------------------------------------------------
// handleStripeWebhook integration tests
// ---------------------------------------------------------------------------

/** Minimal ExecutionContext stub. */
function fakeCtx(): ExecutionContext {
    return {
        waitUntil: (p: Promise<unknown>) => { void p; },
        passThroughOnException: () => undefined,
    } as unknown as ExecutionContext;
}

/** Minimal D1 stub — records all bind/run calls. */
function fakeDb(): {
    prepare: Mock;
    runCalls: Array<{ sql: string; params: unknown[] }>;
} {
    const runCalls: Array<{ sql: string; params: unknown[] }> = [];

    const prepare = vi.fn((sql: string) => {
        const params: unknown[] = [];
        const stmt = {
            bind: vi.fn((...args: unknown[]) => {
                params.push(...args);
                return stmt;
            }),
            run: vi.fn(async () => {
                runCalls.push({ sql, params: [...params] });
                return {};
            }),
        };
        return stmt;
    });
    return { prepare, runCalls };
}

/** Build a signed Stripe webhook Request. */
async function makeStripeRequest(
    event: Record<string, unknown>,
    secret: string,
    nowMs?: number,
): Promise<Request> {
    const body = JSON.stringify(event);
    const tsSec = Math.floor((nowMs ?? Date.now()) / 1000);
    const sig = await buildStripeSignature(secret, body, tsSec);
    return new Request("https://signup.corelink.humangr.com/webhooks/stripe", {
        method: "POST",
        headers: {
            "Content-Type": "application/json",
            "stripe-signature": sig,
        },
        body,
    });
}

const baseEnv = (db?: ReturnType<typeof fakeDb>): StripeWebhookEnv => ({
    STRIPE_WEBHOOK_SECRET: TEST_SECRET,
    ANALYTICS_ENDPOINT: "https://analytics.test/ingest",
    ANALYTICS_INGEST_KEY: "test_key",
    // Price→tier reverse map (mirrors the checkout backend's
    // STRIPE_PRICE_ID_{TIER} env vars) so subscription.updated can map a price
    // change back to a tier.
    STRIPE_PRICE_ID_STARTER: "price_starter_xxx",
    STRIPE_PRICE_ID_TEAM: "price_team_yyy",
    STRIPE_PRICE_ID_PRO: "price_pro_zzz",
    BILLING_DB: db
        ? { prepare: db.prepare }
        : undefined,
});

describe("handleStripeWebhook", () => {
    beforeEach(() => {
        vi.spyOn(globalThis, "fetch").mockResolvedValue({
            ok: true,
            status: 200,
            text: async () => "",
        } as unknown as Response);
    });

    it("returns 503 when STRIPE_WEBHOOK_SECRET not configured", async () => {
        const req = new Request("https://x.test/webhooks/stripe", { method: "POST", body: "{}" });
        const res = await handleStripeWebhook(req, {
            ANALYTICS_ENDPOINT: "https://x.test",
        }, fakeCtx());
        expect(res.status).toBe(503);
    });

    it("returns 400 on invalid signature", async () => {
        const req = new Request("https://x.test/webhooks/stripe", {
            method: "POST",
            headers: { "stripe-signature": "t=12345,v1=badhex" },
            body: "{}",
        });
        const res = await handleStripeWebhook(req, baseEnv(), fakeCtx());
        expect(res.status).toBe(400);
        expect(await res.text()).toBe("invalid_signature");
    });

    it("returns 400 with missing stripe-signature header", async () => {
        const req = new Request("https://x.test/webhooks/stripe", { method: "POST", body: "{}" });
        const res = await handleStripeWebhook(req, baseEnv(), fakeCtx());
        expect(res.status).toBe(400);
        expect(await res.text()).toBe("invalid_signature");
    });

    it("checkout.session.completed → D1 upsert + analytics + 200", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_checkout_1",
            type: "checkout.session.completed",
            data: {
                object: {
                    customer: "cus_test123",
                    subscription: "sub_test456",
                    amount_total: 4900,
                    metadata: {
                        tenant_id: "tenant_abc",
                        clerk_user_id: "user_clerk_xyz",
                        plan: "starter",
                    },
                },
            },
        };

        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());

        expect(res.status).toBe(200);
        expect(await res.text()).toBe("ok");

        // D1 upsert should have been called.
        expect(db.prepare).toHaveBeenCalledWith(expect.stringContaining("INSERT INTO tenant_billing"));

        // Analytics fetch should have been called with paid_subscription_started.
        const fetchSpy = vi.mocked(globalThis.fetch);
        const callBody = JSON.parse(fetchSpy.mock.calls[0]![1]?.body as string) as {
            events: Array<{ event_name: string; tenant_id: string }>;
        };
        expect(callBody.events[0]!.event_name).toBe("paid_subscription_started");
        expect(callBody.events[0]!.tenant_id).toBe("tenant_abc");
    });

    it("checkout.session.completed with no BILLING_DB → 200 (graceful degradation)", async () => {
        const nowMs = Date.now();
        const event = {
            id: "evt_nodb_1",
            type: "checkout.session.completed",
            data: {
                object: {
                    customer: "cus_nodb",
                    subscription: "sub_nodb",
                    amount_total: 4900,
                    metadata: { tenant_id: "t_nodb", plan: "starter" },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const env = baseEnv();
        delete (env as { BILLING_DB?: unknown }).BILLING_DB;
        const res = await handleStripeWebhook(req, env, fakeCtx());
        expect(res.status).toBe(200);
    });

    it("checkout.session.completed without tenant_id metadata → 200, no D1 write", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_nometa_1",
            type: "checkout.session.completed",
            data: {
                object: {
                    customer: "cus_nometa",
                    subscription: "sub_nometa",
                    amount_total: 4900,
                    metadata: {},
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        expect(res.status).toBe(200);
        // No D1 write (no tenant_id to key on).
        expect(db.prepare).not.toHaveBeenCalled();
    });

    it("customer.subscription.updated → D1 update + analytics + 200", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const periodEndSec = Math.floor(nowMs / 1000) + 30 * 24 * 3600;
        const event = {
            id: "evt_sub_updated_1",
            type: "customer.subscription.updated",
            data: {
                object: {
                    id: "sub_updated_abc",
                    status: "active",
                    current_period_end: periodEndSec,
                    metadata: { tenant_id: "tenant_xyz" },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());

        expect(res.status).toBe(200);
        expect(db.prepare).toHaveBeenCalledWith(expect.stringContaining("UPDATE tenant_billing"));

        // Verify D1 was called with correct subscription id.
        const updateCall = db.runCalls.find((c) => c.sql.includes("UPDATE tenant_billing"));
        expect(updateCall).toBeDefined();
        expect(updateCall!.params).toContain("sub_updated_abc");
        expect(updateCall!.params).toContain("paid"); // active → paid mapping

        // Analytics.
        const fetchSpy = vi.mocked(globalThis.fetch);
        const body = JSON.parse(fetchSpy.mock.calls[0]![1]?.body as string) as {
            events: Array<{ event_name: string }>;
        };
        expect(body.events[0]!.event_name).toBe("subscription_updated");
    });

    it("customer.subscription.deleted → D1 cancel + analytics + 200", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_sub_deleted_1",
            type: "customer.subscription.deleted",
            data: {
                object: {
                    id: "sub_deleted_xyz",
                    metadata: { tenant_id: "tenant_del" },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());

        expect(res.status).toBe(200);
        expect(db.prepare).toHaveBeenCalledWith(expect.stringContaining("UPDATE tenant_billing"));

        const cancelCall = db.runCalls.find((c) => c.sql.includes("UPDATE tenant_billing"));
        expect(cancelCall).toBeDefined();
        // cancelBilling SQL: SET status = 'canceled' (hardcoded), params are [nowMs, subId].
        expect(cancelCall!.params).toContain("sub_deleted_xyz");
        // Verify the SQL itself hardcodes 'canceled' (not a bound param).
        expect(cancelCall!.sql).toContain("'canceled'");

        const fetchSpy = vi.mocked(globalThis.fetch);
        const body = JSON.parse(fetchSpy.mock.calls[0]![1]?.body as string) as {
            events: Array<{ event_name: string }>;
        };
        expect(body.events[0]!.event_name).toBe("subscription_canceled");
    });

    it("unknown event type → 200 with no D1 or analytics side effects", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_unknown_1",
            type: "invoice.payment_succeeded",
            data: { object: { id: "in_test" } },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const fetchSpy = vi.mocked(globalThis.fetch);
        fetchSpy.mockClear();

        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        expect(res.status).toBe(200);
        expect(db.prepare).not.toHaveBeenCalled();
        expect(fetchSpy).not.toHaveBeenCalled();
    });

    it("idempotency: calling checkout.session.completed twice with same tenant → uses ON CONFLICT upsert (both calls 200)", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_idem_1",
            type: "checkout.session.completed",
            data: {
                object: {
                    customer: "cus_idem",
                    subscription: "sub_idem",
                    amount_total: 4900,
                    metadata: { tenant_id: "tenant_idem", plan: "starter" },
                },
            },
        };

        // First call.
        const req1 = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res1 = await handleStripeWebhook(req1, baseEnv(db), fakeCtx());
        expect(res1.status).toBe(200);

        // Second call (Stripe retry) — same event id, slightly later timestamp.
        const req2 = await makeStripeRequest(event, TEST_SECRET, nowMs + 1000);
        const res2 = await handleStripeWebhook(req2, baseEnv(db), fakeCtx());
        expect(res2.status).toBe(200);

        // Both calls invoke the same ON CONFLICT upsert path.
        const insertCalls = db.runCalls.filter((c) => c.sql.includes("INSERT INTO tenant_billing"));
        expect(insertCalls).toHaveLength(2);
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
        expect(deact!.params).toContain("cus_pf");

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
        expect(deact!.params).toContain("cus_del_gate");
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
        const t1 = t0 + 2000;
        req = await makeStripeRequest(checkout("evt_resub_2"), TEST_SECRET, t1);
        expect((await handleStripeWebhook(req, baseEnv(db), fakeCtx())).status).toBe(200);

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
            (p) => typeof p === "number" && p >= t1 - 5000 && p <= t1 + 5000,
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
});
