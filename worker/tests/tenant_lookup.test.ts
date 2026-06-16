/**
 * Unit tests for POST /internal/v1/auth/tenant/lookup — githugr authz #3.
 *
 * Server-to-server tenant resolution: githugr's www server presents the shared
 * Clerk `sub` (clerk_user_id) + the shared internal-auth secret, and gets back
 * the CoreLink tenant it owns. Keyed on `sub` (ratified GREENLIGHT 2026-06-15);
 * fail-CLOSED 404 when no tenant maps to the subject.
 */

import { describe, it, expect } from "vitest";
import type { D1Database } from "@cloudflare/workers-types";
import workerHandler from "../src/index.js";
import type { Env } from "../src/index.js";

const INTERNAL_KEY = "test-internal-auth-key-0123456789"; // ≥32 chars

function makeCtx(): ExecutionContext {
  return {
    waitUntil: (_p: Promise<unknown>) => {},
    passThroughOnException: () => {},
  } as unknown as ExecutionContext;
}

interface Row {
  tenant_id: string;
  tier: string | null;
  tenant_state: string | null;
}

/** CONFIG_DB mock: returns the full row for a known clerk_user_id, else null.
 * `throwOnQuery` models a D1 backend fault. */
function makeConfigDb(rows: Map<string, Row>, throwOnQuery = false): D1Database {
  return {
    prepare: (_sql: string) => ({
      bind: (...args: unknown[]) => ({
        first: async <T>() => {
          if (throwOnQuery) {
            throw new Error("d1 down");
          }
          const sub = args[0] as string;
          return (rows.get(sub) ?? null) as T | null;
        },
      }),
    }),
  } as unknown as D1Database;
}

function makeEnv(opts: {
  rows?: Map<string, Row>;
  withInternalKey?: boolean;
  shortKey?: boolean;
  throwOnQuery?: boolean;
}): Env {
  const key = opts.shortKey ? "tooshort" : INTERNAL_KEY;
  return {
    ENVIRONMENT: "test",
    CONFIG_DB: makeConfigDb(opts.rows ?? new Map(), opts.throwOnQuery ?? false),
    CORELINK_INTERNAL_AUTH_KEY: opts.withInternalKey === false ? undefined : key,
  } as Env;
}

function lookupFetch(
  env: Env,
  opts: { auth?: string; body?: unknown; method?: string } = {},
): Promise<Response> {
  const headers: Record<string, string> = { "Content-Type": "application/json" };
  if (opts.auth !== undefined) {
    headers["x-corelink-internal-auth"] = opts.auth;
  }
  const method = opts.method ?? "POST";
  const init: RequestInit = { method, headers };
  if (method === "POST") {
    init.body = opts.body !== undefined ? JSON.stringify(opts.body) : JSON.stringify({ sub: "user_x" });
  }
  const req = new Request("http://localhost/internal/v1/auth/tenant/lookup", init);
  return workerHandler.fetch!(req, env, makeCtx());
}

describe("POST /internal/v1/auth/tenant/lookup — githugr authz #3", () => {
  it("resolves sub → { tenant_id, role:'owner', tier, tenant_state }", async () => {
    const env = makeEnv({
      rows: new Map([["user_abc", { tenant_id: "acme", tier: "pro", tenant_state: "active" }]]),
    });
    const resp = await lookupFetch(env, { auth: INTERNAL_KEY, body: { sub: "user_abc" } });
    expect(resp.status).toBe(200);
    const body = (await resp.json()) as Record<string, unknown>;
    expect(body["tenant_id"]).toBe("acme");
    expect(body["role"]).toBe("owner");
    expect(body["tier"]).toBe("pro");
    expect(body["tenant_state"]).toBe("active");
  });

  it("defaults tier to 'free' when the column is unset", async () => {
    const env = makeEnv({
      rows: new Map([["user_free", { tenant_id: "t-free", tier: null, tenant_state: "active" }]]),
    });
    const resp = await lookupFetch(env, { auth: INTERNAL_KEY, body: { sub: "user_free" } });
    expect(resp.status).toBe(200);
    const body = (await resp.json()) as Record<string, unknown>;
    expect(body["tier"]).toBe("free");
  });

  it("fail-CLOSED 404 when no tenant maps to the subject", async () => {
    const env = makeEnv({ rows: new Map() });
    const resp = await lookupFetch(env, { auth: INTERNAL_KEY, body: { sub: "user_unknown" } });
    expect(resp.status).toBe(404);
    const body = (await resp.json()) as Record<string, unknown>;
    expect(body["error"]).toBe("NOT_FOUND");
  });

  it("401 when the internal-auth header is missing", async () => {
    const env = makeEnv({ rows: new Map([["user_abc", { tenant_id: "acme", tier: "pro", tenant_state: "active" }]]) });
    const resp = await lookupFetch(env, { body: { sub: "user_abc" } }); // no auth header
    expect(resp.status).toBe(401);
  });

  it("401 when the internal-auth header is wrong", async () => {
    const env = makeEnv({ rows: new Map([["user_abc", { tenant_id: "acme", tier: "pro", tenant_state: "active" }]]) });
    const resp = await lookupFetch(env, { auth: "wrong-secret-but-32-chars-padding!", body: { sub: "user_abc" } });
    expect(resp.status).toBe(401);
  });

  it("fail-CLOSED 403 when CORELINK_INTERNAL_AUTH_KEY is unbound", async () => {
    const env = makeEnv({ withInternalKey: false });
    const resp = await lookupFetch(env, { auth: INTERNAL_KEY, body: { sub: "user_abc" } });
    expect(resp.status).toBe(403);
  });

  it("fail-CLOSED 403 when the bound secret is too short (< 32)", async () => {
    const env = makeEnv({ shortKey: true });
    // Caller supplies the (short) key — still 403 because the gate refuses an
    // undersized secret regardless of the provided value.
    const resp = await lookupFetch(env, { auth: "tooshort", body: { sub: "user_abc" } });
    expect(resp.status).toBe(403);
  });

  it("400 when sub is absent (email fallback is N/A — see module doc)", async () => {
    const env = makeEnv({ rows: new Map() });
    const resp = await lookupFetch(env, { auth: INTERNAL_KEY, body: { email: "a@b.com" } });
    expect(resp.status).toBe(400);
  });

  it("405 on a non-POST method", async () => {
    const env = makeEnv({ rows: new Map() });
    const resp = await lookupFetch(env, { auth: INTERNAL_KEY, method: "GET", body: { sub: "user_abc" } });
    expect(resp.status).toBe(405);
  });

  it("500 (fail-CLOSED) on a D1 backend fault", async () => {
    const env = makeEnv({ rows: new Map(), throwOnQuery: true });
    const resp = await lookupFetch(env, { auth: INTERNAL_KEY, body: { sub: "user_abc" } });
    expect(resp.status).toBe(500);
  });
});
