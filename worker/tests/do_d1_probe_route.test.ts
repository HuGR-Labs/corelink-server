/**
 * `/_internal/do-d1-probe/{tenant_id}` — delivery for the DO D1-placement
 * instrument (`worker/src/durable_object.ts` → `d1_probe`).
 *
 * The measurement is only worth anything if the probe reaches the SAME DO
 * instance that serves that tenant's traffic — DO placement is per-DO-id, so a
 * probe of a different DO answers nothing. These tests pin exactly that:
 *
 *   1. the DO id is derived with `idFromName(<tenant_id from the path>)` — the
 *      identical derivation the tenant data path uses;
 *   2. the forward targets `/_do/health` and preserves the query (`?colo=1`);
 *   3. the caller's internal-auth secret is STRIPPED before the DO sees it;
 *   4. the route is gated (401 without the right key) and is NOT publicly
 *      reachable;
 *   5. it gates on the low-privilege READ consumer (`quota_read`), never on the
 *      erase key — reading a latency number must not require the key that can
 *      delete a tenant's bytes.
 */

import { describe, it, expect } from "vitest";
import workerHandler, { internalConsumerForPath } from "../src/index.js";
import type { Env } from "../src/index.js";

const SHARED_KEY = "s".repeat(64);
const TENANT = "ee30f7ba-fc25-4d71-939e-ebe130b4c6a3";

function makeCtx(): ExecutionContext {
  return {
    waitUntil: () => {},
    passThroughOnException: () => {},
  } as unknown as ExecutionContext;
}

/** Env whose DO namespace records the name it was asked for + the forwarded request. */
function makeEnv(over: Partial<Record<string, unknown>> = {}): {
  env: Env;
  idFromNameCalls: string[];
  forwarded: () => Request | null;
} {
  const idFromNameCalls: string[] = [];
  let forwarded: Request | null = null;
  const stub = {
    fetch: async (req: Request): Promise<Response> => {
      forwarded = req;
      return new Response(
        JSON.stringify({ status: "stopped", d1_probe: { probe_version: 1 } }),
        { status: 503, headers: { "Content-Type": "application/json" } },
      );
    },
  };
  const env = {
    CORELINK_SERVER: {
      idFromName: (n: string) => {
        idFromNameCalls.push(n);
        return { toString: () => `id:${n}` };
      },
      get: (_id: unknown) => stub,
      idFromString: (_s: string) => ({ toString: () => "id" }),
      newUniqueId: () => ({ toString: () => "unique-id" }),
      jurisdiction: (_j: string) => ({}) as unknown,
    } as unknown as Env["CORELINK_SERVER"],
    ENVIRONMENT: "test",
    CORELINK_INTERNAL_AUTH_KEY: SHARED_KEY,
    REQUEST_QUOTA_DISABLED: "true",
    ...over,
  } as unknown as Env;
  return { env, idFromNameCalls, forwarded: () => forwarded };
}

describe("/_internal/do-d1-probe/{tenant} — consumer gate", () => {
  it("gates on the low-privilege READ consumer, not on erase", () => {
    expect(internalConsumerForPath(`/_internal/do-d1-probe/${TENANT}`)).toBe("quota_read");
  });

  it("leaves every other consumer mapping unchanged", () => {
    expect(internalConsumerForPath("/_internal/pat/mint")).toBe("pat_mint");
    expect(internalConsumerForPath("/_internal/dsr/erase")).toBe("erase");
    expect(internalConsumerForPath(`/_internal/tenant/${TENANT}/quota`)).toBe("quota_read");
  });
});

describe("/_internal/do-d1-probe/{tenant} — delivery", () => {
  it("is NOT reachable without the internal key (401)", async () => {
    const { env, idFromNameCalls } = makeEnv();
    const resp = await workerHandler.fetch!(
      new Request(`http://localhost/_internal/do-d1-probe/${TENANT}`),
      env,
      makeCtx(),
    );
    expect(resp.status).toBe(401);
    // The DO must never be touched by an unauthenticated caller.
    expect(idFromNameCalls).toEqual([]);
  });

  it("forwards to idFromName(<tenant>) /_do/health and returns the DO body verbatim", async () => {
    const { env, idFromNameCalls, forwarded } = makeEnv();
    const resp = await workerHandler.fetch!(
      new Request(`http://localhost/_internal/do-d1-probe/${TENANT}`, {
        headers: { "x-corelink-internal-auth": SHARED_KEY },
      }),
      env,
      makeCtx(),
    );
    // SAME derivation as the tenant data path — this is the whole point.
    expect(idFromNameCalls).toEqual([TENANT]);
    const req = forwarded();
    expect(req).not.toBeNull();
    expect(new URL(req!.url).pathname).toBe("/_do/health");
    expect(req!.method).toBe("GET");
    // Body passes through untouched (503 while the container is stopped — the
    // numbers live in the BODY, not in the status).
    expect(resp.status).toBe(503);
    const body = (await resp.json()) as Record<string, unknown>;
    expect(body["d1_probe"]).toBeDefined();
  });

  it("preserves ?colo=1 and STRIPS the caller's internal-auth secret", async () => {
    const { env, forwarded } = makeEnv();
    await workerHandler.fetch!(
      new Request(`http://localhost/_internal/do-d1-probe/${TENANT}?colo=1`, {
        headers: {
          "x-corelink-internal-auth": SHARED_KEY,
          // A caller trying to smuggle a forged tenant into the DO.
          "x-corelink-tenant-id": "victim-tenant",
        },
      }),
      env,
      makeCtx(),
    );
    const req = forwarded();
    expect(new URL(req!.url).search).toBe("?colo=1");
    expect(req!.headers.get("x-corelink-internal-auth")).toBeNull();
    expect(req!.headers.get("x-corelink-tenant-id")).toBeNull();
  });

  it("rejects a missing tenant id (400) — placement is per-DO-id, so it is required", async () => {
    const { env, idFromNameCalls } = makeEnv();
    const resp = await workerHandler.fetch!(
      new Request("http://localhost/_internal/do-d1-probe/", {
        headers: { "x-corelink-internal-auth": SHARED_KEY },
      }),
      env,
      makeCtx(),
    );
    expect(resp.status).toBe(400);
    expect(idFromNameCalls).toEqual([]);
  });

  it("uses the dedicated quota-read key when one is bound (shared key then fails)", async () => {
    const readKey = "q".repeat(64);
    const { env } = makeEnv({ CORELINK_QUOTA_READ_AUTH_KEY: readKey });
    const okResp = await workerHandler.fetch!(
      new Request(`http://localhost/_internal/do-d1-probe/${TENANT}`, {
        headers: { "x-corelink-internal-auth": readKey },
      }),
      env,
      makeCtx(),
    );
    expect(okResp.status).toBe(503); // reached the DO (container stopped)

    const { env: env2 } = makeEnv({ CORELINK_QUOTA_READ_AUTH_KEY: readKey });
    const badResp = await workerHandler.fetch!(
      new Request(`http://localhost/_internal/do-d1-probe/${TENANT}`, {
        headers: { "x-corelink-internal-auth": SHARED_KEY },
      }),
      env2,
      makeCtx(),
    );
    expect(badResp.status).toBe(401);
  });
});
