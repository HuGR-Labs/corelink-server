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

});
