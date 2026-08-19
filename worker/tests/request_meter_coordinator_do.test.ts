import { describe, it, expect } from "vitest";
import type {
  DurableObjectState,
  DurableObjectStorage,
  DurableObjectId,
  DurableObjectTransaction,
  DurableObjectFacets,
} from "@cloudflare/workers-types";
import { RequestMeterCoordinatorDO } from "../src/request_meter_coordinator_do.js";
import type { Env } from "../src/index.js";

// In-memory storage-backed mock state (mirrors event_log_do.test.ts).
function makeBackedState(store: Map<string, unknown>, idStr = "tenant-a"): DurableObjectState {
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

function refillReq(body: Record<string, unknown>): Request {
  return new Request("http://do/meter", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ op: "refill", ...body }),
  });
}

const YM = "2026-08";

describe("RequestMeterCoordinatorDO — DO shell over the lease core", () => {
  it("grants a block, persists state, and survives a simulated DO restart", async () => {
    const store = new Map<string, unknown>();
    let do_ = new RequestMeterCoordinatorDO(makeBackedState(store), makeEnv());

    let res = await do_.fetch(
      refillReq({ cap: 25_000, region: "iad", yearMonth: YM, spentDelta: 0, reportedBalance: 0, block: 10_000 }),
    );
    let body = (await res.json()) as { granted: number; newBalance: number; remaining: number };
    expect(body.granted).toBe(10_000);
    expect(body.remaining).toBe(15_000);

    // New instance over the SAME store = a restart: state must survive.
    do_ = new RequestMeterCoordinatorDO(makeBackedState(store), makeEnv());
    res = await do_.fetch(refillReq({ cap: 25_000, region: "iad", yearMonth: YM, spentDelta: 0, reportedBalance: body.newBalance, block: 10_000 }));
    body = (await res.json()) as typeof body;
    expect(body.granted).toBe(10_000); // 20k held ≤ 25k
    expect(body.newBalance).toBe(20_000);
  });

  it("enforces over-serve=0 across regions through the fetch API", async () => {
    const store = new Map<string, unknown>();
    const do_ = new RequestMeterCoordinatorDO(makeBackedState(store), makeEnv());
    const cap = 15_000;

    const a = (await (await do_.fetch(refillReq({ cap, region: "iad", yearMonth: YM, spentDelta: 0, reportedBalance: 0, block: 10_000 }))).json()) as { granted: number };
    expect(a.granted).toBe(10_000);
    const b = (await (await do_.fetch(refillReq({ cap, region: "nrt", yearMonth: YM, spentDelta: 0, reportedBalance: 0, block: 10_000 }))).json()) as { granted: number; remaining: number };
    expect(b.granted).toBe(5_000); // only 5k left
    const c = (await (await do_.fetch(refillReq({ cap, region: "lhr", yearMonth: YM, spentDelta: 0, reportedBalance: 0, block: 10_000 }))).json()) as { granted: number };
    expect(c.granted).toBe(0); // cap reached
  });

  it("month rollover resets the budget", async () => {
    const store = new Map<string, unknown>();
    const do_ = new RequestMeterCoordinatorDO(makeBackedState(store), makeEnv());
    await do_.fetch(refillReq({ cap: 10_000, region: "iad", yearMonth: YM, spentDelta: 0, reportedBalance: 0, block: 10_000 }));
    const next = (await (await do_.fetch(refillReq({ cap: 10_000, region: "iad", yearMonth: "2026-09", spentDelta: 0, reportedBalance: 0, block: 10_000 }))).json()) as { granted: number; yearMonth: string };
    expect(next.yearMonth).toBe("2026-09");
    expect(next.granted).toBe(10_000); // fresh month, full block
  });

  it("reclaim frees an idle region's balance without spending it", async () => {
    const store = new Map<string, unknown>();
    const do_ = new RequestMeterCoordinatorDO(makeBackedState(store), makeEnv());
    const cap = 15_000;
    await do_.fetch(refillReq({ cap, region: "iad", yearMonth: YM, spentDelta: 0, reportedBalance: 0, block: 10_000 }));
    await do_.fetch(refillReq({ cap, region: "nrt", yearMonth: YM, spentDelta: 0, reportedBalance: 0, block: 10_000 })); // nrt gets 5k

    await do_.fetch(
      new Request("http://do/meter", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ op: "reclaim", region: "iad", yearMonth: YM, keep: 0 }),
      }),
    );
    // nrt can now lease the freed 10k.
    const more = (await (await do_.fetch(refillReq({ cap, region: "nrt", yearMonth: YM, spentDelta: 0, reportedBalance: 5_000, block: 10_000 }))).json()) as { granted: number };
    expect(more.granted).toBe(10_000);
  });

  it("read reports presence + rejects bad args / methods", async () => {
    const store = new Map<string, unknown>();
    const do_ = new RequestMeterCoordinatorDO(makeBackedState(store), makeEnv());

    const readReq = () =>
      new Request("http://do/meter", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ op: "read" }) });
    let r = (await (await do_.fetch(readReq())).json()) as { present: boolean };
    expect(r.present).toBe(false);

    await do_.fetch(refillReq({ cap: 100, region: "iad", yearMonth: YM, spentDelta: 0, reportedBalance: 0, block: 10 }));
    r = (await (await do_.fetch(readReq())).json()) as { present: boolean };
    expect(r.present).toBe(true);

    const bad = await do_.fetch(refillReq({ cap: "nope", region: "iad", yearMonth: YM, spentDelta: 0, reportedBalance: 0, block: 10 }));
    expect(bad.status).toBe(400);

    const getRes = await do_.fetch(new Request("http://do/meter", { method: "GET" }));
    expect(getRes.status).toBe(405);
  });
});
