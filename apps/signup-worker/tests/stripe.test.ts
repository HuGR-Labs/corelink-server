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

    it("checkout.session.completed without tenant_id metadata → 200, no billing/tier write", async () => {
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
        // No billing/tier mutation (no tenant_id to key on). The dedup claim
        // row is written independently of tenant_id, so we assert the absence
        // of the billing/tier writes specifically rather than zero D1 calls.
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
                    metadata: { tenant_id: "tenant_idem", plan: "starter" },
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
                    metadata: { tenant_id: "tenant_dedup", plan: "starter" },
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
                    metadata: { tenant_id: "tenant_retry", plan: "starter" },
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
                    metadata: { tenant_id: "tenant_claim_err", plan: "starter" },
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
});
