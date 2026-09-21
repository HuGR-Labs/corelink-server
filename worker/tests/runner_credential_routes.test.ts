import { describe, expect, it } from "vitest";
import type { D1Database, DurableObjectNamespace, DurableObjectState, DurableObjectStorage } from "@cloudflare/workers-types";
import type { Env } from "../src/index.js";
import {
  handleRunnerAdopt,
  handleRunnerPrepare,
  prepareRunnerCredential,
} from "../src/lib/runner_credential_routes.js";
import { matchRoute } from "../src/route_match.js";
import { CoreLinkServer } from "../src/durable_object.js";

const KEY = "runner-route-test-key-0123456789abcdef";
const OP = "11111111-1111-4111-8111-111111111111";
const TENANT_V4 = "22222222-2222-4222-8222-222222222222";
const TENANT_V7 = "018f48a8-2c08-7f7e-8a1d-2c3d4e5f6071";
const operation = { operationId: OP, tenantId: TENANT_V4, jobId: "job-1", repo: "acme/repo", lifecycleGeneration: "1" };

function request(path: string, body: unknown, method = "POST", auth = KEY): Request {
  return new Request(`https://worker.test${path}`, {
    method,
    headers: { "content-type": "application/json", "x-corelink-internal-auth": auth },
    body: method === "POST" ? JSON.stringify(body) : undefined,
  });
}

function namespace(response = new Response(null, { status: 204 }), captured?: { request?: Request }): DurableObjectNamespace {
  return {
    idFromName: () => ({}),
    get: () => ({ fetch: async (req: Request) => { captured && (captured.request = req); return response; } }),
  } as unknown as DurableObjectNamespace;
}

function env(db: D1Database, captured?: { request?: Request }, key = KEY): Env {
  return { CONFIG_DB: db, CORELINK_SERVER: namespace(undefined, captured), CORELINK_RUNNER_MINT_AUTH_KEY: key } as unknown as Env;
}

function db(mode: "ok" | "conflict" | "error"): D1Database {
  return {
    prepare: () => ({
      bind: () => ({
        run: async () => {
          if (mode === "error") throw new Error("db down");
          return { success: true, meta: { changes: mode === "ok" ? 1 : 0 } };
        },
        first: async () => null,
      }),
    }),
  } as unknown as D1Database;
}

describe("runner credential handoff routes", () => {
  it("dispatches the private prepare path through the actual DO fetch entry point", async () => {
    const storage = {
      get: async () => undefined, list: async () => new Map(), getAlarm: async () => null, setAlarm: async () => {}, put: async () => {},
      transaction: async (fn: (txn: DurableObjectStorage) => Promise<unknown>) => fn(storage as unknown as DurableObjectStorage),
    } as unknown as DurableObjectStorage;
    const state = {
      id: { equals: () => true, toString: () => "system" }, storage,
      blockConcurrencyWhile: async (fn: () => Promise<void>) => fn(),
    } as unknown as DurableObjectState;
    const response = await new CoreLinkServer(state, env(db("ok"))).fetch(request("/_do/runner-cleanup/prepare", operation));
    expect(response.status).toBe(204);
  });

  it("matches the dispatcher adoption endpoint to its Worker handler", () => {
    expect(matchRoute(new URL("https://worker.test/internal/v1/runner/adopt")).routeKind).toBe("runner_adopt");
  });

  it("forwards prepare only with the runner auth key and frozen body", async () => {
    const captured: { request?: Request } = {};
    const ok = await prepareRunnerCredential(env(db("ok"), captured), "req-1", operation);
    expect(ok).toBe(true);
    expect(captured.request?.method).toBe("POST");
    await expect(captured.request?.json()).resolves.toEqual({
      operationId: OP, tenantId: TENANT_V4, jobId: "job-1", repo: "acme/repo", lifecycleGeneration: "1",
    });
  });

  it.each([
    ["GET", 403],
    ["POST", 204],
  ] as const)("private prepare route handles method %s", async (method, status) => {
    const state = { id: { equals: () => true } } as unknown as DurableObjectState;
    const storage = {
      transaction: async (fn: (txn: DurableObjectStorage) => Promise<void>) => fn({
        list: async () => new Map(), get: async () => undefined, getAlarm: async () => null, setAlarm: async () => {}, put: async () => {},
      } as unknown as DurableObjectStorage),
    } as unknown as DurableObjectStorage;
    const response = await handleRunnerPrepare(request("/_do/runner-cleanup/prepare", operation, method), env(db("ok")), state, storage, "req-2");
    expect(response.status).toBe(status);
  });

  it("private prepare rejects a non-system DO id", async () => {
    const state = { id: { equals: () => false } } as unknown as DurableObjectState;
    const response = await handleRunnerPrepare(request("/_do/runner-cleanup/prepare", operation), env(db("ok")), state, {} as DurableObjectStorage, "req-3");
    expect(response.status).toBe(403);
  });

  it("rejects missing auth and malformed private prepare requests before storage", async () => {
    const state = { id: { equals: () => true } } as unknown as DurableObjectState;
    const response = await handleRunnerPrepare(
      new Request("https://worker.test/_do/runner-cleanup/prepare", { method: "POST" }),
      env(db("error")),
      state,
      {} as DurableObjectStorage,
      "req-auth",
    );
    expect(response.status).toBe(401);
    const malformed = new Request("https://worker.test/_do/runner-cleanup/prepare", {
      method: "POST",
      headers: { "x-corelink-internal-auth": KEY },
      body: "{",
    });
    expect((await handleRunnerPrepare(malformed, env(db("error")), state, {} as DurableObjectStorage, "req-json")).status).toBe(400);
  });

  it.each(["abc", "01", "-1", "9223372036854775808"])("rejects non-canonical generation %s", async (generation) => {
    const state = { id: { equals: () => true } } as unknown as DurableObjectState;
    const response = await handleRunnerPrepare(request("/_do/runner-cleanup/prepare", { ...operation, lifecycleGeneration: generation }), env(db("error")), state, {} as DurableObjectStorage, "req-generation");
    expect(response.status).toBe(400);
  });

  it.each([TENANT_V7, "22222222-2222-1222-8222-222222222222"])("accepts v7 and rejects noncanonical tenant %s at ingress", async (tenantId) => {
    const state = { id: { equals: () => true } } as unknown as DurableObjectState;
    let touchedStorage = false;
    const storage = {
      transaction: async (fn: (txn: DurableObjectStorage) => Promise<unknown>) => {
        touchedStorage = true;
        return fn({ list: async () => new Map(), get: async () => undefined, getAlarm: async () => null, setAlarm: async () => {}, put: async () => {} } as unknown as DurableObjectStorage);
      },
    } as unknown as DurableObjectStorage;
    const response = await handleRunnerPrepare(request("/_do/runner-cleanup/prepare", { ...operation, tenantId }), env(db("ok")), state, storage, "req-tenant");
    expect(response.status).toBe(tenantId === TENANT_V7 ? 204 : 400);
    expect(touchedStorage).toBe(tenantId === TENANT_V7);
  });

  it("adopt returns 204, 409, and 503 without a body", async () => {
    const body = { operation_id: OP, pat_id: "pat-1" };
    const accepted = await handleRunnerAdopt(request("/internal/v1/runner/adopt", body), env(db("ok")), "req-4");
    expect(accepted.status).toBe(204);
    expect(await accepted.text()).toBe("");
    expect((await handleRunnerAdopt(request("/internal/v1/runner/adopt", body), env(db("conflict")), "req-5")).status).toBe(409);
    expect((await handleRunnerAdopt(request("/internal/v1/runner/adopt", body), env(db("error")), "req-6")).status).toBe(503);
  });

  it("adopt rejects malformed body before D1", async () => {
    expect((await handleRunnerAdopt(request("/internal/v1/runner/adopt", { operation_id: "bad", pat_id: "pat" }), env(db("ok")), "req-7")).status).toBe(400);
  });

  it("adopt enforces auth and POST before D1", async () => {
    expect((await handleRunnerAdopt(new Request("https://worker.test/internal/v1/runner/adopt", { method: "POST" }), env(db("error")), "req-adopt-auth")).status).toBe(401);
    expect((await handleRunnerAdopt(request("/internal/v1/runner/adopt", {}, "GET"), env(db("error")), "req-adopt-method")).status).toBe(405);
  });
});
