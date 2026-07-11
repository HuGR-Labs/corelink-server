/**
 * Tests for the ingest trust-split hardening (backend audit finding, 2026-07-11).
 *
 * The `Origin` header is attacker-controllable outside a browser, so the Origin
 * (browser) path is ANONYMOUS ingest. It must NOT be able to:
 *   - assert revenue/provisioning-truth events (server_only_event), or
 *   - batch >1 event/request (D1 write amplification).
 * The keyed server path (X-Corelink-Ingest-Key) retains full capability.
 */

import { describe, expect, it } from "vitest";
import { handleIngest } from "../src/ingest";
import type { Env } from "../src/types";

const ALLOWED = "https://corelink-admin.humangr.com";
const KEY = "k".repeat(40);

function makeEnv(): { env: Env; rows: unknown[][] } {
    const rows: unknown[][] = [];
    const stmt = { bind: (...args: unknown[]) => ({ __args: args }) };
    const db = {
        prepare: (_sql: string) => stmt,
        batch: async (stmts: Array<{ __args: unknown[] }>) => {
            for (const s of stmts) rows.push(s.__args);
            return [];
        },
    };
    const env = {
        ANALYTICS_DB: db as unknown as D1Database,
        ALLOWED_ORIGINS: ALLOWED,
        INGEST_KEY: KEY,
    } as unknown as Env;
    return { env, rows };
}

function post(body: unknown, headers: Record<string, string>): Request {
    return new Request("https://corelink-analytics.humangr.com/v1/event", {
        method: "POST",
        headers: { "Content-Type": "application/json", ...headers },
        body: JSON.stringify(body),
    });
}

const evt = (event_name: string, extra: Record<string, unknown> = {}) => ({
    id: `id-${event_name}-${Object.keys(extra).length}`,
    event_name,
    session_id: "sess-1",
    ...extra,
});

describe("ingest trust-split (anonymous Origin path)", () => {
    it("accepts a front-of-funnel event from an allowed Origin (no key)", async () => {
        const { env, rows } = makeEnv();
        const res = await handleIngest(post(evt("signup_started"), { Origin: ALLOWED }), env);
        const json = (await res.json()) as { accepted: number; rejected: number };
        expect(res.status).toBe(200);
        expect(json.accepted).toBe(1);
        expect(rows).toHaveLength(1);
    });

    it("REJECTS a forged revenue event from the browser path (server_only_event)", async () => {
        const { env, rows } = makeEnv();
        const res = await handleIngest(
            post(evt("paid_subscription_started", { tenant_id: "victim-tenant" }), { Origin: ALLOWED }),
            env,
        );
        const json = (await res.json()) as { accepted: number; rejected: number; errors: Array<{ reason: string }> };
        expect(json.accepted).toBe(0);
        expect(json.rejected).toBe(1);
        expect(json.errors[0]?.reason).toBe("server_only_event");
        expect(rows).toHaveLength(0);
    });

    it("REJECTS other provisioning-truth forgeries from the browser path", async () => {
        const { env } = makeEnv();
        for (const name of ["tenant_created", "pat_issued", "plan_downgraded", "subscription_canceled"]) {
            const res = await handleIngest(post(evt(name), { Origin: ALLOWED }), env);
            const json = (await res.json()) as { accepted: number; errors: Array<{ reason: string }> };
            expect(json.accepted, name).toBe(0);
            expect(json.errors[0]?.reason, name).toBe("server_only_event");
        }
    });

    it("caps the anonymous path to 1 event/request (no D1 amplification)", async () => {
        const { env } = makeEnv();
        const batch = { events: [evt("signup_started", { a: 1 }), evt("pricing_view", { b: 2 })] };
        const res = await handleIngest(post(batch, { Origin: ALLOWED }), env);
        const json = (await res.json()) as { error?: string };
        expect(res.status).toBe(400);
        expect(json.error).toBe("invalid_batch_size");
    });

    it("rejects requests with neither an allowed Origin nor a key (403)", async () => {
        const { env } = makeEnv();
        const res = await handleIngest(post(evt("signup_started"), { Origin: "https://evil.example.com" }), env);
        expect(res.status).toBe(403);
    });
});

describe("ingest trust-split (trusted keyed server path)", () => {
    it("accepts revenue events when the ingest key is presented", async () => {
        const { env, rows } = makeEnv();
        const res = await handleIngest(
            post(evt("paid_subscription_started", { tenant_id: "t1" }), { "X-Corelink-Ingest-Key": KEY }),
            env,
        );
        const json = (await res.json()) as { accepted: number };
        expect(json.accepted).toBe(1);
        expect(rows).toHaveLength(1);
    });

    it("allows batching up to 100 on the keyed path", async () => {
        const { env, rows } = makeEnv();
        const events = Array.from({ length: 5 }, (_, i) => evt("tenant_created", { n: i }));
        const res = await handleIngest(post({ events }, { "X-Corelink-Ingest-Key": KEY }), env);
        const json = (await res.json()) as { accepted: number };
        expect(json.accepted).toBe(5);
        expect(rows).toHaveLength(5);
    });

    it("rejects a wrong key (403), not falling through to anonymous", async () => {
        const { env } = makeEnv();
        const res = await handleIngest(
            post(evt("paid_subscription_started"), { "X-Corelink-Ingest-Key": "wrong-key" }),
            env,
        );
        expect(res.status).toBe(403);
    });
});
