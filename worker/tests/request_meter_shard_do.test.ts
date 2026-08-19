import { describe, it, expect } from "vitest";
import type {
  DurableObjectState,
  DurableObjectStorage,
  DurableObjectId,
  DurableObjectTransaction,
  DurableObjectFacets,
} from "@cloudflare/workers-types";
import { RequestMeterShardDO } from "../src/request_meter_shard_do.js";
import type { Env } from "../src/index.js";

// In-memory storage-backed mock state (mirrors request_meter_coordinator_do.test.ts).
function makeBackedState(store: Map<string, unknown>, idStr = "tenant-a:iad"): DurableObjectState {
  const storage = {
    get: async (key: string) => store.get(key),
    put: async (key: string, value: unknown) => {
      store.set(key, value);
    },
    delete: async (key: string) => store.delete(key),
    deleteAll: async () => store.clear(),
    list: async () => new Map(store),
    getAlarm: async () => null,
    setAlarm: async () => {},
    deleteAlarm: async () => {},
    transaction: async (fn: (t: DurableObjectTransaction) => Promise<void>) =>
      fn({} as DurableObjectTransaction),
  } as unknown as DurableObjectStorage;

  return {
    id: {
      toString: () => idStr,
      name: idStr,
      equals: (o: DurableObjectId) => o.toString() === idStr,
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
  return { CORELINK_SERVER: {} as never, ENVIRONMENT: "test" } as unknown as Env;
}

const YM = "2026-08";

function debitReq(body: Record<string, unknown>): Request {
  return new Request("http://do/shard", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ op: "debit", ...body }),
  });
}
function applyRefillReq(body: Record<string, unknown>): Request {
  return new Request("http://do/shard", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ op: "applyRefill", ...body }),
  });
}
const readReq = () =>
  new Request("http://do/shard", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ op: "read" }),
  });

describe("RequestMeterShardDO — DO shell over the shard core", () => {
  it("empty shard denies and asks for refill; grant persists across a restart", async () => {
    const store = new Map<string, unknown>();
    let do_ = new RequestMeterShardDO(makeBackedState(store), makeEnv());

    let res = await do_.fetch(debitReq({ yearMonth: YM, lowWater: 0 }));
    let body = (await res.json()) as {
      served: boolean;
      needsRefill: boolean;
      balance: number;
      refillReq: { spentDelta: number; reportedBalance: number };
    };
    expect(body.served).toBe(false);
    expect(body.needsRefill).toBe(true);
    expect(body.refillReq).toEqual({ spentDelta: 0, reportedBalance: 0 });

    // Coordinator grants 5 → shard adopts it.
    await do_.fetch(applyRefillReq({ yearMonth: YM, newBalance: 5 }));

    // New instance over the SAME store = a restart: the balance must survive.
    do_ = new RequestMeterShardDO(makeBackedState(store), makeEnv());
    res = await do_.fetch(debitReq({ yearMonth: YM, lowWater: 0 }));
    body = (await res.json()) as typeof body;
    expect(body.served).toBe(true);
    expect(body.balance).toBe(4);
  });

  it("serves exactly the leased balance and no more (local over-serve=0)", async () => {
    const store = new Map<string, unknown>();
    const do_ = new RequestMeterShardDO(makeBackedState(store), makeEnv());
    await do_.fetch(applyRefillReq({ yearMonth: YM, newBalance: 3 }));

    let served = 0;
    for (let i = 0; i < 10; i++) {
      const b = (await (await do_.fetch(debitReq({ yearMonth: YM, lowWater: 0 }))).json()) as {
        served: boolean;
      };
      if (b.served) served++;
    }
    expect(served).toBe(3);
  });

  it("reports spentSinceSync via refillReq, then zeroes it on applyRefill", async () => {
    const store = new Map<string, unknown>();
    const do_ = new RequestMeterShardDO(makeBackedState(store), makeEnv());
    await do_.fetch(applyRefillReq({ yearMonth: YM, newBalance: 5 }));
    await do_.fetch(debitReq({ yearMonth: YM, lowWater: 0 }));
    const b = (await (await do_.fetch(debitReq({ yearMonth: YM, lowWater: 0 }))).json()) as {
      refillReq: { spentDelta: number; reportedBalance: number };
    };
    expect(b.refillReq).toEqual({ spentDelta: 2, reportedBalance: 3 });

    await do_.fetch(applyRefillReq({ yearMonth: YM, newBalance: 13 }));
    const after = (await (await do_.fetch(debitReq({ yearMonth: YM, lowWater: 0 }))).json()) as {
      refillReq: { spentDelta: number; reportedBalance: number };
    };
    expect(after.refillReq).toEqual({ spentDelta: 1, reportedBalance: 12 });
  });

  it("drops a stale-month balance to 0 (month isolation)", async () => {
    const store = new Map<string, unknown>();
    const do_ = new RequestMeterShardDO(makeBackedState(store), makeEnv());
    await do_.fetch(applyRefillReq({ yearMonth: YM, newBalance: 100 }));
    const b = (await (await do_.fetch(debitReq({ yearMonth: "2026-09", lowWater: 0 }))).json()) as {
      served: boolean;
      balance: number;
      yearMonth: string;
    };
    expect(b.served).toBe(false);
    expect(b.balance).toBe(0);
    expect(b.yearMonth).toBe("2026-09");
  });

  it("read reports presence + rejects bad args / methods", async () => {
    const store = new Map<string, unknown>();
    const do_ = new RequestMeterShardDO(makeBackedState(store), makeEnv());

    let r = (await (await do_.fetch(readReq())).json()) as { present: boolean };
    expect(r.present).toBe(false);

    await do_.fetch(applyRefillReq({ yearMonth: YM, newBalance: 10 }));
    r = (await (await do_.fetch(readReq())).json()) as { present: boolean };
    expect(r.present).toBe(true);

    const bad = await do_.fetch(debitReq({ yearMonth: YM, lowWater: "nope" }));
    expect(bad.status).toBe(400);

    const getRes = await do_.fetch(new Request("http://do/shard", { method: "GET" }));
    expect(getRes.status).toBe(405);
  });
});
