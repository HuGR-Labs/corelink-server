/**
 * Tests for the `AnalyticsIngest` RPC entrypoint (service-binding path).
 *
 * The property under test is the TRUST SPLIT, from both directions:
 *   - an RPC caller (a Worker holding the service binding) may write
 *     `SERVER_ONLY_EVENT_NAMES` with NO `INGEST_KEY` bound anywhere, and
 *   - a browser-shaped HTTP caller still may NOT — the `Origin` path keeps
 *     rejecting server-only events exactly as before.
 * Plus: the two surfaces must produce BYTE-IDENTICAL writes, because they share
 * one validator + one `INSERT OR IGNORE` statement (`ingestEvents`).
 *
 * The fake D1 below emulates the real uniqueness constraint — `id TEXT NOT NULL
 * PRIMARY KEY` is the table's ONLY one (migrations/0001_create_analytics_events.sql:7)
 * — so the idempotency assertion tests `INSERT OR IGNORE` semantics rather than
 * just counting calls.
 */

import { describe, expect, it } from "vitest";
import { AnalyticsIngest } from "../src/index";
import { handleIngest } from "../src/ingest";
import type { Env, EventPayload } from "../src/types";

const ALLOWED = "https://humangr.com";
const KEY = "k".repeat(40);

/** A server-only event name (apps/analytics-worker/src/ingest.ts:51). */
const SERVER_ONLY = "first_cli_authed";

interface Harness {
    env: Env;
    /** Every row handed to `batch()`, in bind order. */
    binds: unknown[][];
    /** Rows that actually survived the PRIMARY KEY (INSERT OR IGNORE). */
    table: Map<string, unknown[]>;
    /** Every SQL string passed to `prepare()`. */
    sql: string[];
    /** Set to make `batch()` reject, simulating a D1 fault. */
    fail: { on: boolean };
}

function makeEnv(opts: { ingestKey?: string } = {}): Harness {
    const binds: unknown[][] = [];
    const table = new Map<string, unknown[]>();
    const sql: string[] = [];
    const fail = { on: false };
    const db = {
        prepare: (statement: string) => {
            sql.push(statement);
            return { bind: (...args: unknown[]) => ({ __args: args }) };
        },
        batch: async (stmts: Array<{ __args: unknown[] }>) => {
            if (fail.on) throw new Error("D1_UNAVAILABLE");
            for (const s of stmts) {
                binds.push(s.__args);
                const id = String(s.__args[0]);
                // `INSERT OR IGNORE` + `id` PRIMARY KEY ⇒ re-insert is a no-op.
                if (!table.has(id)) table.set(id, s.__args);
            }
            return [];
        },
    };
    const env = {
        ANALYTICS_DB: db as unknown as D1Database,
        ALLOWED_ORIGINS: ALLOWED,
        ...(opts.ingestKey === undefined ? {} : { INGEST_KEY: opts.ingestKey }),
    } as unknown as Env;
    return { env, binds, table, sql, fail };
}

/** An `ExecutionContext` stand-in — the entrypoint never uses it, but the base
 *  class constructor takes one, so the shape must be supplied. */
const CTX = {
    waitUntil: () => {},
    passThroughOnException: () => {},
    props: {},
} as unknown as ExecutionContext;

function rpc(env: Env): AnalyticsIngest {
    return new AnalyticsIngest(CTX, env);
}

function post(body: unknown, headers: Record<string, string>): Request {
    return new Request("https://corelink-analytics.humangr.com/v1/event", {
        method: "POST",
        headers: { "Content-Type": "application/json", ...headers },
        body: JSON.stringify(body),
    });
}

const evt = (over: Partial<EventPayload> = {}): EventPayload =>
    ({
        id: `${SERVER_ONLY}:tenant-1`,
        event_name: SERVER_ONLY,
        tenant_id: "tenant-1",
        properties: { surface: "cli" },
        ...over,
    }) as EventPayload;

describe("AnalyticsIngest RPC — trusted by construction, no shared secret", () => {
    it("accepts a SERVER-ONLY event with NO ingest key bound anywhere", async () => {
        // No INGEST_KEY in env at all: this is the operator step being removed.
        const h = makeEnv();
        expect(h.env.INGEST_KEY).toBeUndefined();

        const res = await rpc(h.env).ingestServerEvent(evt());

        expect(res).toEqual({ accepted: 1, rejected: 0, errors: [] });
        expect(h.table.size).toBe(1);
        expect(h.table.get(`${SERVER_ONLY}:tenant-1`)?.[1]).toBe(SERVER_ONLY);
    });

    it("uses the INSERT OR IGNORE statement (not a plain INSERT)", async () => {
        const h = makeEnv();
        await rpc(h.env).ingestServerEvent(evt());
        expect(h.sql[0]).toContain("INSERT OR IGNORE INTO analytics_events");
    });

    it("leaves created_at NULL when omitted, so ingest stamps the server clock", async () => {
        const h = makeEnv();
        await rpc(h.env).ingestServerEvent(evt());
        // Bind order: id, event_name, tenant_id, user_id, session_id, properties, created_at
        expect(h.binds[0]?.[6]).toBeNull();
    });

    it("passes an explicit created_at straight through", async () => {
        const h = makeEnv();
        await rpc(h.env).ingestServerEvent(evt({ created_at: "2026-08-01T00:00:00.000Z" }));
        expect(h.binds[0]?.[6]).toBe("2026-08-01T00:00:00.000Z");
    });
});

describe("AnalyticsIngest RPC — idempotency is the PRIMARY KEY", () => {
    it("coalesces a re-sent deterministic id into ONE row", async () => {
        const h = makeEnv();
        const first = await rpc(h.env).ingestServerEvent(evt());
        const second = await rpc(h.env).ingestServerEvent(evt());

        // Both calls report acceptance (ingest does not read back) …
        expect(first.accepted).toBe(1);
        expect(second.accepted).toBe(1);
        // … but the table holds exactly one row: OR IGNORE dropped the dup.
        expect(h.binds).toHaveLength(2);
        expect(h.table.size).toBe(1);
    });

    it("keeps distinct ids as distinct rows", async () => {
        const h = makeEnv();
        await rpc(h.env).ingestServerEvent(evt({ id: `${SERVER_ONLY}:a`, tenant_id: "a" }));
        await rpc(h.env).ingestServerEvent(evt({ id: `${SERVER_ONLY}:b`, tenant_id: "b" }));
        expect(h.table.size).toBe(2);
    });
});

describe("AnalyticsIngest RPC — the SAME validation as the HTTP path", () => {
    it("enforces the 64-char id cap", async () => {
        const h = makeEnv();
        const ok = await rpc(h.env).ingestServerEvent(evt({ id: "x".repeat(64) }));
        expect(ok.accepted).toBe(1);

        const tooLong = await rpc(h.env).ingestServerEvent(evt({ id: "x".repeat(65) }));
        expect(tooLong).toMatchObject({ accepted: 0, rejected: 1 });
        expect(tooLong.errors[0]?.reason).toBe("invalid_id");
        expect(h.table.size).toBe(1);
    });

    it("rejects an empty id", async () => {
        const h = makeEnv();
        const res = await rpc(h.env).ingestServerEvent(evt({ id: "" }));
        expect(res.errors[0]?.reason).toBe("invalid_id");
        expect(h.table.size).toBe(0);
    });

    it("enforces the 128-char tenant_id cap", async () => {
        const h = makeEnv();
        const ok = await rpc(h.env).ingestServerEvent(
            evt({ id: "cap-ok", tenant_id: "t".repeat(128) }),
        );
        expect(ok.accepted).toBe(1);

        const tooLong = await rpc(h.env).ingestServerEvent(
            evt({ id: "cap-bad", tenant_id: "t".repeat(129) }),
        );
        expect(tooLong.errors[0]?.reason).toBe("invalid_tenant_id");
        expect(h.table.size).toBe(1);
    });

    it("enforces the privacy gate on properties (no email / IP at the edge)", async () => {
        const h = makeEnv();
        for (const forbidden of ["email", "ip", "ip_address", "remote_addr"]) {
            const res = await rpc(h.env).ingestServerEvent(
                evt({ id: `priv-${forbidden}`, properties: { [forbidden]: "x" } }),
            );
            expect(res.accepted, forbidden).toBe(0);
            expect(res.errors[0]?.reason, forbidden).toBe(`forbidden_field:${forbidden}`);
        }
        expect(h.table.size).toBe(0);
    });

    it("rejects an event name outside the closed allow-list", async () => {
        const h = makeEnv();
        const res = await rpc(h.env).ingestServerEvent(
            evt({ event_name: "totally_made_up" as EventPayload["event_name"] }),
        );
        expect(res.errors[0]?.reason).toBe("unknown_event_name");
        expect(h.table.size).toBe(0);
    });

    it("rejects a non-object payload instead of throwing at the RPC boundary", async () => {
        const h = makeEnv();
        const res = await rpc(h.env).ingestServerEvent(null as unknown as EventPayload);
        expect(res).toMatchObject({ accepted: 0, rejected: 1 });
        expect(res.errors[0]?.reason).toBe("not_an_object");
    });

    it("absorbs a D1 fault — analytics must never break its caller", async () => {
        const h = makeEnv();
        h.fail.on = true;
        const res = await rpc(h.env).ingestServerEvent(evt());
        expect(res.accepted).toBe(0);
        expect(res.rejected).toBe(1);
        expect(res.errors[0]?.reason).toContain("d1_error:");
    });

    it("writes BYTE-IDENTICAL binds to the keyed HTTP path (one shared write path)", async () => {
        const viaRpc = makeEnv();
        await rpc(viaRpc.env).ingestServerEvent(evt());

        const viaHttp = makeEnv({ ingestKey: KEY });
        await handleIngest(post(evt(), { "X-Corelink-Ingest-Key": KEY }), viaHttp.env);

        expect(viaRpc.binds).toEqual(viaHttp.binds);
        expect(viaRpc.sql[0]).toBe(viaHttp.sql[0]);
    });
});

describe("server-only stays server-only — a browser still cannot forge it", () => {
    it("REJECTS the server-only event on the browser Origin path (no key)", async () => {
        const h = makeEnv({ ingestKey: KEY });
        const res = await handleIngest(post(evt(), { Origin: ALLOWED }), h.env);
        const json = (await res.json()) as {
            accepted: number;
            rejected: number;
            errors: Array<{ reason: string }>;
        };
        expect(res.status).toBe(200);
        expect(json.accepted).toBe(0);
        expect(json.errors[0]?.reason).toBe("server_only_event");
        expect(h.table.size).toBe(0);
    });

    it("REJECTS it for an attacker guessing the key from an allowed Origin", async () => {
        const h = makeEnv({ ingestKey: KEY });
        const res = await handleIngest(
            post(evt(), { Origin: ALLOWED, "X-Corelink-Ingest-Key": "k".repeat(39) + "z" }),
            h.env,
        );
        const json = (await res.json()) as { errors: Array<{ reason: string }> };
        expect(json.errors[0]?.reason).toBe("server_only_event");
        expect(h.table.size).toBe(0);
    });

    it("403s a keyless, origin-less HTTP caller (the RPC shape over HTTP)", async () => {
        const h = makeEnv({ ingestKey: KEY });
        const res = await handleIngest(post(evt(), {}), h.env);
        expect(res.status).toBe(403);
        expect(h.table.size).toBe(0);
    });

    it("still 403s when NO key is configured — absent key never means 'trusted'", async () => {
        // Same env the RPC path is happy with (no INGEST_KEY): the HTTP surface
        // must not silently downgrade to open ingest just because the key is gone.
        const h = makeEnv();
        const res = await handleIngest(post(evt(), { "X-Corelink-Ingest-Key": "" }), h.env);
        expect(res.status).toBe(403);
        expect(h.table.size).toBe(0);
    });
});

describe("AnalyticsIngest — deploy-surface invariants", () => {
    it("is exported from the wrangler `main` module so `entrypoint =` can resolve it", async () => {
        const mod = await import("../src/index");
        expect(typeof mod.AnalyticsIngest).toBe("function");
        expect(typeof mod.AnalyticsIngest.prototype.ingestServerEvent).toBe("function");
    });

    it("exposes NO fetch method — the RPC class must stay HTTP-unreachable", () => {
        // The `trusted = true` argument depends on this class never being
        // reachable by an untrusted caller. A `fetch()` here would break it.
        expect((AnalyticsIngest.prototype as unknown as { fetch?: unknown }).fetch).toBeUndefined();
    });

    it("does not disturb the default fetch export", async () => {
        const mod = await import("../src/index");
        expect(typeof (mod.default as ExportedHandler<Env>).fetch).toBe("function");
        expect(typeof (mod.default as ExportedHandler<Env>).scheduled).toBe("function");
    });
});
