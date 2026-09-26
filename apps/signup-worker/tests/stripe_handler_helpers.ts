/** Shared fixtures for Stripe webhook handler tests. */
import { vi, type Mock } from "vitest";
import type { StripeWebhookEnv } from "../src/webhooks/stripe.js";

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
export const TEST_SECRET = "whsec_" + btoa(String.fromCharCode(...new Array(32).fill(0)));
export function fakeCtx(): ExecutionContext {
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
export function fakeDb(): {
    prepare: Mock;
    batch: Mock;
    runCalls: Array<{ sql: string; params: unknown[] }>;
} {
    const runCalls: Array<{ sql: string; params: unknown[] }> = [];
    // Durable mirror of the dedup PK: event_ids already claimed.
    const claimedEventIds = new Set<string>();
    // Staging ownership rows have a separate unique receipt fence.
    const ownedReceipts = new Set<string>();

    const prepare = vi.fn((sql: string) => {
        const params: unknown[] = [];
        const stmt = {
            sql,
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
                } else if (sql.includes("INSERT INTO staging_load_test_resources") && sql.includes("VALUES")) {
                    const receiptKey = params.slice(0, 4).join("\0");
                    if (ownedReceipts.has(receiptKey)) {
                        changes = 0;
                    } else {
                        ownedReceipts.add(receiptKey);
                    }
                }
                return { meta: { changes } };
            }),
        };
        return stmt;
    });
    const batch = vi.fn(async (statements: Array<{ run(): Promise<unknown> }>) => {
        if (statements.length === 3) {
            for (const statement of statements) await statement.run();
            // Runner entitlement CAS relies on a RETURNING row from the fence.
            return [{ results: [{}] }, {}, { results: [{}] }];
        }
        const previousClaims = new Set(claimedEventIds);
        const previousReceipts = new Set(ownedReceipts);
        const initialCallCount = runCalls.length;
        const results: unknown[] = [];
        let priorChanges = 1;
        try {
            for (const statement of statements) {
                const candidate = statement as { sql?: string; run(): Promise<{ meta?: { changes?: number } }> };
                if (candidate.sql?.includes("WHERE changes() = 0") && priorChanges === 0) {
                    throw new Error("ownership replay conflict");
                }
                const result = await statement.run() as { meta?: { changes?: number } };
                priorChanges = result.meta?.changes ?? 0;
                results.push(result);
            }
            return results;
        } catch (error) {
            claimedEventIds.clear();
            for (const id of previousClaims) claimedEventIds.add(id);
            ownedReceipts.clear();
            for (const receipt of previousReceipts) ownedReceipts.add(receipt);
            runCalls.splice(initialCallCount);
            throw error;
        }
    });
    return { prepare, batch, runCalls };
}

/**
 * Variant fake DB whose idempotency claim INSERT always THROWS (D1 error),
 * exercising the fail-safe claim-then-process path: the handler must proceed to
 * dispatch rather than drop a possible first-time event. All other statements
 * behave normally.
 */
export function fakeDbClaimThrows(): {
    prepare: Mock;
    batch: Mock;
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
    const batch = vi.fn(async (statements: Array<{ run(): Promise<unknown> }>) => {
        const results = [];
        for (const statement of statements) results.push(await statement.run());
        return results;
    });
    return { prepare, batch, runCalls };
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
export function fakeDbFailableWrite(failWhile: (sql: string) => boolean): {
    prepare: Mock;
    batch: Mock;
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
    const batch = vi.fn(async (statements: Array<{ run(): Promise<unknown> }>) => {
        const results = [];
        for (const statement of statements) results.push(await statement.run());
        return results;
    });
    return { prepare, batch, runCalls };
}

/** Build a signed Stripe webhook Request. */
export async function makeStripeRequest(
    event: Record<string, unknown>,
    secret: string,
    nowMs?: number,
): Promise<Request> {
    const created = Math.floor((nowMs ?? Date.now()) / 1000);
    // Stripe always supplies these provider timestamps. Fixtures that are about
    // another behavior inherit realistic values unless they pin one explicitly.
    if (typeof event.created !== "number") event.created = created;
    if (typeof event.type === "string" && event.type.startsWith("customer.subscription.")) {
        const data = event.data as { object?: Record<string, unknown> } | undefined;
        if (data?.object && typeof data.object.created !== "number") data.object.created = created;
    }
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

export const baseEnv = (db?: ReturnType<typeof fakeDb>): StripeWebhookEnv => ({
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
        ? { prepare: db.prepare, batch: db.batch }
        : undefined,
});
