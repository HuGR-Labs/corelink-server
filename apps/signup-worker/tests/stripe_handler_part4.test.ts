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
});
