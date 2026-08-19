import { describe, it, expect } from "vitest";
import type {
  DurableObjectState,
  DurableObjectStorage,
  DurableObjectId,
  DurableObjectNamespace,
  DurableObjectTransaction,
  DurableObjectFacets,
} from "@cloudflare/workers-types";
import { RequestMeterShardDO } from "../src/request_meter_shard_do.js";
import { RequestMeterCoordinatorDO } from "../src/request_meter_coordinator_do.js";
import { meterViaDO, type DOMeterNamespaces } from "../src/lib/edge_do_meter.js";
import type { Env } from "../src/index.js";

// DO state mock (mirrors request_meter_*_do.test.ts).
function makeBackedState(store: Map<string, unknown>, idStr: string): DurableObjectState {
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

// A namespace mock that instantiates the REAL DO class per idFromName(name) and
// persists its store across calls — so meterViaDO runs the true fetch contract.
type DOClass = { new (state: DurableObjectState, env: Env): { fetch(r: Request): Promise<Response> } };
function makeNamespace(Cls: DOClass): DurableObjectNamespace {
  const instances = new Map<string, { fetch(r: Request): Promise<Response> }>();
  return {
    idFromName: (name: string) =>
      ({ toString: () => name, name, equals: (o: DurableObjectId) => o.toString() === name }) as DurableObjectId,
    get: (id: DurableObjectId) => {
      const name = id.toString();
      let inst = instances.get(name);
      if (!inst) {
        inst = new Cls(makeBackedState(new Map(), name), makeEnv());
        instances.set(name, inst);
      }
      const bound = inst;
      return {
        fetch: (input: RequestInfo, init?: RequestInit) =>
          bound.fetch(new Request(input as string, init)),
      };
    },
  } as unknown as DurableObjectNamespace;
}

function freshNs(): DOMeterNamespaces {
  return {
    shard: makeNamespace(RequestMeterShardDO as unknown as DOClass),
    coordinator: makeNamespace(RequestMeterCoordinatorDO as unknown as DOClass),
  };
}

const YM = "2026-08";
const P = (over: Partial<Parameters<typeof meterViaDO>[1]> = {}) => ({
  tenantId: "tenant-a",
  region: "iad",
  yearMonth: YM,
  cap: 1_000_000,
  block: 1_000,
  lowWater: 0,
  ...over,
});

describe("meterViaDO — worker-side shard+coordinator orchestration", () => {
  it("first request refills from empty and serves (cap headroom)", async () => {
    const ns = freshNs();
    const v = await meterViaDO(ns, P());
    expect(v.withinCap).toBe(true);
    expect(v.refilled).toBe(true);
    expect(v.balance).toBe(999); // 1000 granted − 1 served
  });

  it("serves exactly `cap` requests then denies (over-serve=0 through the orchestration)", async () => {
    const ns = freshNs();
    const cap = 3;
    let served = 0;
    for (let i = 0; i < 8; i++) {
      const v = await meterViaDO(ns, P({ cap, block: 10 }));
      if (v.withinCap) served++;
    }
    expect(served).toBe(3);
    // Once more: still denied, not negative.
    const last = await meterViaDO(ns, P({ cap, block: 10 }));
    expect(last.withinCap).toBe(false);
  });

  it("does not hop the coordinator when the balance is above low-water", async () => {
    const ns = freshNs();
    // Prime a lease (empty→refill→serve).
    await meterViaDO(ns, P({ block: 1_000, lowWater: 0 }));
    // Next request has balance 999 > lowWater 0 ⇒ no refill.
    const v = await meterViaDO(ns, P({ block: 1_000, lowWater: 0 }));
    expect(v.withinCap).toBe(true);
    expect(v.refilled).toBe(false);
    expect(v.balance).toBe(998);
  });

  it("two regions never collectively serve past the shared cap", async () => {
    const ns = freshNs();
    const cap = 5;
    const regions = ["iad", "lhr"];
    let served = 0;
    for (let i = 0; i < 20; i++) {
      const region = regions[i % 2];
      const v = await meterViaDO(ns, P({ cap, block: 4, region }));
      if (v.withinCap) served++;
    }
    expect(served).toBe(cap); // 5, never 6+
  });

  it("month rollover gives a fresh cap", async () => {
    const ns = freshNs();
    const cap = 2;
    let served = 0;
    for (let i = 0; i < 5; i++) {
      if ((await meterViaDO(ns, P({ cap, block: 10 }))).withinCap) served++;
    }
    expect(served).toBe(2);
    // New month: budget resets.
    const next = await meterViaDO(ns, P({ cap, block: 10, yearMonth: "2026-09" }));
    expect(next.withinCap).toBe(true);
  });
});
