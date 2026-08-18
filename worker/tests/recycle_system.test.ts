// Operator container force-recycle route — `POST /_internal/admin/recycle-system`.
// Verifies the apex destroys the local `_system` container via the auth-free DO
// management path `/_do/stop` AND fans the recycle to every regional worker so one
// operator call converges the whole fleet onto the current container start-env
// (the lever that applies a rotated start-time secret to an already-running DO
// container, which a container-app rollout does not restart).
import { describe, it, expect } from "vitest";
import workerHandler from "../src/index.js";
import type { Env } from "../src/index.js";

// ≥32 chars — the operator internal-auth floor (INTERNAL_AUTH_KEY_MIN_LEN).
const KEY = "recycle-test-internal-auth-key-0123456789abcdef";

function makeCtx(): ExecutionContext {
  return {
    waitUntil: (_p: Promise<unknown>) => {},
    passThroughOnException: () => {},
  } as unknown as ExecutionContext;
}

function makeRecycleEnv(): {
  env: Env;
  calls: { system: string[]; regions: Record<string, number> };
} {
  const calls = { system: [] as string[], regions: {} as Record<string, number> };
  const systemStub = {
    fetch: async (req: Request): Promise<Response> => {
      calls.system.push(new URL(req.url).pathname);
      return new Response(JSON.stringify({ stopped: true, request_id: "stub" }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      });
    },
  };
  const namespace = {
    idFromName: (_n: string) => ({ toString: () => "id" }),
    get: (_id: unknown) => systemStub,
    idFromString: (_s: string) => ({ toString: () => "id" }),
    newUniqueId: () => ({ toString: () => "u" }),
    jurisdiction: (_j: string) => namespace,
  } as unknown as DurableObjectNamespace;
  const mkRegion = (name: string) => ({
    fetch: async (_req: Request): Promise<Response> => {
      calls.regions[name] = (calls.regions[name] ?? 0) + 1;
      return new Response(
        JSON.stringify({ recycled: true, scope: "regional-local", request_id: "stub" }),
        { status: 200, headers: { "Content-Type": "application/json" } },
      );
    },
  });
  const env = {
    CORELINK_SERVER: namespace,
    ENVIRONMENT: "test",
    CORELINK_INTERNAL_AUTH_KEY: KEY,
    CORELINK_ADMIN_AUTH_KEY: KEY,
    PROD_LHR: mkRegion("lhr"),
    PROD_SAM: mkRegion("sam"),
    PROD_NRT: mkRegion("nrt"),
    PROD_SYD: mkRegion("syd"),
  } as unknown as Env;
  return { env, calls };
}

describe("POST /_internal/admin/recycle-system", () => {
  it("recycles the local _system via /_do/stop and fans to all 4 regionals", async () => {
    const { env, calls } = makeRecycleEnv();
    const req = new Request("http://localhost/_internal/admin/recycle-system", {
      method: "POST",
      headers: { "x-corelink-internal-auth": KEY },
    });
    const resp = await workerHandler.fetch!(req, env, makeCtx());
    expect(resp.status).toBe(200);
    const body = (await resp.json()) as {
      recycled_local: boolean;
      regions: Array<{ region: string; recycled: boolean }>;
    };
    expect(body.recycled_local).toBe(true);
    // The local leg rewrote the path to the DO management endpoint.
    expect(calls.system).toContain("/_do/stop");
    // Every regional worker was fanned to exactly once.
    expect(Object.keys(calls.regions).sort()).toEqual(["lhr", "nrt", "sam", "syd"]);
    expect(body.regions.map((r) => r.region).sort()).toEqual(["lhr", "nrt", "sam", "syd"]);
    expect(body.regions.every((r) => r.recycled)).toBe(true);
  });

  it("rejects without the operator internal-auth key (gate before dispatch)", async () => {
    const { env } = makeRecycleEnv();
    const req = new Request("http://localhost/_internal/admin/recycle-system", {
      method: "POST",
    });
    const resp = await workerHandler.fetch!(req, env, makeCtx());
    expect(resp.ok).toBe(false);
  });

  it("a fan-out target recycles ONLY its local container (no re-fan / loop guard)", async () => {
    const { env, calls } = makeRecycleEnv();
    const req = new Request("http://localhost/_internal/admin/recycle-system", {
      method: "POST",
      headers: { "x-corelink-internal-auth": KEY, "x-corelink-fanout-from": "iad" },
    });
    const resp = await workerHandler.fetch!(req, env, makeCtx());
    expect(resp.status).toBe(200);
    const body = (await resp.json()) as { scope: string };
    expect(body.scope).toBe("regional-local");
    expect(calls.system).toContain("/_do/stop");
    // Must NOT re-fan out to the regionals.
    expect(Object.keys(calls.regions)).toEqual([]);
  });
});
