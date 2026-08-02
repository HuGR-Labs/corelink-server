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
        const db = fakeDb();
        const req = new Request("https://x.test/webhooks/stripe", {
            method: "POST",
            headers: { "stripe-signature": "t=12345,v1=badhex" },
            body: "{}",
        });
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        expect(res.status).toBe(400);
        expect(await res.text()).toBe("invalid_signature");
        // Rejected BEFORE any side effect.
        expect(db.runCalls).toHaveLength(0);
    });

    it("returns 400 with missing stripe-signature header", async () => {
        const db = fakeDb();
        const req = new Request("https://x.test/webhooks/stripe", { method: "POST", body: "{}" });
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        expect(res.status).toBe(400);
        expect(await res.text()).toBe("invalid_signature");
        expect(db.runCalls).toHaveLength(0);
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
                    payment_status: "paid",
                    metadata: {
                        tenant_id: "tenant_abc",
                        clerk_user_id: "user_clerk_xyz",
                        tier: "starter",
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

    it("missing BILLING_DB → 503 fail-closed, NO processing (was a silent 200 that no-op'd every money-path write)", async () => {
        // AUDIT FIX 2: without the binding every D1 write silently no-ops and
        // the old handler still acked 200 — Stripe never retried, so a PAID
        // checkout was dropped with no recovery. Now mirrors the
        // STRIPE_WEBHOOK_SECRET check: 503 BEFORE any processing so Stripe
        // retries until the deploy misconfiguration is fixed.
        const nowMs = Date.now();
        const event = {
            id: "evt_nodb_1",
            type: "checkout.session.completed",
            data: {
                object: {
                    customer: "cus_nodb",
                    subscription: "sub_nodb",
                    amount_total: 4900,
                    payment_status: "paid",
                    metadata: { tenant_id: "t_nodb", tier: "starter" },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const env = baseEnv();
        delete (env as { BILLING_DB?: unknown }).BILLING_DB;
        const fetchSpy = vi.mocked(globalThis.fetch);
        fetchSpy.mockClear();
        const res = await handleStripeWebhook(req, env, fakeCtx());
        expect(res.status).toBe(503);
        expect(await res.text()).toBe("billing_db_unbound");
        // No processing happened: no analytics emit either.
        expect(fetchSpy).not.toHaveBeenCalled();
    });

    it("checkout.session.completed without tenant_id metadata → 500 (fail-loud, no write, Stripe redelivers)", async () => {
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
                    payment_status: "paid",
                    metadata: {},
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        // A paid checkout.session.completed with no tenant_id is a money-path
        // anomaly: the customer PAID but we cannot provision them. We MUST NOT
        // silently 200 (the old behavior dropped the entitlement with no retry
        // and no signal) — we return 500 BEFORE the claim so Stripe redelivers.
        expect(res.status).toBe(500);
        // No billing/tier mutation (we bailed before any write).
        expect(
            db.runCalls.find(
                (c) => c.sql.includes("tenant_billing") || c.sql.includes("tier_selections"),
            ),
        ).toBeUndefined();
    });

    it("checkout.session.completed without metadata[tier] → 500 (fail-loud, no write, Stripe redelivers)", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_notier_1",
            type: "checkout.session.completed",
            data: {
                object: {
                    customer: "cus_notier",
                    subscription: "sub_notier",
                    amount_total: 14900,
                    payment_status: "paid",
                    // tenant_id + customer present, but NO tier: our checkout
                    // backend always sets metadata[tier], so this session is
                    // anomalous. The old fallback silently recorded "starter"
                    // (a Max $149 checkout billing-rowed as Starter $35).
                    metadata: { tenant_id: "tenant_notier" },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        expect(res.status).toBe(500);
        expect(await res.text()).toBe("checkout_missing_tier");
        // No billing/tier mutation (we bailed before any write).
        expect(
            db.runCalls.find(
                (c) => c.sql.includes("tenant_billing") || c.sql.includes("tier_selections"),
            ),
        ).toBeUndefined();
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
                    metadata: { tenant_id: "tenant_del", tier: "pro" },
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
            events: Array<{ event_name: string; properties: Record<string, unknown> }>;
        };
        expect(body.events[0]!.event_name).toBe("subscription_canceled");
        // from_plan is the REAL prior plan (metadata[tier] / price→tier map),
        // not the old hardcoded "starter".
        expect(body.events[0]!.properties["from_plan"]).toBe("pro");
        expect(body.events[0]!.properties["to_plan"]).toBe("free");
    });

    it("unknown event type → 200 with no billing/tier mutation or analytics (only the dedup claim)", async () => {
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
        // The ONLY D1 write is the idempotency claim (dedup runs before the
        // switch); no billing/tier_selections mutation for an unhandled type.
        expect(
            db.runCalls.every((c) =>
                c.sql.includes("INSERT OR IGNORE INTO stripe_webhook_events_processed"),
            ),
        ).toBe(true);
        expect(
            db.runCalls.find(
                (c) => c.sql.includes("tenant_billing") || c.sql.includes("tier_selections"),
            ),
        ).toBeUndefined();
        // No analytics emit for an unhandled type.
        expect(fetchSpy).not.toHaveBeenCalled();
    });

    it("idempotency: two DISTINCT checkout events both process (ON CONFLICT upsert stays safe; both 200)", async () => {
        // Distinct event ids are NOT deduped — both deliveries dispatch and both
        // exercise the tenant_billing ON CONFLICT upsert (the historical intent
        // of this test). Same-id dedup is covered by the dedup tests below.
        const db = fakeDb();
        const nowMs = Date.now();
        const mk = (id: string) => ({
            id,
            type: "checkout.session.completed",
            data: {
                object: {
                    customer: "cus_idem",
                    subscription: "sub_idem",
                    amount_total: 4900,
                    payment_status: "paid",
                    metadata: { tenant_id: "tenant_idem", tier: "starter" },
                },
            },
        });

        const req1 = await makeStripeRequest(mk("evt_idem_a"), TEST_SECRET, nowMs);
        expect((await handleStripeWebhook(req1, baseEnv(db), fakeCtx())).status).toBe(200);

        const req2 = await makeStripeRequest(mk("evt_idem_b"), TEST_SECRET, nowMs + 1000);
        expect((await handleStripeWebhook(req2, baseEnv(db), fakeCtx())).status).toBe(200);

        // Two distinct events → the ON CONFLICT upsert path runs twice.
        const insertCalls = db.runCalls.filter((c) => c.sql.includes("INSERT INTO tenant_billing"));
        expect(insertCalls).toHaveLength(2);
    });

    // ------------------------------------------------------------------
    // #34/#36: durable dedup on Stripe event.id
    // ------------------------------------------------------------------

    it("dedup: first delivery processes AND records the event in stripe_webhook_events_processed", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_dedup_first",
            type: "checkout.session.completed",
            data: {
                object: {
                    customer: "cus_dedup",
                    subscription: "sub_dedup",
                    amount_total: 4900,
                    payment_status: "paid",
                    metadata: { tenant_id: "tenant_dedup", tier: "starter" },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        expect(res.status).toBe(200);

        // The idempotency claim was written (INSERT OR IGNORE on event_id) with
        // the outcome marked 'dispatched' for a handled event type.
        const claim = db.runCalls.find((c) =>
            c.sql.includes("INSERT OR IGNORE INTO stripe_webhook_events_processed"),
        );
        expect(claim).toBeDefined();
        expect(claim!.params[0]).toBe("evt_dedup_first"); // event_id is ?1
        expect(claim!.params).toContain("checkout.session.completed");
        expect(claim!.params).toContain("dispatched");

        // First delivery still dispatched the real side effects.
        expect(
            db.runCalls.find((c) => c.sql.includes("INSERT INTO tenant_billing")),
        ).toBeDefined();
    });

    it("dedup (option b): a REDELIVERY re-runs the idempotent writes but SKIPS the analytics emit (no double MRR)", async () => {
        // Core option-(b) assertion: the entitlement/billing writes are
        // idempotent upserts and MUST always run (a redelivery safely
        // re-converges D1 — and is the recovery path for a prior failed
        // fire-and-forget write). The dedup gates ONLY the non-idempotent
        // analytics emit, so MRR is not double-counted on a Stripe retry.
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_dedup_retry",
            type: "checkout.session.completed",
            data: {
                object: {
                    customer: "cus_retry",
                    subscription: "sub_retry",
                    amount_total: 4900,
                    payment_status: "paid",
                    metadata: { tenant_id: "tenant_retry", tier: "starter" },
                },
            },
        };

        // First delivery → processes + emits.
        const req1 = await makeStripeRequest(event, TEST_SECRET, nowMs);
        expect((await handleStripeWebhook(req1, baseEnv(db), fakeCtx())).status).toBe(200);

        // Snapshot the side effects after the first delivery.
        const billingInsertsAfterFirst = db.runCalls.filter((c) =>
            c.sql.includes("INSERT INTO tenant_billing"),
        ).length;
        const fetchSpy = vi.mocked(globalThis.fetch);
        const emitsAfterFirst = fetchSpy.mock.calls.length;
        expect(billingInsertsAfterFirst).toBe(1);
        expect(emitsAfterFirst).toBeGreaterThanOrEqual(1);

        // Redelivery (Stripe retry) — SAME event id, later signature timestamp.
        const req2 = await makeStripeRequest(event, TEST_SECRET, nowMs + 1000);
        const res2 = await handleStripeWebhook(req2, baseEnv(db), fakeCtx());
        expect(res2.status).toBe(200);
        expect(await res2.text()).toBe("ok");

        // The idempotent entitlement write DID run AGAIN on the redelivery (NOT
        // short-circuited) — this is the entitlement-loss recovery guarantee.
        const billingInsertsAfterSecond = db.runCalls.filter((c) =>
            c.sql.includes("INSERT INTO tenant_billing"),
        ).length;
        expect(billingInsertsAfterSecond).toBe(billingInsertsAfterFirst + 1); // write re-ran
        // The tier_selections activation (entitlement gate) also re-ran.
        const tierActivations = db.runCalls.filter(
            (c) => c.sql.includes("INSERT INTO tier_selections") && c.sql.includes("ON CONFLICT"),
        );
        expect(tierActivations).toHaveLength(2);

        // But the analytics emit did NOT fire a second time (no double MRR).
        expect(fetchSpy.mock.calls.length).toBe(emitsAfterFirst); // no re-emit

        // The dedup claim was attempted on BOTH deliveries (the second hit the
        // PK conflict → duplicate → emit skipped, writes still ran).
        const claims = db.runCalls.filter((c) =>
            c.sql.includes("INSERT OR IGNORE INTO stripe_webhook_events_processed"),
        );
        expect(claims).toHaveLength(2);
    });

    it("dedup REGRESSION (entitlement-loss): a successful claim NEVER blocks the idempotent entitlement write on redelivery", async () => {
        // The adversarial-review CRITICAL: the original design claimed the
        // event then whole-handler short-circuited the duplicate, so a
        // redelivery skipped the (fire-and-forget, non-awaited) entitlement
        // write entirely. If the first delivery's write had failed, the
        // customer paid with NO access and NO recovery. This proves the gate
        // never blocks the entitlement write: even though the claim SUCCEEDS on
        // the first delivery (changes=1) and CONFLICTS on the second
        // (duplicate), the tier_selections activation write runs on BOTH.
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_entitlement_recovery",
            type: "checkout.session.completed",
            data: {
                object: {
                    customer: "cus_recover",
                    subscription: "sub_recover",
                    amount_total: 4900,
                    payment_status: "paid",
                    metadata: { tenant_id: "tenant_recover", tier: "starter" },
                },
            },
        };

        // Delivery 1 — claim succeeds (changes=1).
        const req1 = await makeStripeRequest(event, TEST_SECRET, nowMs);
        expect((await handleStripeWebhook(req1, baseEnv(db), fakeCtx())).status).toBe(200);

        // Delivery 2 — claim CONFLICTS (duplicate). Despite the dedup claim
        // succeeding/blocking the emit, the entitlement gate write MUST re-run.
        const req2 = await makeStripeRequest(event, TEST_SECRET, nowMs + 1000);
        expect((await handleStripeWebhook(req2, baseEnv(db), fakeCtx())).status).toBe(200);

        // The dedup recorded the first as 'claimed' and the second as a PK
        // conflict (two claim attempts).
        const claims = db.runCalls.filter((c) =>
            c.sql.includes("INSERT OR IGNORE INTO stripe_webhook_events_processed"),
        );
        expect(claims).toHaveLength(2);

        // CRITICAL: the entitlement (tier_selections) activation write ran on
        // BOTH deliveries — the dedup gate did NOT block it. No entitlement loss.
        const activations = db.runCalls.filter(
            (c) => c.sql.includes("INSERT INTO tier_selections") && c.sql.includes("ON CONFLICT"),
        );
        expect(activations).toHaveLength(2);
        // And the tenant_billing upsert ran on both deliveries too.
        const billingUpserts = db.runCalls.filter((c) =>
            c.sql.includes("INSERT INTO tenant_billing"),
        );
        expect(billingUpserts).toHaveLength(2);
    });

    it("REGRESSION (6-tier): checkout.session.completed with tier=solo activates tier_selections 'solo' (was silently skipped)", async () => {
        // The GAP-6 activation gate used an inline starter/team/pro triple, so
        // a PAID Solo ($15) or Max ($149) checkout completed WITHOUT activating
        // the entitlement FSM — pay-but-not-entitled. The gate now uses the
        // canonical asPaidTier set; this pins solo end-to-end.
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_checkout_solo_1",
            type: "checkout.session.completed",
            data: {
                object: {
                    customer: "cus_solo_1",
                    subscription: "sub_solo_1",
                    amount_total: 1500,
                    payment_status: "paid",
                    metadata: { tenant_id: "tenant_solo", tier: "solo" },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        expect((await handleStripeWebhook(req, baseEnv(db), fakeCtx())).status).toBe(200);

        const activation = db.runCalls.find(
            (c) => c.sql.includes("INSERT INTO tier_selections") && c.sql.includes("ON CONFLICT"),
        );
        expect(activation).toBeDefined();
        expect(activation!.params).toContain("solo");
    });

    it("REGRESSION (6-tier): checkout.session.completed with tier=max activates tier_selections 'max' (was silently skipped)", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_checkout_max_1",
            type: "checkout.session.completed",
            data: {
                object: {
                    customer: "cus_max_1",
                    subscription: "sub_max_1",
                    amount_total: 14900,
                    payment_status: "paid",
                    metadata: { tenant_id: "tenant_max", tier: "max" },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        expect((await handleStripeWebhook(req, baseEnv(db), fakeCtx())).status).toBe(200);

        const activation = db.runCalls.find(
            (c) => c.sql.includes("INSERT INTO tier_selections") && c.sql.includes("ON CONFLICT"),
        );
        expect(activation).toBeDefined();
        expect(activation!.params).toContain("max");
    });

    it("6-tier reverse map: subscription.updated with the SOLO price id propagates tier 'solo' (no tier metadata)", async () => {
        // Pins tierFromSubscriptionPrice for the new SKUs: no metadata[tier] on
        // the subscription object — the price→tier map alone must resolve it.
        const db = fakeDb();
        const nowMs = Date.now();
        const periodEndSec = Math.floor(nowMs / 1000) + 30 * 24 * 3600;
        const event = {
            id: "evt_sub_solo_price_1",
            type: "customer.subscription.updated",
            data: {
                object: {
                    id: "sub_solo_price",
                    customer: "cus_solo_price",
                    status: "active",
                    current_period_end: periodEndSec,
                    metadata: { tenant_id: "tenant_solo_price" },
                    items: { data: [{ price: { id: "price_solo_aaa" } }] },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        expect((await handleStripeWebhook(req, baseEnv(db), fakeCtx())).status).toBe(200);

        const propagation = db.runCalls.find(
            (c) => c.sql.includes("UPDATE tier_selections") && c.params.includes("solo"),
        );
        expect(propagation).toBeDefined();
        expect(propagation!.params).toContain("cus_solo_price");
    });

    it("6-tier reverse map: subscription.updated with the MAX price id propagates tier 'max' (no tier metadata)", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const periodEndSec = Math.floor(nowMs / 1000) + 30 * 24 * 3600;
        const event = {
            id: "evt_sub_max_price_1",
            type: "customer.subscription.updated",
            data: {
                object: {
                    id: "sub_max_price",
                    customer: "cus_max_price",
                    status: "active",
                    current_period_end: periodEndSec,
                    metadata: { tenant_id: "tenant_max_price" },
                    items: { data: [{ price: { id: "price_max_www" } }] },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        expect((await handleStripeWebhook(req, baseEnv(db), fakeCtx())).status).toBe(200);

        const propagation = db.runCalls.find(
            (c) => c.sql.includes("UPDATE tier_selections") && c.params.includes("max"),
        );
        expect(propagation).toBeDefined();
        expect(propagation!.params).toContain("cus_max_price");
    });

    it("dedup: claim INSERT failure (D1 error) FAILS SAFE — event is still processed (not lost)", async () => {
        const db = fakeDbClaimThrows();
        const nowMs = Date.now();
        const event = {
            id: "evt_claim_err",
            type: "checkout.session.completed",
            data: {
                object: {
                    customer: "cus_claim_err",
                    subscription: "sub_claim_err",
                    amount_total: 4900,
                    payment_status: "paid",
                    metadata: { tenant_id: "tenant_claim_err", tier: "starter" },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        // Claim threw → handler falls through and still dispatches (never drops
        // a possible first-time event).
        expect(res.status).toBe(200);
        expect(
            db.runCalls.find((c) => c.sql.includes("INSERT INTO tenant_billing")),
        ).toBeDefined();
    });

    it("dedup: an UNKNOWN event type is claimed with outcome 'acknowledged_unknown'", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_dedup_unknown",
            type: "invoice.payment_succeeded",
            data: { object: { id: "in_x" } },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        expect(res.status).toBe(200);
        const claim = db.runCalls.find((c) =>
            c.sql.includes("INSERT OR IGNORE INTO stripe_webhook_events_processed"),
        );
        expect(claim).toBeDefined();
        expect(claim!.params).toContain("acknowledged_unknown");
    });

    // ------------------------------------------------------------------
    // #37: durable entitlement writes — await + 500-on-failure +
    //      process-then-claim (emit exactly-once on first SUCCESSFUL delivery)
    // ------------------------------------------------------------------

    it("durability: write FAILURE → non-2xx (500), NO claim row, NO emit; then redelivery → writes succeed, claim made, emit fires once", async () => {
        // The core #37 guarantee. The first delivery's entitlement/billing write
        // throws (D1 error). The handler must NOT return 200, NOT claim, NOT emit
        // — so Stripe redelivers. The redelivery (write now succeeds) completes
        // the writes, claims the event, and emits EXACTLY ONCE. (Before #37 the
        // failed write was swallowed under a 200 → customer paid, no access.)
        let failWrites = true; // first delivery's billing write throws.
        const db = fakeDbFailableWrite(
            (sql) => failWrites && sql.includes("INSERT INTO tenant_billing"),
        );
        const nowMs = Date.now();
        const event = {
            id: "evt_durable_retry",
            type: "checkout.session.completed",
            data: {
                object: {
                    customer: "cus_durable",
                    subscription: "sub_durable",
                    amount_total: 4900,
                    payment_status: "paid",
                    metadata: { tenant_id: "tenant_durable", tier: "starter" },
                },
            },
        };
        const fetchSpy = vi.mocked(globalThis.fetch);

        // --- Delivery 1: the billing write fails. ---
        const req1 = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res1 = await handleStripeWebhook(req1, baseEnv(db), fakeCtx());
        expect(res1.status).toBe(500); // non-2xx → Stripe will redeliver.
        expect(await res1.text()).toBe("write_failed");
        // NO claim row was written (process-then-claim: we 500'd before claiming).
        expect(
            db.runCalls.find((c) =>
                c.sql.includes("INSERT OR IGNORE INTO stripe_webhook_events_processed"),
            ),
        ).toBeUndefined();
        // NO analytics emit on the failed delivery.
        expect(fetchSpy).not.toHaveBeenCalled();

        // --- Delivery 2 (Stripe redelivery): the write now succeeds. ---
        failWrites = false;
        const req2 = await makeStripeRequest(event, TEST_SECRET, nowMs + 1000);
        const res2 = await handleStripeWebhook(req2, baseEnv(db), fakeCtx());
        expect(res2.status).toBe(200);
        // The entitlement/billing write completed this time.
        expect(
            db.runCalls.find((c) => c.sql.includes("INSERT INTO tenant_billing")),
        ).toBeDefined();
        // The claim was made AFTER the writes succeeded (changes=1, first success).
        expect(
            db.runCalls.find((c) =>
                c.sql.includes("INSERT OR IGNORE INTO stripe_webhook_events_processed"),
            ),
        ).toBeDefined();
        // The emit fired EXACTLY ONCE (on the first SUCCESSFUL delivery), so MRR
        // is neither lost (the failed attempt) nor double-counted.
        expect(fetchSpy.mock.calls.length).toBe(1);
        const body = JSON.parse(fetchSpy.mock.calls[0]![1]?.body as string) as {
            events: Array<{ event_name: string }>;
        };
        expect(body.events[0]!.event_name).toBe("paid_subscription_started");
    });

    it("durability: happy path → writes AWAITED, 200, claim made, emit once", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_durable_happy",
            type: "checkout.session.completed",
            data: {
                object: {
                    customer: "cus_happy",
                    subscription: "sub_happy",
                    amount_total: 4900,
                    payment_status: "paid",
                    metadata: { tenant_id: "tenant_happy", tier: "starter" },
                },
            },
        };
        const fetchSpy = vi.mocked(globalThis.fetch);
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        expect(res.status).toBe(200);

        // Writes ran (awaited) AND the claim was written after them.
        expect(
            db.runCalls.find((c) => c.sql.includes("INSERT INTO tenant_billing")),
        ).toBeDefined();
        expect(
            db.runCalls.find((c) => c.sql.includes("INSERT INTO tier_selections")),
        ).toBeDefined();
        expect(
            db.runCalls.find((c) =>
                c.sql.includes("INSERT OR IGNORE INTO stripe_webhook_events_processed"),
            ),
        ).toBeDefined();
        // emit fired exactly once.
        expect(fetchSpy.mock.calls.length).toBe(1);
    });

    it("durability: TRUE duplicate (already-succeeded) redelivery → writes re-run, NO second emit, 200", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_durable_dup",
            type: "checkout.session.completed",
            data: {
                object: {
                    customer: "cus_dup",
                    subscription: "sub_dup",
                    amount_total: 4900,
                    payment_status: "paid",
                    metadata: { tenant_id: "tenant_dup", tier: "starter" },
                },
            },
        };
        const fetchSpy = vi.mocked(globalThis.fetch);

        // Delivery 1 — succeeds, claims, emits.
        const req1 = await makeStripeRequest(event, TEST_SECRET, nowMs);
        expect((await handleStripeWebhook(req1, baseEnv(db), fakeCtx())).status).toBe(200);
        const emitsAfterFirst = fetchSpy.mock.calls.length;
        const billingAfterFirst = db.runCalls.filter((c) =>
            c.sql.includes("INSERT INTO tenant_billing"),
        ).length;
        expect(emitsAfterFirst).toBe(1);

        // Delivery 2 — true duplicate (prior success): writes re-run (idempotent),
        // claim is a PK conflict → NO second emit, still 200.
        const req2 = await makeStripeRequest(event, TEST_SECRET, nowMs + 1000);
        const res2 = await handleStripeWebhook(req2, baseEnv(db), fakeCtx());
        expect(res2.status).toBe(200);
        // Idempotent writes re-ran (recovery path stays intact).
        expect(
            db.runCalls.filter((c) => c.sql.includes("INSERT INTO tenant_billing")).length,
        ).toBe(billingAfterFirst + 1);
        // But the emit did NOT fire a second time (exactly-once on success).
        expect(fetchSpy.mock.calls.length).toBe(emitsAfterFirst);
    });

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

    it("checkout.session.completed with UNKNOWN/missing payment_status → 500 fail-loud, no writes", async () => {
        // Fail-safe in BOTH directions: never grant on an unproven payment,
        // never silently 200 a session shape we do not understand.
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_nops_1",
            type: "checkout.session.completed",
            data: {
                object: {
                    id: "cs_nops_1",
                    customer: "cus_nops",
                    subscription: "sub_nops",
                    amount_total: 4900,
                    // NOTE: no payment_status at all (anomalous session shape).
                    metadata: { tenant_id: "tenant_nops", tier: "starter" },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        expect(res.status).toBe(500);
        expect(await res.text()).toBe("checkout_payment_status_unknown");
        expect(
            db.runCalls.find(
                (c) => c.sql.includes("tenant_billing") || c.sql.includes("tier_selections"),
            ),
        ).toBeUndefined();
    });

    it("checkout.session.async_payment_succeeded → activates IDENTICALLY to a paid completed session", async () => {
        // This is how a delayed payment method eventually activates: the
        // session completed earlier with payment_status='unpaid' (no writes);
        // once the payment clears, Stripe fires this event with the same
        // session object (now payment_status='paid').
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_async_ok_1",
            type: "checkout.session.async_payment_succeeded",
            data: {
                object: {
                    id: "cs_async_ok_1",
                    customer: "cus_async_ok",
                    subscription: "sub_async_ok",
                    amount_total: 4900,
                    payment_status: "paid",
                    metadata: { tenant_id: "tenant_async_ok", tier: "starter" },
                },
            },
        };
        const fetchSpy = vi.mocked(globalThis.fetch);
        fetchSpy.mockClear();
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        expect(res.status).toBe(200);

        // Identical activation to the paid-completed path (the shared helper):
        // tenant_billing 'paid' upsert …
        const billing = db.runCalls.find((c) => c.sql.includes("INSERT INTO tenant_billing"));
        expect(billing).toBeDefined();
        expect(billing!.sql).toContain("'paid'");
        expect(billing!.params).toContain("tenant_async_ok");
        // … + canonical tier_selections activation …
        const activation = db.runCalls.find(
            (c) => c.sql.includes("INSERT INTO tier_selections") && c.sql.includes("ON CONFLICT"),
        );
        expect(activation).toBeDefined();
        expect(activation!.params).toContain("starter");
        expect(activation!.sql).toContain("'active'");
        // … + the paid_subscription_started MRR emit.
        const body = JSON.parse(fetchSpy.mock.calls[0]![1]?.body as string) as {
            events: Array<{ event_name: string; tenant_id: string }>;
        };
        expect(body.events[0]!.event_name).toBe("paid_subscription_started");
        expect(body.events[0]!.tenant_id).toBe("tenant_async_ok");
    });

    it("checkout.session.async_payment_succeeded missing metadata[tier] → 500 (same fail-loud as completed)", async () => {
        // The shared arm means the async path inherits ALL the completed-path
        // validation — pin one of the guards to prove it.
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_async_notier_1",
            type: "checkout.session.async_payment_succeeded",
            data: {
                object: {
                    id: "cs_async_notier_1",
                    customer: "cus_async_notier",
                    subscription: "sub_async_notier",
                    amount_total: 14900,
                    payment_status: "paid",
                    metadata: { tenant_id: "tenant_async_notier" }, // no tier
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        expect(res.status).toBe(500);
        expect(await res.text()).toBe("checkout_missing_tier");
        expect(
            db.runCalls.find(
                (c) => c.sql.includes("tenant_billing") || c.sql.includes("tier_selections"),
            ),
        ).toBeUndefined();
    });

    it("checkout.session.async_payment_failed → 200, no billing/tier writes (nothing was granted, nothing to revoke)", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_async_fail_1",
            type: "checkout.session.async_payment_failed",
            data: {
                object: {
                    id: "cs_async_fail_1",
                    customer: "cus_async_fail",
                    subscription: "sub_async_fail",
                    payment_status: "unpaid",
                    metadata: { tenant_id: "tenant_async_fail", tier: "starter" },
                },
            },
        };
        const fetchSpy = vi.mocked(globalThis.fetch);
        fetchSpy.mockClear();
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        expect(res.status).toBe(200);
        // No entitlement/billing mutation (the unpaid completed event never
        // wrote anything, so there is nothing to revert) — only the dedup
        // claim is allowed to touch D1.
        expect(
            db.runCalls.find(
                (c) => c.sql.includes("tenant_billing") || c.sql.includes("tier_selections"),
            ),
        ).toBeUndefined();
        // And no analytics emit.
        expect(fetchSpy).not.toHaveBeenCalled();
        // The event IS claimed as a dispatched type (forensics).
        const claim = db.runCalls.find((c) =>
            c.sql.includes("INSERT OR IGNORE INTO stripe_webhook_events_processed"),
        );
        expect(claim).toBeDefined();
        expect(claim!.params).toContain("dispatched");
    });

    // ------------------------------------------------------------------
    // AUDIT FIX 3: price↔tier defense-in-depth assert. metadata[tier] is
    // server-set at checkout, but a checkout-backend bug could desync it from
    // the price actually subscribed. The checkout.session webhook payload
    // carries no price data (line_items requires expand[], never present in
    // events), so the cross-check lives on subscription.created/updated —
    // the first payloads that carry BOTH metadata[tier] and the price.
    // ------------------------------------------------------------------

    it("subscription.updated with metadata[tier] CONTRADICTING the subscribed price → 500 fail-loud, no writes", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_mismatch_upd_1",
            type: "customer.subscription.updated",
            data: {
                object: {
                    id: "sub_mismatch_upd",
                    customer: "cus_mismatch_upd",
                    status: "active",
                    current_period_end: Math.floor(nowMs / 1000) + 30 * 24 * 3600,
                    // metadata says PRO but the actual subscribed price is STARTER.
                    metadata: { tenant_id: "tenant_mismatch_upd", tier: "pro" },
                    items: { data: [{ price: { id: "price_starter_xxx" } }] },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        expect(res.status).toBe(500);
        expect(await res.text()).toBe("subscription_tier_price_mismatch");
        // No write of EITHER candidate tier — never pick a winner.
        expect(
            db.runCalls.find(
                (c) => c.sql.includes("tenant_billing") || c.sql.includes("tier_selections"),
            ),
        ).toBeUndefined();
    });

    it("subscription.updated with metadata[tier] AGREEING with the price → processes normally (no false positive)", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_match_upd_1",
            type: "customer.subscription.updated",
            data: {
                object: {
                    id: "sub_match_upd",
                    customer: "cus_match_upd",
                    status: "active",
                    current_period_end: Math.floor(nowMs / 1000) + 30 * 24 * 3600,
                    metadata: { tenant_id: "tenant_match_upd", tier: "pro" },
                    items: { data: [{ price: { id: "price_pro_zzz" } }] },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        expect(res.status).toBe(200);
        const update = db.runCalls.find((c) => c.sql.includes("UPDATE tenant_billing"));
        expect(update).toBeDefined();
        expect(update!.params).toContain("paid");
        // Tier propagation used the (consistent) resolved tier.
        const tierUpdate = db.runCalls.find(
            (c) => c.sql.includes("UPDATE tier_selections") && c.sql.includes("SET tier"),
        );
        expect(tierUpdate).toBeDefined();
        expect(tierUpdate!.params).toContain("pro");
    });

    it("subscription.created with metadata[tier] CONTRADICTING the subscribed price → 500 fail-loud, no backfill", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_mismatch_cre_1",
            type: "customer.subscription.created",
            data: {
                object: {
                    id: "sub_mismatch_cre",
                    customer: "cus_mismatch_cre",
                    current_period_end: Math.floor(nowMs / 1000) + 30 * 24 * 3600,
                    // metadata says MAX but the actual subscribed price is SOLO.
                    metadata: { tenant_id: "tenant_mismatch_cre", tier: "max" },
                    items: { data: [{ price: { id: "price_solo_aaa" } }] },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        expect(res.status).toBe(500);
        expect(await res.text()).toBe("subscription_tier_price_mismatch");
        expect(
            db.runCalls.find(
                (c) => c.sql.includes("tenant_billing") || c.sql.includes("tier_selections"),
            ),
        ).toBeUndefined();
    });

    // ------------------------------------------------------------------
    // AUDIT FIX 4: tenant_billing terminal-state guard. Stripe webhooks are
    // unordered — a late customer.subscription.updated(status=active) arriving
    // AFTER customer.subscription.deleted used to resurrect tenant_billing to
    // 'paid'. The UPDATE now carries `AND status != 'canceled'` so a canceled
    // row never moves forward (matching the canonical tier_selections gate,
    // where activation only ever happens via checkout.session.completed).
    // The fake DB is stateless, so the regression is pinned on the SQL guard
    // the statement executes with.
    // ------------------------------------------------------------------

    it("terminal-state: deleted then LATE updated(active) → the billing UPDATE is guarded so a 'canceled' row stays canceled", async () => {
        const db = fakeDb();
        const nowMs = Date.now();

        // 1) The subscription is deleted → tenant_billing.status = 'canceled'.
        const del = {
            id: "evt_term_del",
            type: "customer.subscription.deleted",
            data: {
                object: {
                    id: "sub_terminal",
                    customer: "cus_terminal",
                    metadata: { tenant_id: "tenant_terminal" },
                },
            },
        };
        let req = await makeStripeRequest(del, TEST_SECRET, nowMs);
        expect((await handleStripeWebhook(req, baseEnv(db), fakeCtx())).status).toBe(200);
        expect(db.runCalls.find((c) => c.sql.includes("'canceled'"))).toBeDefined();

        // 2) A LATE/out-of-order updated(status=active) for the SAME
        //    subscription arrives afterwards.
        const lateUpdate = {
            id: "evt_term_late_upd",
            type: "customer.subscription.updated",
            data: {
                object: {
                    id: "sub_terminal",
                    customer: "cus_terminal",
                    status: "active",
                    current_period_end: Math.floor(nowMs / 1000) + 30 * 24 * 3600,
                    metadata: { tenant_id: "tenant_terminal" },
                },
            },
        };
        req = await makeStripeRequest(lateUpdate, TEST_SECRET, nowMs + 1000);
        expect((await handleStripeWebhook(req, baseEnv(db), fakeCtx())).status).toBe(200);

        // The status='paid' UPDATE the late event executes is guarded: it can
        // never touch a row whose status is already 'canceled'.
        const update = db.runCalls.find(
            (c) => c.sql.includes("UPDATE tenant_billing") && c.params.includes("paid"),
        );
        expect(update).toBeDefined();
        expect(update!.sql).toContain("status != 'canceled'");

        // And the canonical gate is NOT resurrected for a CANCELED subscription.
        // A dunning-recovery re-activation statement IS issued on any
        // grantsAccess updated event, but it is GUARDED on
        // `tenant_billing.status != 'canceled'`, so against a canceled row it is
        // a definitive no-op (re-subscribing must go through checkout). Assert
        // the guard is present on any tier_selections→'active' write here.
        const lateActivation = db.runCalls.find(
            (c) =>
                c.sql.includes("tier_selections") &&
                c.sql.includes("'active'") &&
                !c.sql.includes("'inactive'"),
        );
        if (lateActivation) {
            expect(lateActivation.sql).toContain("status != 'canceled'");
        }
    });

    // ------------------------------------------------------------------
    // F34: invoice.payment_failed missing fallback revocation path
    // A terminal invoice.payment_failed whose payload omits `customer` must
    // STILL revoke the canonical access gate (tier_selections), keyed by
    // subscription id via tenant_billing (exactly as subscription.updated and
    // subscription.deleted do). Without the fix, `tier_selections.subscription_state`
    // stays 'active' and the tenant retains paid-tier access despite exhausted dunning.
    // ------------------------------------------------------------------

    it("F34: invoice.payment_failed terminal with NO customer → gate STILL revoked by subscription id (fail-closed fallback)", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_pf_no_customer",
            type: "invoice.payment_failed",
            data: {
                object: {
                    // subscription present — always available on invoice objects.
                    subscription: "sub_pf_nocust",
                    // NOTE: `customer` field intentionally omitted — the F34 bug scenario.
                    attempt_count: 4,
                    next_payment_attempt: null, // terminal: Stripe has given up
                    metadata: { tenant_id: "tenant_pf_nocust" },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        expect(res.status).toBe(200);

        // Secondary mirror: tenant_billing.status='past_due' (keyed by subscription id).
        const billing = db.runCalls.find(
            (c) => c.sql.includes("UPDATE tenant_billing") && c.params.includes("past_due"),
        );
        expect(billing).toBeDefined();
        expect(billing!.params).toContain("sub_pf_nocust");
        // Must NOT have written current_period_end_ms (status-only SQL).
        expect(billing!.sql).not.toContain("current_period_end_ms");

        // CANONICAL gate: tier_selections MUST be flipped inactive — keyed by
        // subscription id (via tenant_billing subquery), NOT by customer (absent).
        // This is the F34 fix: the fallback path that was previously missing.
        const deact = db.runCalls.find(
            (c) =>
                c.sql.includes("UPDATE tier_selections") &&
                c.sql.includes("'inactive'") &&
                c.sql.includes("tenant_billing"),
        );
        expect(deact).toBeDefined();
        expect(deact!.params).toContain("sub_pf_nocust");
    });

    // H1: even WITH a customer field present, revocation must be keyed by the
    // SPECIFIC subscription (via tenant_billing), NOT by the shared customer —
    // otherwise a runner-sub failure would revoke the tenant's cache tier. There
    // is no longer a customer-keyed revocation path.
    it("H1: invoice.payment_failed terminal WITH customer → gate revoked by SUBSCRIPTION id (never by customer)", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_pf_with_customer",
            type: "invoice.payment_failed",
            data: {
                object: {
                    subscription: "sub_pf_cust",
                    customer: "cus_pf_cust",
                    attempt_count: 4,
                    next_payment_attempt: null,
                    metadata: { tenant_id: "tenant_pf_cust" },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        expect(res.status).toBe(200);

        // The deactivation must resolve the tenant THROUGH tenant_billing by the
        // subscription id — and must NOT bind the shared customer.
        const deact = db.runCalls.find(
            (c) =>
                c.sql.includes("UPDATE tier_selections") &&
                c.sql.includes("'inactive'"),
        );
        expect(deact).toBeDefined();
        expect(deact!.sql).toContain("tenant_billing");
        expect(deact!.params).toContain("sub_pf_cust");
        // Crucially: the customer id is NEVER used as a revocation key (that is the
        // H1 defect — it would flip every tier_selections row for the tenant).
        expect(deact!.params).not.toContain("cus_pf_cust");
        // And there is no customer-keyed (subquery-less) tier_selections deactivate.
        const deactByCustomer = db.runCalls.find(
            (c) =>
                c.sql.includes("UPDATE tier_selections") &&
                c.sql.includes("'inactive'") &&
                c.sql.includes("stripe_customer_id"),
        );
        expect(deactByCustomer).toBeUndefined();
    });

    it("terminal-state: a late invoice.payment_failed after cancel is guarded too (status-only UPDATE carries the guard)", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_term_pf",
            type: "invoice.payment_failed",
            data: {
                object: {
                    subscription: "sub_term_pf",
                    customer: "cus_term_pf",
                    attempt_count: 4,
                    next_payment_attempt: null, // terminal dunning failure
                    metadata: { tenant_id: "tenant_term_pf" },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        expect((await handleStripeWebhook(req, baseEnv(db), fakeCtx())).status).toBe(200);
        const billing = db.runCalls.find(
            (c) => c.sql.includes("UPDATE tenant_billing") && c.params.includes("past_due"),
        );
        expect(billing).toBeDefined();
        // 'canceled' is terminal: even the past_due status writer is guarded.
        expect(billing!.sql).toContain("status != 'canceled'");
    });

    // ==================================================================
    // WP2: SELF-SERVE RUNNER PURCHASE — seed + revoke runners_entitlement
    // via the dedicated runner_billing map (migration 0087). The runner
    // subscription is a SEPARATE Stripe subscription; a runner event must
    // NEVER touch tier_selections / tenant_billing (the cache plane), and a
    // cache event must NEVER touch runners_entitlement / runner_billing.
    // ==================================================================

    it("runner checkout.session.completed → runner_billing upserted, NO cache tier_selections activation", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_runner_checkout_1",
            type: "checkout.session.completed",
            data: {
                object: {
                    id: "cs_runner_1",
                    customer: "cus_runner_1",
                    subscription: "sub_runner_1",
                    amount_total: 3000,
                    payment_status: "paid",
                    metadata: { tenant_id: "tenant_runner_1", tier: "runner_starter" },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        expect(res.status).toBe(200);

        // runner_billing row upserted (subscription→tenant map) with status active.
        const rb = db.runCalls.find((c) => c.sql.includes("INSERT INTO runner_billing"));
        expect(rb).toBeDefined();
        expect(rb!.params).toContain("sub_runner_1");
        expect(rb!.params).toContain("tenant_runner_1");
        expect(rb!.params).toContain("runner_starter");
        expect(rb!.params).toContain("active");

        // NO cache activation: neither tier_selections nor tenant_billing touched.
        expect(db.runCalls.find((c) => c.sql.includes("tier_selections"))).toBeUndefined();
        expect(db.runCalls.find((c) => c.sql.includes("tenant_billing"))).toBeUndefined();
        // Entitlement is NOT seeded at checkout (no price on the session); it
        // arrives on the subscription.created/updated event.
        expect(
            db.runCalls.find((c) => c.sql.includes("INSERT INTO runners_entitlement")),
        ).toBeUndefined();

        // Analytics: runner_subscription_started (distinct from the cache MRR event).
        const fetchSpy = vi.mocked(globalThis.fetch);
        const body = JSON.parse(fetchSpy.mock.calls[0]![1]?.body as string) as {
            events: Array<{ event_name: string; tenant_id: string }>;
        };
        expect(body.events[0]!.event_name).toBe("runner_subscription_started");
        expect(body.events[0]!.tenant_id).toBe("tenant_runner_1");
    });

    it("runner checkout.session.completed missing subscription id → 500 fail-loud, no runner_billing write", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_runner_nosub_1",
            type: "checkout.session.completed",
            data: {
                object: {
                    id: "cs_runner_nosub_1",
                    customer: "cus_runner_nosub",
                    // NOTE: no subscription id — cannot map the runner sub.
                    amount_total: 3000,
                    payment_status: "paid",
                    metadata: { tenant_id: "tenant_runner_nosub", tier: "runner_pro" },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        expect(res.status).toBe(500);
        expect(await res.text()).toBe("checkout_runner_missing_subscription");
        expect(db.runCalls.find((c) => c.sql.includes("runner_billing"))).toBeUndefined();
    });

    // Each of the 5 runner tiers: subscription event with the runner PRICE and a
    // granting status seeds runners_entitlement with the correct caps.
    const runnerLadder: Array<[string, string, number, number]> = [
        ["price_runner_starter_r1", "runner_starter", 20, 100],
        ["price_runner_pro_r2", "runner_pro", 40, 240],
        ["price_runner_team_r3", "runner_team", 80, 600],
        ["price_runner_scale_r4", "runner_scale", 160, 1200],
        ["price_runner_max_r5", "runner_max", 320, 2400],
    ];
    for (const [priceId, tier, maxConcurrency, maxVcpuH] of runnerLadder) {
        it(`runner subscription.updated (${tier}, active) → runners_entitlement seeded ${maxConcurrency}/${maxVcpuH}`, async () => {
            const db = fakeDb();
            const nowMs = Date.now();
            const event = {
                id: `evt_runner_seed_${tier}`,
                type: "customer.subscription.updated",
                data: {
                    object: {
                        id: `sub_runner_seed_${tier}`,
                        customer: `cus_runner_seed_${tier}`,
                        status: "active",
                        current_period_end: Math.floor(nowMs / 1000) + 30 * 24 * 3600,
                        // Runner price; NO cache tier metadata (metadata[tier] is a
                        // runner tier, which the cache map ignores).
                        metadata: { tenant_id: `tenant_runner_seed_${tier}`, tier },
                        items: { data: [{ price: { id: priceId } }] },
                    },
                },
            };
            const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
            const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
            expect(res.status).toBe(200);

            // runners_entitlement seeded with the tier's caps.
            const ent = db.runCalls.find((c) =>
                c.sql.includes("INSERT INTO runners_entitlement"),
            );
            expect(ent).toBeDefined();
            expect(ent!.params).toContain(maxConcurrency); // max_concurrency
            expect(ent!.params).toContain(maxVcpuH); // max_vcpu_h
            // Race-free: the event carries `metadata.tenant_id`, so the seed binds
            // the tenant id DIRECTLY and does NOT correlate through `runner_billing`
            // — the concurrent `runner_billing` INSERT can't lose the race (the
            // money-path bug where a paying customer got 0 capacity).
            expect(ent!.params).toContain(`tenant_runner_seed_${tier}`); // tenant id
            expect(ent!.sql).not.toContain("FROM runner_billing");
            // runner_billing status upserted (tenant+plan from metadata).
            const rb = db.runCalls.find((c) => c.sql.includes("INSERT INTO runner_billing"));
            expect(rb).toBeDefined();
            expect(rb!.params).toContain(tier);
            // The cache plane is NOT MUTATED for a runner sub: the cache
            // tenant_billing UPDATE + any tier_selections re-activation are issued
            // (the arm is shared) but every one is guarded through tenant_billing,
            // which has NO row for the runner subscription id → 0 rows affected.
            // Assert there is NO unguarded/customer-keyed cache write (which WOULD
            // touch a real cache tenant): no tier_selections write keyed by
            // customer, and no SET tier propagation (cache resolveSubscriptionTier
            // is null for a runner price).
            for (const c of db.runCalls) {
                if (c.sql.includes("tier_selections")) {
                    // Every tier_selections statement here MUST be subquery-guarded
                    // through tenant_billing (safe no-op), never a bare customer key.
                    expect(c.sql).toContain("tenant_billing");
                }
            }
            expect(
                db.runCalls.find((c) => c.sql.includes("UPDATE tier_selections") && c.sql.includes("SET tier")),
            ).toBeUndefined();
            // No revoke on a granting status.
            expect(
                db.runCalls.find((c) => c.sql.includes("DELETE FROM runners_entitlement")),
            ).toBeUndefined();
        });
    }

    it("runner subscription.created (runner price, active) → runners_entitlement seeded (created arm)", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_runner_created_1",
            type: "customer.subscription.created",
            data: {
                object: {
                    id: "sub_runner_created_1",
                    customer: "cus_runner_created_1",
                    status: "active",
                    current_period_end: Math.floor(nowMs / 1000) + 30 * 24 * 3600,
                    metadata: { tenant_id: "tenant_runner_created_1", tier: "runner_team" },
                    items: { data: [{ price: { id: "price_runner_team_r3" } }] },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        expect(res.status).toBe(200);

        const ent = db.runCalls.find((c) => c.sql.includes("INSERT INTO runners_entitlement"));
        expect(ent).toBeDefined();
        expect(ent!.params).toContain(80);
        expect(ent!.params).toContain(600);
    });

    it("runner seed does NOT correlate on runner_billing when the tenant is known (race fix — paying customer must always get capacity)", async () => {
        // Regression for the prod money-path bug: `runner_billing` (INSERT) and the
        // entitlement seed were both pushed onto `requiredWrites` and run via
        // `Promise.all`, i.e. CONCURRENTLY. The old seed `SELECT`ed the tenant FROM
        // `runner_billing`, so it could race ahead of that INSERT, read no row, and
        // silently seed 0 rows — a paying runner customer with no capacity (acquire
        // 429). With the tenant known from metadata, the seed MUST bind it directly.
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_runner_race_1",
            type: "customer.subscription.created",
            data: {
                object: {
                    id: "sub_runner_race_1",
                    customer: "cus_runner_race_1",
                    status: "active",
                    current_period_end: Math.floor(nowMs / 1000) + 30 * 24 * 3600,
                    metadata: { tenant_id: "tenant_runner_race_1", tier: "runner_starter" },
                    items: { data: [{ price: { id: "price_runner_starter_r1" } }] },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        expect(res.status).toBe(200);

        const ent = db.runCalls.find((c) => c.sql.includes("INSERT INTO runners_entitlement"));
        expect(ent).toBeDefined();
        // The seed binds the tenant id directly and does NOT read runner_billing —
        // so it cannot lose the race with the concurrent runner_billing INSERT.
        expect(ent!.sql).not.toContain("FROM runner_billing");
        expect(ent!.sql).toContain("VALUES");
        expect(ent!.params).toContain("tenant_runner_race_1");
        expect(ent!.params).toContain(20); // runner_starter max_concurrency
        expect(ent!.params).toContain(100); // runner_starter max_vcpu_h
    });

    it("runner subscription.updated non-granting status (past_due) → runners_entitlement REVOKED", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_runner_pastdue_1",
            type: "customer.subscription.updated",
            data: {
                object: {
                    id: "sub_runner_pastdue_1",
                    customer: "cus_runner_pastdue_1",
                    status: "past_due", // non-granting
                    metadata: { tenant_id: "tenant_runner_pastdue_1", tier: "runner_pro" },
                    items: { data: [{ price: { id: "price_runner_pro_r2" } }] },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        expect(res.status).toBe(200);

        // Revoked, not seeded.
        const del = db.runCalls.find((c) => c.sql.includes("DELETE FROM runners_entitlement"));
        expect(del).toBeDefined();
        expect(del!.params).toContain("sub_runner_pastdue_1");
        expect(
            db.runCalls.find((c) => c.sql.includes("INSERT INTO runners_entitlement")),
        ).toBeUndefined();
        // runner_billing status mirror updated.
        expect(db.runCalls.find((c) => c.sql.includes("runner_billing"))).toBeDefined();
    });

    it("runner subscription.deleted → runners_entitlement revoked + runner_billing marked canceled", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_runner_deleted_1",
            type: "customer.subscription.deleted",
            data: {
                object: {
                    id: "sub_runner_deleted_1",
                    customer: "cus_runner_deleted_1",
                    metadata: { tenant_id: "tenant_runner_deleted_1", tier: "runner_max" },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        expect(res.status).toBe(200);

        // Entitlement DELETEd (resolved through runner_billing subquery).
        const del = db.runCalls.find((c) => c.sql.includes("DELETE FROM runners_entitlement"));
        expect(del).toBeDefined();
        expect(del!.params).toContain("sub_runner_deleted_1");
        expect(del!.sql).toContain("SELECT tenant_id FROM runner_billing");
        // Defense-in-depth (launch-audit): the DELETE is guarded so it does NOT
        // over-revoke a tenant that still holds ANOTHER active/trialing runner sub,
        // while a single-sub cancel still deletes (self-exclusion by subscription id).
        expect(del!.sql).toContain("status IN ('active', 'trialing')");
        expect(del!.sql).toContain("runner_subscription_id != ?1");
        // runner_billing marked canceled (status-only).
        const rb = db.runCalls.find(
            (c) => c.sql.includes("UPDATE runner_billing") && c.params.includes("canceled"),
        );
        expect(rb).toBeDefined();
        expect(rb!.params).toContain("sub_runner_deleted_1");
    });

    it("runner invoice.payment_failed terminal → runners_entitlement revoked + runner_billing past_due", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_runner_pf_1",
            type: "invoice.payment_failed",
            data: {
                object: {
                    subscription: "sub_runner_pf_1",
                    customer: "cus_runner_pf_1",
                    attempt_count: 4,
                    next_payment_attempt: null, // terminal
                    metadata: { tenant_id: "tenant_runner_pf_1" },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        expect(res.status).toBe(200);

        // Runner entitlement revoked (disambiguated THROUGH runner_billing —
        // the invoice carries no price, which is exactly why the table exists).
        const del = db.runCalls.find((c) => c.sql.includes("DELETE FROM runners_entitlement"));
        expect(del).toBeDefined();
        expect(del!.params).toContain("sub_runner_pf_1");
        // runner_billing status → past_due.
        const rb = db.runCalls.find(
            (c) => c.sql.includes("UPDATE runner_billing") && c.params.includes("past_due"),
        );
        expect(rb).toBeDefined();
        expect(rb!.params).toContain("sub_runner_pf_1");
    });

    // ---- NO-REGRESSION: a CACHE event must not touch the runner tables ----

    it("cache checkout.session.completed → NO runner_billing / runners_entitlement writes (no regression)", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_cache_noregress_1",
            type: "checkout.session.completed",
            data: {
                object: {
                    customer: "cus_cache_nr",
                    subscription: "sub_cache_nr",
                    amount_total: 4900,
                    payment_status: "paid",
                    metadata: { tenant_id: "tenant_cache_nr", tier: "starter" },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        expect(res.status).toBe(200);
        // Cache activation happened …
        expect(db.runCalls.find((c) => c.sql.includes("INSERT INTO tier_selections"))).toBeDefined();
        // … but the runner tables are untouched.
        expect(db.runCalls.find((c) => c.sql.includes("runner_billing"))).toBeUndefined();
        expect(db.runCalls.find((c) => c.sql.includes("runners_entitlement"))).toBeUndefined();
    });

    it("cache subscription.updated (cache price) → NO runners_entitlement seed/revoke (no regression)", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_cache_sub_noregress_1",
            type: "customer.subscription.updated",
            data: {
                object: {
                    id: "sub_cache_nr_2",
                    customer: "cus_cache_nr_2",
                    status: "active",
                    current_period_end: Math.floor(nowMs / 1000) + 30 * 24 * 3600,
                    metadata: { tenant_id: "tenant_cache_nr_2" },
                    items: { data: [{ price: { id: "price_pro_zzz" } }] }, // CACHE price
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        expect(res.status).toBe(200);
        // Cache tier propagation ran …
        expect(
            db.runCalls.find((c) => c.sql.includes("UPDATE tier_selections") && c.sql.includes("SET tier")),
        ).toBeDefined();
        // … no runner entitlement write of either kind.
        expect(
            db.runCalls.find(
                (c) =>
                    c.sql.includes("INSERT INTO runners_entitlement") ||
                    c.sql.includes("DELETE FROM runners_entitlement"),
            ),
        ).toBeUndefined();
    });

    it("cache subscription.deleted → runner writers issued but are no-ops (subquery matches no runner_billing row)", async () => {
        // The deleted/payment_failed arms issue the runner revoke UNCONDITIONALLY
        // (disambiguation via runner_billing). For a CACHE cancel the statements
        // run but resolve through runner_billing → 0 rows. Assert the runner
        // revoke SQL is present AND correctly subquery-guarded so it cannot
        // affect a non-runner tenant.
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_cache_del_noregress_1",
            type: "customer.subscription.deleted",
            data: {
                object: {
                    id: "sub_cache_del_nr",
                    customer: "cus_cache_del_nr",
                    metadata: { tenant_id: "tenant_cache_del_nr", tier: "pro" },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db), fakeCtx());
        expect(res.status).toBe(200);
        const del = db.runCalls.find((c) => c.sql.includes("DELETE FROM runners_entitlement"));
        expect(del).toBeDefined();
        // Guarded through the runner_billing subquery → a cache-sub id matches no
        // runner_billing row, so this DELETE affects nothing (safe no-op).
        expect(del!.sql).toContain("SELECT tenant_id FROM runner_billing");
    });

    // ------------------------------------------------------------------
    // H1 (money/GDPR): two subscriptions on ONE Stripe customer (checkout
    // reuses one cus_… per tenant across the cache AND runner subs). A terminal
    // event for the RUNNER subscription must NOT flip the tenant's ACTIVE
    // cache-tier row to inactive. The stateless runCalls fakes above assert the
    // SQL SHAPE; this stateful fake proves the ROW OUTCOME (fails-before /
    // passes-after) by executing the two tier_selections revoke SQL variants
    // against seeded rows.
    // ------------------------------------------------------------------

    /**
     * Stateful D1 stub holding tier_selections / tenant_billing / runner_billing
     * rows. It interprets ONLY the statements this test path issues:
     *  - the dedup claim (PK-conflict changes 1/0),
     *  - a tier_selections deactivate — EITHER the customer-keyed variant
     *    (`WHERE stripe_customer_id = ?1`, the pre-H1 bug) OR the
     *    subscription-keyed variant (`… tenant_id IN (SELECT tenant_id FROM
     *    tenant_billing WHERE stripe_subscription_id = ?1)`, the H1 fix).
     * Every other statement is a recorded no-op. Whichever SQL the handler emits,
     * the fake applies its real semantics — so reverting the fix (customer-keyed)
     * would genuinely flip the cache row and fail the assertion.
     */
    function fakeStatefulBillingDb(seed: {
        tierSelections: Array<{
            tenant_id: string;
            stripe_customer_id: string;
            subscription_state: string;
        }>;
        tenantBilling: Array<{ tenant_id: string; stripe_subscription_id: string }>;
        runnerBilling: Array<{ runner_subscription_id: string; tenant_id: string }>;
    }): { prepare: Mock; tierSelections: typeof seed.tierSelections } {
        const { tierSelections, tenantBilling } = seed;
        const claimed = new Set<string>();
        const prepare = vi.fn((sql: string) => {
            const params: unknown[] = [];
            const stmt = {
                bind: vi.fn((...args: unknown[]) => {
                    params.push(...args);
                    return stmt;
                }),
                run: vi.fn(async () => {
                    if (sql.includes("INSERT OR IGNORE INTO stripe_webhook_events_processed")) {
                        const id = params[0] as string;
                        const changes = claimed.has(id) ? 0 : (claimed.add(id), 1);
                        return { meta: { changes } };
                    }
                    // tier_selections revoke → apply the real WHERE semantics.
                    if (sql.includes("UPDATE tier_selections") && sql.includes("'inactive'")) {
                        const key = params[0] as string;
                        if (sql.includes("stripe_customer_id = ?1")) {
                            // Pre-H1 customer-keyed path (the bug): flips EVERY row
                            // sharing the customer.
                            for (const r of tierSelections) {
                                if (r.stripe_customer_id === key && r.subscription_state !== "inactive") {
                                    r.subscription_state = "inactive";
                                }
                            }
                        } else if (sql.includes("SELECT tenant_id FROM tenant_billing")) {
                            // H1 subscription-keyed path: resolve the tenant THROUGH
                            // tenant_billing (cache-only map) by the subscription id.
                            const tenants = tenantBilling
                                .filter((b) => b.stripe_subscription_id === key)
                                .map((b) => b.tenant_id);
                            for (const r of tierSelections) {
                                if (tenants.includes(r.tenant_id) && r.subscription_state !== "inactive") {
                                    r.subscription_state = "inactive";
                                }
                            }
                        }
                    }
                    // All other statements (cancelBilling, runner writers) are no-ops.
                    return { meta: { changes: 1 } };
                }),
            };
            return stmt;
        });
        return { prepare, tierSelections };
    }

    it("H1 REGRESSION: runner subscription.deleted on a shared customer leaves the ACTIVE cache tier row ACTIVE (fails pre-fix)", async () => {
        const nowMs = Date.now();
        const db = fakeStatefulBillingDb({
            // The tenant's cache tier is ACTIVE (a paying cache customer).
            tierSelections: [
                {
                    tenant_id: "tenant_h1",
                    stripe_customer_id: "cus_h1_shared",
                    subscription_state: "active",
                },
            ],
            // tenant_billing maps ONLY the cache subscription (one row per tenant).
            tenantBilling: [{ tenant_id: "tenant_h1", stripe_subscription_id: "sub_cache_h1" }],
            // runner_billing maps the SEPARATE runner subscription (same tenant/customer).
            runnerBilling: [{ runner_subscription_id: "sub_runner_h1", tenant_id: "tenant_h1" }],
        });

        // Cancel the RUNNER add-on. The deleted subscription object carries the
        // SHARED customer (as real Stripe events do).
        const event = {
            id: "evt_h1_runner_del",
            type: "customer.subscription.deleted",
            data: {
                object: {
                    id: "sub_runner_h1",
                    customer: "cus_h1_shared",
                    metadata: { tenant_id: "tenant_h1", tier: "runner_max" },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db as unknown as ReturnType<typeof fakeDb>), fakeCtx());
        expect(res.status).toBe(200);

        // THE ASSERTION: the paying cache customer keeps access — the cache tier
        // row is STILL 'active'. (Pre-H1 the customer-keyed revoke flipped it to
        // 'inactive' because it shares the tenant's single Stripe customer.)
        expect(db.tierSelections[0]!.subscription_state).toBe("active");
    });

    it("H1 CONTROL: cache subscription.deleted DOES deactivate the cache tier row (no fail-open)", async () => {
        const nowMs = Date.now();
        const db = fakeStatefulBillingDb({
            tierSelections: [
                {
                    tenant_id: "tenant_h1",
                    stripe_customer_id: "cus_h1_shared",
                    subscription_state: "active",
                },
            ],
            tenantBilling: [{ tenant_id: "tenant_h1", stripe_subscription_id: "sub_cache_h1" }],
            runnerBilling: [{ runner_subscription_id: "sub_runner_h1", tenant_id: "tenant_h1" }],
        });

        // Cancel the CACHE subscription itself → the cache tier MUST be revoked.
        const event = {
            id: "evt_h1_cache_del",
            type: "customer.subscription.deleted",
            data: {
                object: {
                    id: "sub_cache_h1",
                    customer: "cus_h1_shared",
                    metadata: { tenant_id: "tenant_h1", tier: "pro" },
                },
            },
        };
        const req = await makeStripeRequest(event, TEST_SECRET, nowMs);
        const res = await handleStripeWebhook(req, baseEnv(db as unknown as ReturnType<typeof fakeDb>), fakeCtx());
        expect(res.status).toBe(200);

        // The cache sub id IS in tenant_billing → the tenant's cache tier row is
        // deactivated. Scoping by subscription did not break legitimate revocation.
        expect(db.tierSelections[0]!.subscription_state).toBe("inactive");
    });

    // ---- IDEMPOTENCY: redelivery re-runs harmlessly (distinct from same-id dedup) ----

    it("runner subscription.updated REDELIVERY re-runs the idempotent entitlement seed (ON CONFLICT), no error", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_runner_idem_1",
            type: "customer.subscription.updated",
            data: {
                object: {
                    id: "sub_runner_idem_1",
                    customer: "cus_runner_idem_1",
                    status: "active",
                    current_period_end: Math.floor(nowMs / 1000) + 30 * 24 * 3600,
                    metadata: { tenant_id: "tenant_runner_idem_1", tier: "runner_scale" },
                    items: { data: [{ price: { id: "price_runner_scale_r4" } }] },
                },
            },
        };
        // First delivery.
        const req1 = await makeStripeRequest(event, TEST_SECRET, nowMs);
        expect((await handleStripeWebhook(req1, baseEnv(db), fakeCtx())).status).toBe(200);
        // Redelivery (SAME event id, later signature timestamp) — writes re-run.
        const req2 = await makeStripeRequest(event, TEST_SECRET, nowMs + 1000);
        expect((await handleStripeWebhook(req2, baseEnv(db), fakeCtx())).status).toBe(200);

        const seeds = db.runCalls.filter((c) => c.sql.includes("INSERT INTO runners_entitlement"));
        expect(seeds).toHaveLength(2); // idempotent ON CONFLICT upsert ran both times
        // The seed SQL is a guarded ON CONFLICT upsert (safe on redelivery).
        expect(seeds[0]!.sql).toContain("ON CONFLICT(tenant_id) DO UPDATE");
    });
});

// ---------------------------------------------------------------------------
// tierFromSubscriptionPrice / detectTierPriceMismatch — focused coverage
//
// Both helpers are internal to stripe.ts and exercised via handleStripeWebhook.
// The reverse-map tests below complete the 5-tier coverage for the tiers not
// yet pinned in the main suite (starter, team, pro — solo and max are covered
// above). The mismatch tests pin detectTierPriceMismatch with additional tier
// combinations beyond the pro/starter case already in the main suite.
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
