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
});
