import { describe, expect, it, vi } from "vitest";
import type { DurableObjectState, DurableObjectStorage } from "@cloudflare/workers-types";
import { CoreLinkServer } from "../src/durable_object.js";
import type { Env } from "../src/index.js";
import {
  enforcePatIssueRateLimit,
  PAT_ISSUE_BUCKET_KEY,
} from "../src/pat_issue_rate_limit.js";

type Store = Map<string, unknown>;

function makeState(store: Store, serialized = true, getFailure?: Error): DurableObjectState {
  let tail = Promise.resolve();
  const blockConcurrencyWhile = async <T>(fn: () => Promise<T>): Promise<T> => {
    const previous = tail;
    let release!: () => void;
    tail = new Promise<void>((resolve) => { release = resolve; });
    if (serialized) await previous;
    try {
      return await fn();
    } finally {
      release();
    }
  };
  const storage = {
    get: async (key: string) => {
      if (getFailure) throw getFailure;
      return store.get(key);
    },
    put: async (key: string, value: unknown) => {
      if (getFailure) throw getFailure;
      store.set(key, value);
    },
  } as unknown as DurableObjectStorage;
  return {
    id: { toString: () => "pat-test-do" },
    storage,
    blockConcurrencyWhile,
  } as unknown as DurableObjectState;
}

async function spend(
  state: DurableObjectState,
  tenantId: string,
): Promise<{ allowed: boolean; response?: Response }> {
  return enforcePatIssueRateLimit(
    state,
    state.storage,
    null,
    "pat-test",
    tenantId,
  );
}

describe("durable per-tenant PAT issuance limiter", () => {
  it("blocks the real DO request before container startup and ignores a forged lease", async () => {
    vi.useFakeTimers();
    try {
      vi.setSystemTime(0);
      const store = new Map<string, unknown>([
        ["lifecycle", { tenantId: "tenant-a" }],
        [PAT_ISSUE_BUCKET_KEY, {
          version: 1,
          endpoint: "pat-issue",
          tenantId: "tenant-a",
          availableTokens: 0,
          lastRefillAtMs: 0,
        }],
      ]);
      const state = makeState(store);
      const server = new CoreLinkServer(state, {
        CORELINK_SERVER: {} as never,
        ENVIRONMENT: "test",
      } as Env);
      const response = await server.fetch(new Request("http://localhost/v1/pats", {
        method: "POST",
        headers: {
          "x-corelink-tenant-id": "tenant-a",
          ["x-corelink-pat-issue-authorized"]: "1",
        },
        body: "{}",
      }));
      expect(response.status).toBe(429);
      expect(response.headers.get("Retry-After")).toBe("360");
    } finally {
      vi.useRealTimers();
    }
  });

  it("allows exactly the burst and serializes concurrent aliases", async () => {
    const state = makeState(new Map(), true);
    const results = await Promise.all(
      Array.from({ length: 11 }, () => spend(state, "tenant-a")),
    );
    expect(results.filter((result) => result.allowed)).toHaveLength(10);
    expect(results.filter((result) => !result.allowed)).toHaveLength(1);
    expect(results.find((result) => !result.allowed)?.response?.status).toBe(429);
    expect(results.find((result) => !result.allowed)?.response?.headers.get("Retry-After")).toBe("360");
  });

  it("persists the exhausted bucket across a simulated restart", async () => {
    const store = new Map<string, unknown>();
    const first = makeState(store);
    for (let i = 0; i < 10; i++) expect((await spend(first, "tenant-a")).allowed).toBe(true);

    const restarted = makeState(store);
    const denied = await spend(restarted, "tenant-a");
    expect(denied.allowed).toBe(false);
    expect(denied.response?.status).toBe(429);
    expect(store.has(PAT_ISSUE_BUCKET_KEY)).toBe(true);
  });

  it("refills at 360 seconds, but not one millisecond before", async () => {
    vi.useFakeTimers();
    try {
      vi.setSystemTime(0);
      const state = makeState(new Map());
      for (let i = 0; i < 10; i++) expect((await spend(state, "tenant-a")).allowed).toBe(true);
      expect((await spend(state, "tenant-a")).allowed).toBe(false);
      vi.setSystemTime(359_999);
      expect((await spend(state, "tenant-a")).allowed).toBe(false);
      vi.setSystemTime(360_000);
      expect((await spend(state, "tenant-a")).allowed).toBe(true);
    } finally {
      vi.useRealTimers();
    }
  });

  it("rejects tenant switching and keeps the original bucket binding", async () => {
    const store = new Map<string, unknown>();
    const state = makeState(store);
    expect((await spend(state, "tenant-a")).allowed).toBe(true);
    const mismatch = await spend(state, "tenant-b");
    expect(mismatch.allowed).toBe(false);
    expect(mismatch.response?.status).toBe(403);
    expect((store.get(PAT_ISSUE_BUCKET_KEY) as { tenantId: string }).tenantId).toBe("tenant-a");
  });

  it.each([
    ["missing", null, 401],
    ["anonymous", "_anonymous", 403],
    ["whitespace", " tenant-a", 403],
  ])("fails closed for %s tenant identity", async (_name, tenant, status) => {
    const state = makeState(new Map());
    const result = await enforcePatIssueRateLimit(state, state.storage, null, "pat-test", tenant);
    expect(result.allowed).toBe(false);
    expect(result.response?.status).toBe(status);
  });

  it("fails closed for storage errors and malformed persisted state", async () => {
    const failed = makeState(new Map(), true, new Error("storage unavailable"));
    const unavailable = await spend(failed, "tenant-a");
    expect(unavailable.allowed).toBe(false);
    expect(unavailable.response?.status).toBe(503);

    const malformedStore = new Map<string, unknown>([
      [PAT_ISSUE_BUCKET_KEY, { version: 1, endpoint: "wrong", tenantId: "tenant-a" }],
    ]);
    const malformed = await spend(makeState(malformedStore), "tenant-a");
    expect(malformed.allowed).toBe(false);
    expect(malformed.response?.status).toBe(503);
  });
});
