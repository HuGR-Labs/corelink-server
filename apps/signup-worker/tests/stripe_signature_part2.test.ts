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

describe("verifyStripeSignature", () => {
    it("returns true for a valid signature at current timestamp", async () => {
        const body = JSON.stringify({ id: "evt_test", type: "checkout.session.completed" });
        const nowMs = Date.now();
        const tsSec = Math.floor(nowMs / 1000);
        const sig = await buildStripeSignature(TEST_SECRET, body, tsSec);
        const result = await verifyStripeSignature(body, sig, TEST_SECRET, nowMs);
        expect(result).toBe(true);
    });

    it("accepts a real Stripe-generated signature (KAT) — guards the whsec key derivation", async () => {
        // Ground-truth vector from stripe-node's
        // `Stripe.webhooks.generateTestHeaderString({ payload, secret, timestamp })`.
        // Stripe's HMAC key is the ENTIRE `whsec_…` string (never base64-decoded).
        // The previous base64-decode-the-remainder derivation produced
        // 7e4ef99…653d647 for this exact vector — i.e. it would REJECT every real
        // Stripe webhook, so a paying customer would receive no entitlement.
        const secret = "whsec_Y29yZWxpbmsta2F0LXNlY3JldC1tYXRlcmlhbA==";
        const payload = '{"id":"evt_kat","type":"checkout.session.completed"}';
        const timestamp = 1_700_000_000;
        const header =
            `t=${timestamp},v1=0d59e9692961c0dd6d7912b8a3e3fa09d15bb0077167a3f113b09cc7d963c27d`;
        const result = await verifyStripeSignature(payload, header, secret, timestamp * 1000);
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

/**
 * Minimal D1 stub — records all bind/run calls.
 *
 * Models the `stripe_webhook_events_processed` idempotency table's `INSERT OR
 * IGNORE` conflict behavior on the `event_id` PRIMARY KEY: the first claim for
 * a given event_id reports `meta.changes = 1` (row inserted), a redelivery of
 * the same event_id reports `meta.changes = 0` (PK conflict ignored). Every
 * other statement reports `meta.changes = 1` (nothing in the handler reads it).
 */
function fakeDb(): {
    prepare: Mock;
    runCalls: Array<{ sql: string; params: unknown[] }>;
} {
    const runCalls: Array<{ sql: string; params: unknown[] }> = [];
    // Durable mirror of the dedup PK: event_ids already claimed.
    const claimedEventIds = new Set<string>();

    const prepare = vi.fn((sql: string) => {
        const params: unknown[] = [];
        const stmt = {
            bind: vi.fn((...args: unknown[]) => {
                params.push(...args);
                return stmt;
            }),
            run: vi.fn(async () => {
                runCalls.push({ sql, params: [...params] });
                let changes = 1;
                if (sql.includes("INSERT OR IGNORE INTO stripe_webhook_events_processed")) {
                    // event_id is bound as ?1 (first param).
                    const eventId = params[0] as string;
                    if (claimedEventIds.has(eventId)) {
                        changes = 0; // PK conflict → IGNOREd.
                    } else {
                        claimedEventIds.add(eventId);
                        changes = 1;
                    }
                }
                return { meta: { changes } };
            }),
        };
        return stmt;
    });
    return { prepare, runCalls };
}

/**
 * Variant fake DB whose idempotency claim INSERT always THROWS (D1 error),
 * exercising the fail-safe claim-then-process path: the handler must proceed to
 * dispatch rather than drop a possible first-time event. All other statements
 * behave normally.
 */
function fakeDbClaimThrows(): {
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
                if (sql.includes("INSERT OR IGNORE INTO stripe_webhook_events_processed")) {
                    throw new Error("D1_UNAVAILABLE");
                }
                runCalls.push({ sql, params: [...params] });
                return { meta: { changes: 1 } };
            }),
        };
        return stmt;
    });
    return { prepare, runCalls };
}

/**
 * Variant fake DB that lets a CHOSEN entitlement/billing write throw, to exercise
 * the #37 durability path (await writes → 500 on failure). `failWhile()` returns
 * true for a `run()` whose SQL should throw; flipping the closed-over flag lets a
 * test fail the FIRST delivery's write then succeed on the (Stripe) redelivery.
 * The idempotency claim INSERT models PK-conflict dedup exactly like `fakeDb`, so
 * a redelivery after a prior SUCCESS hits the conflict (and a redelivery after a
 * prior FAILURE — where no claim was ever written — does not).
 */
function fakeDbFailableWrite(failWhile: (sql: string) => boolean): {
    prepare: Mock;
    runCalls: Array<{ sql: string; params: unknown[] }>;
} {
    const runCalls: Array<{ sql: string; params: unknown[] }> = [];
    const claimedEventIds = new Set<string>();
    const prepare = vi.fn((sql: string) => {
        const params: unknown[] = [];
        const stmt = {
            bind: vi.fn((...args: unknown[]) => {
                params.push(...args);
                return stmt;
            }),
            run: vi.fn(async () => {
                // A targeted entitlement/billing write throws (D1 error) — the
                // call is NOT recorded as a successful runCall.
                if (
                    !sql.includes("INSERT OR IGNORE INTO stripe_webhook_events_processed") &&
                    failWhile(sql)
                ) {
                    throw new Error("D1_WRITE_FAILED");
                }
                runCalls.push({ sql, params: [...params] });
                let changes = 1;
                if (sql.includes("INSERT OR IGNORE INTO stripe_webhook_events_processed")) {
                    const eventId = params[0] as string;
                    if (claimedEventIds.has(eventId)) changes = 0;
                    else {
                        claimedEventIds.add(eventId);
                        changes = 1;
                    }
                }
                return { meta: { changes } };
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
    STRIPE_PRICE_ID_SOLO: "price_solo_aaa",
    STRIPE_PRICE_ID_STARTER: "price_starter_xxx",
    STRIPE_PRICE_ID_TEAM: "price_team_yyy",
    STRIPE_PRICE_ID_PRO: "price_pro_zzz",
    STRIPE_PRICE_ID_MAX: "price_max_www",
    // Runner price → entitlement reverse map (the SEPARATE runner subscription
    // price ladder). Disjoint from the cache prices above.
    STRIPE_PRICE_ID_RUNNER_STARTER: "price_runner_starter_r1",
    STRIPE_PRICE_ID_RUNNER_PRO: "price_runner_pro_r2",
    STRIPE_PRICE_ID_RUNNER_TEAM: "price_runner_team_r3",
    STRIPE_PRICE_ID_RUNNER_SCALE: "price_runner_scale_r4",
    STRIPE_PRICE_ID_RUNNER_MAX: "price_runner_max_r5",
    BILLING_DB: db
        ? { prepare: db.prepare }
        : undefined,
});
