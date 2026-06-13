/**
 * Unit tests for worker/src/event_log_do.ts — the ADR-0065 thin append-only
 * event-log Durable Object primitive.
 *
 * These exercise the frozen contract consumers (hugit) depend on:
 *   - append returns a strictly-monotonic, gap-free, 1-based seq + a ts_ms;
 *   - read returns entries in seq order from a `from_seq` cursor, bounded by limit;
 *   - the DO is tenant-pinned (cross-tenant 403);
 *   - payload is stored verbatim and never interpreted;
 *   - head + seq survive a simulated DO restart (new instance over same storage).
 *
 * The mock DurableObjectState backs a real in-memory Map so the storage
 * get/put/list semantics (and lexicographic list order) are faithful — the
 * rollout_controller.test.ts no-op storage is insufficient for append/read.
 */

import { describe, it, expect } from "vitest";
import { EventLogDO } from "../src/event_log_do.js";
import type { Env } from "../src/index.js";

// ──────────────────────────────────────────────────────────────────────────────
// In-memory storage-backed mock state
// ──────────────────────────────────────────────────────────────────────────────

function makeBackedState(store: Map<string, unknown>, idStr = "tenant-a-do"): DurableObjectState {
  const storage = {
    get: async (key: string) => store.get(key),
    put: async (key: string, value: unknown) => {
      store.set(key, value);
    },
    delete: async (key: string) => store.delete(key),
    deleteAll: async () => store.clear(),
    list: async (options?: { prefix?: string; start?: string; limit?: number }) => {
      const prefix = options?.prefix ?? "";
      const start = options?.start;
      const limit = options?.limit ?? Infinity;
      const keys = Array.from(store.keys())
        .filter((k) => k.startsWith(prefix))
        .filter((k) => start === undefined || k >= start)
        .sort();
      const out = new Map<string, unknown>();
      for (const k of keys) {
        if (out.size >= limit) break;
        out.set(k, store.get(k));
      }
      return out;
    },
    getAlarm: async () => null,
    setAlarm: async () => {},
    deleteAlarm: async () => {},
    transaction: async (fn: (t: DurableObjectTransaction) => Promise<void>) => fn({} as DurableObjectTransaction),
  } as unknown as DurableObjectStorage;

  return {
    id: {
      toString: () => idStr,
      name: idStr,
      equals: (other: DurableObjectId) => other.toString() === idStr,
    } as DurableObjectId,
    storage,
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
// Request helpers
// ──────────────────────────────────────────────────────────────────────────────

function appendReq(tenant: string, payload: unknown): Request {
  return new Request("http://do/_eventlog/append", {
    method: "POST",
    headers: { "x-corelink-tenant-id": tenant, "content-type": "application/json" },
    body: JSON.stringify(payload),
  });
}

function readReq(tenant: string, query = ""): Request {
  return new Request("http://do/_eventlog/read" + query, {
    method: "GET",
    headers: { "x-corelink-tenant-id": tenant },
  });
}

interface AppendBody {
  seq: number;
  ts_ms: number;
}
interface ReadBody {
  entries: Array<{ seq: number; ts_ms: number; payload: unknown }>;
  head: number;
  next_seq: number;
}

// ──────────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────────

describe("EventLogDO — health", () => {
  it("/_do/health returns 200 with class:EventLogDO", async () => {
    const do_ = new EventLogDO(makeBackedState(new Map()), makeEnv());
    const resp = await do_.fetch(new Request("http://do/_do/health"));
    expect(resp.status).toBe(200);
    const body = (await resp.json()) as { status: string; class: string };
    expect(body.status).toBe("ok");
    expect(body.class).toBe("EventLogDO");
  });
});

describe("EventLogDO — append", () => {
  it("first append returns seq=1 and a ts_ms", async () => {
    const do_ = new EventLogDO(makeBackedState(new Map()), makeEnv());
    const resp = await do_.fetch(appendReq("tenant-a", { kind: "x" }));
    expect(resp.status).toBe(200);
    const body = (await resp.json()) as AppendBody;
    expect(body.seq).toBe(1);
    expect(typeof body.ts_ms).toBe("number");
    expect(body.ts_ms).toBeGreaterThan(0);
  });

  it("seq is strictly monotonic and gap-free across appends", async () => {
    const do_ = new EventLogDO(makeBackedState(new Map()), makeEnv());
    const seqs: number[] = [];
    for (let i = 0; i < 5; i++) {
      const resp = await do_.fetch(appendReq("tenant-a", { i }));
      seqs.push(((await resp.json()) as AppendBody).seq);
    }
    expect(seqs).toEqual([1, 2, 3, 4, 5]);
  });

  it("rejects a missing tenant header with 400 TENANT_REQUIRED", async () => {
    const do_ = new EventLogDO(makeBackedState(new Map()), makeEnv());
    const resp = await do_.fetch(
      new Request("http://do/_eventlog/append", {
        method: "POST",
        body: "{}",
      }),
    );
    expect(resp.status).toBe(400);
    expect(((await resp.json()) as { error: string }).error).toBe("TENANT_REQUIRED");
  });

  it("rejects a payload that is not valid JSON with 400 INVALID_PAYLOAD", async () => {
    const do_ = new EventLogDO(makeBackedState(new Map()), makeEnv());
    const resp = await do_.fetch(
      new Request("http://do/_eventlog/append", {
        method: "POST",
        headers: { "x-corelink-tenant-id": "tenant-a" },
        body: "{not json",
      }),
    );
    expect(resp.status).toBe(400);
    expect(((await resp.json()) as { error: string }).error).toBe("INVALID_PAYLOAD");
  });

  it("rejects an oversized payload with 413 PAYLOAD_TOO_LARGE", async () => {
    const do_ = new EventLogDO(makeBackedState(new Map()), makeEnv());
    const big = "x".repeat(97 * 1024);
    const resp = await do_.fetch(appendReq("tenant-a", big));
    expect(resp.status).toBe(413);
    expect(((await resp.json()) as { error: string }).error).toBe("PAYLOAD_TOO_LARGE");
  });
});

describe("EventLogDO — read", () => {
  it("read returns entries in seq order with verbatim payloads", async () => {
    const do_ = new EventLogDO(makeBackedState(new Map()), makeEnv());
    await do_.fetch(appendReq("tenant-a", { n: "one" }));
    await do_.fetch(appendReq("tenant-a", { n: "two" }));
    await do_.fetch(appendReq("tenant-a", { n: "three" }));

    const resp = await do_.fetch(readReq("tenant-a"));
    expect(resp.status).toBe(200);
    const body = (await resp.json()) as ReadBody;
    expect(body.entries.map((e) => e.seq)).toEqual([1, 2, 3]);
    expect(body.entries.map((e) => e.payload)).toEqual([
      { n: "one" },
      { n: "two" },
      { n: "three" },
    ]);
    expect(body.head).toBe(3);
    expect(body.next_seq).toBe(4);
  });

  it("from_seq cursor skips earlier entries", async () => {
    const do_ = new EventLogDO(makeBackedState(new Map()), makeEnv());
    for (let i = 0; i < 4; i++) await do_.fetch(appendReq("tenant-a", { i }));
    const resp = await do_.fetch(readReq("tenant-a", "?from_seq=3"));
    const body = (await resp.json()) as ReadBody;
    expect(body.entries.map((e) => e.seq)).toEqual([3, 4]);
  });

  it("limit bounds the page size", async () => {
    const do_ = new EventLogDO(makeBackedState(new Map()), makeEnv());
    for (let i = 0; i < 5; i++) await do_.fetch(appendReq("tenant-a", { i }));
    const resp = await do_.fetch(readReq("tenant-a", "?from_seq=1&limit=2"));
    const body = (await resp.json()) as ReadBody;
    expect(body.entries.map((e) => e.seq)).toEqual([1, 2]);
  });

  it("read on an empty log returns no entries and head=0", async () => {
    const do_ = new EventLogDO(makeBackedState(new Map()), makeEnv());
    const resp = await do_.fetch(readReq("tenant-a"));
    const body = (await resp.json()) as ReadBody;
    expect(body.entries).toEqual([]);
    expect(body.head).toBe(0);
    expect(body.next_seq).toBe(1);
  });

  it("rejects from_seq=0 with 400 INVALID_FROM_SEQ", async () => {
    const do_ = new EventLogDO(makeBackedState(new Map()), makeEnv());
    const resp = await do_.fetch(readReq("tenant-a", "?from_seq=0"));
    expect(resp.status).toBe(400);
    expect(((await resp.json()) as { error: string }).error).toBe("INVALID_FROM_SEQ");
  });
});

describe("EventLogDO — tenant pinning", () => {
  it("rejects a second tenant on a pinned DO with 403 TENANT_MISMATCH", async () => {
    const store = new Map<string, unknown>();
    const do_ = new EventLogDO(makeBackedState(store), makeEnv());
    await do_.fetch(appendReq("tenant-a", { x: 1 }));
    const resp = await do_.fetch(appendReq("tenant-b", { x: 2 }));
    expect(resp.status).toBe(403);
    expect(((await resp.json()) as { error: string }).error).toBe("TENANT_MISMATCH");
  });
});

describe("EventLogDO — durability across restart", () => {
  it("a new DO instance over the same storage resumes the seq counter", async () => {
    const store = new Map<string, unknown>();
    const first = new EventLogDO(makeBackedState(store), makeEnv());
    await first.fetch(appendReq("tenant-a", { a: 1 }));
    await first.fetch(appendReq("tenant-a", { a: 2 }));

    // Simulate a DO restart: a fresh instance backed by the same storage Map.
    const second = new EventLogDO(makeBackedState(store), makeEnv());
    const resp = await second.fetch(appendReq("tenant-a", { a: 3 }));
    const body = (await resp.json()) as AppendBody;
    expect(body.seq).toBe(3);

    const readResp = await second.fetch(readReq("tenant-a"));
    const readBody = (await readResp.json()) as ReadBody;
    expect(readBody.entries.map((e) => e.seq)).toEqual([1, 2, 3]);
  });

  it("a restarted DO keeps its tenant pin", async () => {
    const store = new Map<string, unknown>();
    const first = new EventLogDO(makeBackedState(store), makeEnv());
    await first.fetch(appendReq("tenant-a", { a: 1 }));

    const second = new EventLogDO(makeBackedState(store), makeEnv());
    const resp = await second.fetch(appendReq("tenant-b", { a: 2 }));
    expect(resp.status).toBe(403);
  });
});
