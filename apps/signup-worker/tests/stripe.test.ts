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
import {
    baseEnv,
    fakeCtx,
    fakeDb,
    fakeDbClaimThrows,
    fakeDbFailableWrite,
    makeStripeRequest,
} from "./stripe_handler_helpers.js";

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
const STAGING_ADMISSION_KEY = "01234567890123456789012345678901";
const STAGING_REQUEST_ID = "b".repeat(32);
const STAGING_DOMAIN = "corelink/staging-ownership-envelope/v1\0";

async function stagingAdmissionEnvelope(nowMs: number): Promise<string> {
    const payload = `v1.123456.webhook.staging.${"a".repeat(40)}.${nowMs - 1000}.${nowMs + 60_000}.${STAGING_REQUEST_ID}`;
    const key = await crypto.subtle.importKey(
        "raw",
        new TextEncoder().encode(STAGING_ADMISSION_KEY),
        { name: "HMAC", hash: "SHA-256" },
        false,
        ["sign"],
    );
    const tag = new Uint8Array(await crypto.subtle.sign(
        "HMAC",
        key,
        new TextEncoder().encode(STAGING_DOMAIN + payload),
    ));
    return `${payload}.${Array.from(tag, (byte) => byte.toString(16).padStart(2, "0")).join("")}`;
}

async function withStagingAdmission(request: Request, nowMs: number): Promise<Request> {
    const headers = new Headers(request.headers);
    headers.set("x-corelink-staging-load-admission", await stagingAdmissionEnvelope(nowMs));
    return new Request(request, { headers });
}

// ---------------------------------------------------------------------------
// verifyStripeSignature unit tests
// ---------------------------------------------------------------------------

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

    it("specialized D1 fixture batches delegate statements in order and stop on failure", async () => {
        const claimDb = fakeDbClaimThrows();
        const beforeClaim = claimDb.prepare("UPDATE fixture_before_claim");
        const failedClaim = claimDb.prepare(
            "INSERT OR IGNORE INTO stripe_webhook_events_processed VALUES (?1)",
        );
        expect(claimDb.batch).toBeTypeOf("function");
        await expect(claimDb.batch([beforeClaim, failedClaim])).rejects.toThrow("D1_UNAVAILABLE");
        expect(claimDb.runCalls.map((call) => call.sql)).toEqual(["UPDATE fixture_before_claim"]);

        let failWrite = true;
        const writeDb = fakeDbFailableWrite((sql) => failWrite && sql.includes("fixture_failure"));
        const firstWrite = writeDb.prepare("UPDATE fixture_first");
        const failedWrite = writeDb.prepare("UPDATE fixture_failure");
        const laterWrite = writeDb.prepare("UPDATE fixture_later");
        expect(writeDb.batch).toBeTypeOf("function");
        await expect(writeDb.batch([firstWrite, failedWrite, laterWrite])).rejects.toThrow(
            "D1_WRITE_FAILED",
        );
        expect(writeDb.runCalls.map((call) => call.sql)).toEqual(["UPDATE fixture_first"]);

        failWrite = false;
        await writeDb.batch([firstWrite, failedWrite, laterWrite]);
        expect(writeDb.runCalls.map((call) => call.sql)).toEqual([
            "UPDATE fixture_first",
            "UPDATE fixture_first",
            "UPDATE fixture_failure",
            "UPDATE fixture_later",
        ]);
        expect(writeDb.batch).toHaveBeenCalledTimes(2);
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

    it("commits admitted claim, billing writes, and ownership rows in one D1 batch; replay rolls back", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_staging_atomic",
            type: "checkout.session.completed",
            data: {
                object: {
                    customer: "cus_staging_atomic",
                    subscription: "sub_staging_atomic",
                    amount_total: 4900,
                    payment_status: "paid",
                    metadata: { tenant_id: "tenant_staging_atomic", tier: "starter" },
                },
            },
        };
        const env = { ...baseEnv(db), CORELINK_STAGING_LOAD_TEST_ADMISSION_KEY: STAGING_ADMISSION_KEY };
        const first = await withStagingAdmission(await makeStripeRequest(event, TEST_SECRET, nowMs), nowMs);
        expect((await handleStripeWebhook(first, env, fakeCtx())).status).toBe(200);
        expect(db.batch).toHaveBeenCalledTimes(1);
        const statements = db.batch.mock.calls[0]?.[0] as Array<{ sql?: string }>;
        expect(statements.some((statement) => statement.sql?.includes("stripe_webhook_events_processed"))).toBe(true);
        expect(statements.some((statement) => statement.sql?.includes("INSERT INTO tenant_billing"))).toBe(true);
        expect(statements.filter((statement) =>
            statement.sql?.includes("INSERT INTO staging_load_test_resources") && statement.sql.includes("VALUES"),
        )).toHaveLength(4);

        const callsAfterFirst = db.runCalls.length;
        const replay = await withStagingAdmission(await makeStripeRequest(event, TEST_SECRET, nowMs + 1000), nowMs + 1000);
        expect((await handleStripeWebhook(replay, env, fakeCtx())).status).toBe(409);
        expect(db.runCalls).toHaveLength(callsAfterFirst);

        const alteredEvent = { ...event, id: "evt_staging_altered" };
        const altered = await makeStripeRequest(alteredEvent, TEST_SECRET, nowMs + 2000);
        const alteredHeaders = new Headers(altered.headers);
        // Reuse the exact signed admission envelope while changing the Stripe
        // event id. The request-level inbox owner fence must reject it too.
        alteredHeaders.set(
            "x-corelink-staging-load-admission",
            first.headers.get("x-corelink-staging-load-admission")!,
        );
        const alteredWithAdmission = new Request(altered, { headers: alteredHeaders });
        expect((await handleStripeWebhook(alteredWithAdmission, env, fakeCtx())).status).toBe(409);
        expect(db.runCalls).toHaveLength(callsAfterFirst);
    });

    it("rejects a present invalid staging envelope before Stripe billing writes", async () => {
        const db = fakeDb();
        const nowMs = Date.now();
        const event = {
            id: "evt_staging_invalid",
            type: "checkout.session.completed",
            data: { object: { customer: "cus_x", subscription: "sub_x", payment_status: "paid", metadata: { tenant_id: "tenant_x", tier: "starter" } } },
        };
        const request = await withStagingAdmission(await makeStripeRequest(event, TEST_SECRET, nowMs), nowMs);
        const headers = new Headers(request.headers);
        headers.set("x-corelink-staging-load-admission", `${headers.get("x-corelink-staging-load-admission")}0`);
        const invalid = new Request(request, { headers });
        const env = { ...baseEnv(db), CORELINK_STAGING_LOAD_TEST_ADMISSION_KEY: STAGING_ADMISSION_KEY };
        expect((await handleStripeWebhook(invalid, env, fakeCtx())).status).toBe(403);
        expect(db.runCalls).toHaveLength(0);
        expect(db.batch).not.toHaveBeenCalled();
    });

});
// B-126 M3 split population: stripe_handler_part2.test.ts, stripe_handler_part3.test.ts, stripe_handler_part4.test.ts, stripe_signature_part2.test.ts, stripe_contract_part.test.ts
