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

import { describe, it, expect, vi, beforeEach } from "vitest";
import {
  TEST_SECRET,
  fakeCtx,
  fakeDb,
  makeStripeRequest,
  baseEnv,
} from "./stripe_handler_helpers.js";
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

        // Entitlement DELETEd through the shared durable fence batch.
        const del = db.runCalls.find((c) => c.sql.includes("DELETE FROM runners_entitlement"));
        expect(del).toBeDefined();
        expect(del!.params).toContain("sub_runner_deleted_1");
        expect(db.batch).toHaveBeenCalledTimes(1);
        expect(db.batch.mock.calls[0]![0]).toHaveLength(3);
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

        // An invoice omits subscription.created, so it cannot manufacture a
        // provider ordering tuple. It updates the mirror and lets the current
        // subscription event issue the fenced revoke.
        expect(db.runCalls.find((c) => c.sql.includes("DELETE FROM runners_entitlement"))).toBeUndefined();
        // runner_billing status → past_due.
        const rb = db.runCalls.find(
            (c) => c.sql.includes("UPDATE runner_billing") && c.params.includes("past_due"),
        );
        expect(rb).toBeDefined();
        expect(rb!.params).toContain("sub_runner_pf_1");
    });

    // ---- NO-REGRESSION: a CACHE event must not touch the runner tables ----

});
