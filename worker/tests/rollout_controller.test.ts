/**
 * Unit tests for worker/src/rollout_controller.ts.
 *
 * RolloutController is a Phase B stub (full WASM bridge deferred to Phase C).
 * These tests verify the stub's API contract: health probe returns 200,
 * all other requests return 501, and responses contain the correct fields.
 */

import { describe, it, expect } from "vitest";
import { RolloutController } from "../src/rollout_controller.js";
import type { Env } from "../src/index.js";

// ──────────────────────────────────────────────────────────────────────────────
// Helpers
// ──────────────────────────────────────────────────────────────────────────────

function makeMockState(idStr = "rollout-do-id"): DurableObjectState {
  return {
    id: {
      toString: () => idStr,
      name: idStr,
      equals: (other: DurableObjectId) => other.toString() === idStr,
    } as DurableObjectId,
    storage: {
      get: async () => undefined,
      put: async () => {},
      delete: async () => {},
      list: async () => new Map(),
      getAlarm: async () => null,
      setAlarm: async () => {},
      deleteAlarm: async () => {},
      transaction: async (fn: (t: DurableObjectTransaction) => Promise<void>) => fn({} as DurableObjectTransaction),
      deleteAll: async () => {},
    } as unknown as DurableObjectStorage,
    container: undefined,
    waitUntil: () => {},
    blockConcurrencyWhile: async <T>(fn: () => Promise<T>) => fn(),
    acceptWebSocket: () => {},
    getWebSockets: () => [],
    setWebSocketAutoResponse: () => {},
    getWebSocketAutoResponse: () => null,
    getWebSocketAutoResponseTimestamp: () => null,
    setHibernatableWebSocketEventTimeout: () => {},
    getHibernatableWebSocketEventTimeout: () => null,
    getTags: () => [],
    abort: () => {},
    props: {},
    facets: {} as DurableObjectFacets,
  } as unknown as DurableObjectState;
}

function makeEnv(): Env {
  return {
    CORELINK_SERVER: {} as DurableObjectNamespace,
    ENVIRONMENT: "test",
  };
}

// ──────────────────────────────────────────────────────────────────────────────
// RolloutController stub tests
// ──────────────────────────────────────────────────────────────────────────────

describe("RolloutController stub", () => {
  it("can be instantiated without throwing", () => {
    const state = makeMockState();
    const env = makeEnv();
    expect(() => new RolloutController(state, env)).not.toThrow();
  });

  it("/_do/health returns 200", async () => {
    const do_ = new RolloutController(makeMockState(), makeEnv());
    const resp = await do_.fetch(new Request("http://localhost/_do/health"));
    expect(resp.status).toBe(200);
  });

  it("/_do/health body contains status:ok and class:RolloutController", async () => {
    const do_ = new RolloutController(makeMockState(), makeEnv());
    const resp = await do_.fetch(new Request("http://localhost/_do/health"));
    const body = await resp.json() as { status: string; class: string; request_id: string };
    expect(body.status).toBe("ok");
    expect(body.class).toBe("RolloutController");
    expect(typeof body.request_id).toBe("string");
  });

  it("/_do/health sets X-Request-Id from incoming header", async () => {
    const do_ = new RolloutController(makeMockState(), makeEnv());
    const resp = await do_.fetch(
      new Request("http://localhost/_do/health", {
        headers: { "x-request-id": "rollout-health-test" },
      }),
    );
    expect(resp.headers.get("x-request-id")).toBe("rollout-health-test");
  });

  it("any non-health request returns 501 NOT_IMPLEMENTED", async () => {
    const do_ = new RolloutController(makeMockState(), makeEnv());
    const resp = await do_.fetch(new Request("http://localhost/v1/rollouts/my-rollout"));
    expect(resp.status).toBe(501);
    const body = await resp.json() as { error: string };
    expect(body.error).toBe("NOT_IMPLEMENTED");
  });

  it("501 response includes request_id", async () => {
    const do_ = new RolloutController(makeMockState(), makeEnv());
    const resp = await do_.fetch(
      new Request("http://localhost/v1/rollouts/status", {
        headers: { "x-request-id": "rollout-stub-test" },
      }),
    );
    expect(resp.headers.get("x-request-id")).toBe("rollout-stub-test");
    const body = await resp.json() as { request_id: string };
    expect(body.request_id).toBe("rollout-stub-test");
  });
});
