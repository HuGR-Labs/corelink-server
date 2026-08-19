import { describe, it, expect } from "vitest";
import type {
  DurableObjectState,
  DurableObjectStorage,
  DurableObjectId,
  DurableObjectTransaction,
  DurableObjectFacets,
  D1Database,
  D1Result,
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

// ── WP-4: D1 seed + reconcile seam ──────────────────────────────────────────
// A mock CONFIG_DB backed by an in-memory (tenant|ym → count) map that records
// every write, so we can prove seed reads and reconcile writes precisely.
function makeMockD1() {
  const store = new Map<string, number>();
  const writes: Array<{ tenant: string; ym: string; count: number }> = [];
  const db = {
    prepare: (sql: string) => ({
      bind: (...binds: unknown[]) => ({
        first: async <T>() => {
          const key = `${binds[0]}|${binds[1]}`;
          return store.has(key) ? ({ request_count: store.get(key) } as unknown as T) : null;
        },
        run: async () => {
          const [tenant, ym, count] = binds as [string, string, number];
          writes.push({ tenant, ym, count });
          const key = `${tenant}|${ym}`;
          store.set(key, Math.max(store.get(key) ?? 0, count)); // mirrors the MAX upsert
          return { success: true } as unknown as D1Result;
        },
      }),
    }),
  } as unknown as D1Database;
  return { db, store, writes };
}
function makeEnvWithD1(db: D1Database): Env {
  return { CORELINK_SERVER: {} as never, ENVIRONMENT: "test", CONFIG_DB: db } as unknown as Env;
}

describe("RequestMeterCoordinatorDO — WP-4 D1 seed + reconcile", () => {
  it("seeds `consumed` from the D1 ledger on the first refill of a month", async () => {
    const mock = makeMockD1();
    mock.store.set(`tenant-a|${YM}`, 400_000); // D1 already holds 400k this month
    const do_ = new RequestMeterCoordinatorDO(makeBackedState(new Map()), makeEnvWithD1(mock.db));

    // First refill for a cap-500k tenant: consumed must START at the seeded 400k,
    // so only 100k of headroom remains (not a fresh 500k).
    const r = (await (
      await do_.fetch(
        refillReq({
          cap: 500_000,
          region: "iad",
          yearMonth: YM,
          spentDelta: 0,
          reportedBalance: 0,
          block: 1_000_000,
          tenantId: "tenant-a",
        }),
      )
    ).json()) as { consumed: number; granted: number; remaining: number };
    expect(r.consumed).toBe(400_000);
    expect(r.granted).toBe(100_000); // min(block, cap−seeded)
    expect(r.remaining).toBe(0);
  });

  it("does NOT write D1 in shadow (reconcileToD1 absent/false)", async () => {
    const mock = makeMockD1();
    const do_ = new RequestMeterCoordinatorDO(makeBackedState(new Map()), makeEnvWithD1(mock.db));
    await do_.fetch(
      refillReq({
        cap: 500_000, region: "iad", yearMonth: YM, spentDelta: 100, reportedBalance: 0,
        block: 1_000, tenantId: "tenant-a", // reconcileToD1 omitted
      }),
    );
    expect(mock.writes).toHaveLength(0); // shadow must never touch the ledger
  });

  it("writes `consumed` to D1 when reconcileToD1 is true (serve), never decreasing", async () => {
    const mock = makeMockD1();
    mock.store.set(`tenant-a|${YM}`, 10_000); // ledger baseline
    const do_ = new RequestMeterCoordinatorDO(makeBackedState(new Map()), makeEnvWithD1(mock.db));
    // Refill reports 5k spent since sync ⇒ consumed = seeded 10k + 5k = 15k.
    await do_.fetch(
      refillReq({
        cap: 500_000, region: "iad", yearMonth: YM, spentDelta: 5_000, reportedBalance: 0,
        block: 1_000, tenantId: "tenant-a", reconcileToD1: true,
      }),
    );
    expect(mock.writes).toHaveLength(1);
    expect(mock.writes[0]).toMatchObject({ tenant: "tenant-a", ym: YM, count: 15_000 });
    expect(mock.store.get(`tenant-a|${YM}`)).toBe(15_000);
  });

  it("runs as a pure lease (no seed/reconcile) when tenantId is absent", async () => {
    const mock = makeMockD1();
    const do_ = new RequestMeterCoordinatorDO(makeBackedState(new Map()), makeEnvWithD1(mock.db));
    const r = (await (
      await do_.fetch(
        refillReq({ cap: 500_000, region: "iad", yearMonth: YM, spentDelta: 0, reportedBalance: 0, block: 1_000 }),
      )
    ).json()) as { consumed: number; granted: number };
    expect(r.consumed).toBe(0); // no seed
    expect(r.granted).toBe(1_000);
    expect(mock.writes).toHaveLength(0); // no reconcile
  });

  it("seeds only ONCE per month — the second refill keeps the in-memory state", async () => {
    const mock = makeMockD1();
    mock.store.set(`tenant-a|${YM}`, 1_000);
    const do_ = new RequestMeterCoordinatorDO(makeBackedState(new Map()), makeEnvWithD1(mock.db));
    const env = { tenantId: "tenant-a", cap: 500_000, region: "iad", yearMonth: YM, block: 1_000 };
    await do_.fetch(refillReq({ ...env, spentDelta: 0, reportedBalance: 0 }));
    // Bump the ledger AFTER the first seed; a re-seed would wrongly pull 9_999.
    mock.store.set(`tenant-a|${YM}`, 9_999);
    const r2 = (await (
      await do_.fetch(refillReq({ ...env, spentDelta: 50, reportedBalance: 1_000 }))
    ).json()) as { consumed: number };
    expect(r2.consumed).toBe(1_050); // seeded 1_000 once + 50 spent — NOT 9_999
  });
});
