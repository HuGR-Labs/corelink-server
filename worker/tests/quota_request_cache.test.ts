/**
 * Tests for the REQUEST-COUNT async fast path (latency WP-B2) — the billing-
 * critical piece. The invariants under test:
 *   - the boundary is enforced EXACTLY: a tenant within burstMargin of its cap
 *     NEVER arms the fast path (falls back to the exact sync path);
 *   - NEVER over-count: KV is refreshed from the D1 RETURNING value, not a blind
 *     local +1;
 *   - uncapped tiers always arm (nothing to enforce), still meter async;
 *   - month-scoped KV key (no cross-month leak);
 *   - d1Error / fan-out are the caller's concern (not entered here).
 */

import { describe, it, expect, beforeEach } from "vitest";
import type { KvReader } from "../src/lib/pat_verify_cache.js";
import {
  decideFastPath,
  tryFastRequestCount,
  populateRequestCountKv,
  burstMargin,
  requestKvKey,
  BURST_MARGIN_FLOOR,
  __resetRequestCacheForTests,
  type RequestD1,
} from "../src/lib/quota_request_cache.js";

const T = "00000000-0000-0000-0000-000000000042";
const DB = {} as unknown as RequestD1;
const YM = "2026-08";

beforeEach(() => {
  __resetRequestCacheForTests();
});

function makeKv(seed: Record<string, string> = {}): {
  kv: KvReader;
  puts: () => number;
  store: Map<string, string>;
} {
  const store = new Map<string, string>(Object.entries(seed));
  let puts = 0;
  const kv: KvReader = {
    get: async (k: string) => store.get(k) ?? null,
    put: async (k: string, v: string) => {
      puts += 1;
      store.set(k, v);
    },
  };
  return { kv, puts: () => puts, store };
}

/** An increment spy returning a fixed post-increment count. */
function makeInc(count: number): {
  increment: (db: RequestD1, t: string) => Promise<{ counted: boolean; count: number }>;
  calls: () => number;
} {
  let calls = 0;
  return {
    increment: async () => {
      calls += 1;
      return { counted: true, count };
    },
    calls: () => calls,
  };
}

describe("burstMargin", () => {
  it("floors at 25 000 for the smallest (free) cap", () => {
    expect(burstMargin(500_000)).toBe(BURST_MARGIN_FLOOR);
    expect(BURST_MARGIN_FLOOR).toBe(25_000);
  });
  it("scales at 5% for large caps", () => {
    expect(burstMargin(20_000_000)).toBe(1_000_000);
  });
});

describe("decideFastPath", () => {
  it("uncapped tier arms with no cache read", async () => {
    const d = await decideFastPath(T, "enterprise", { yearMonth: YM });
    expect(d).toEqual({ arm: true, cachedCount: 0, reason: "uncapped-armed" });
  });

  it("no KV → does not arm", async () => {
    const d = await decideFastPath(T, "free", { yearMonth: YM });
    expect(d.arm).toBe(false);
    expect(d.reason).toBe("no-kv");
  });

  it("KV miss → does not arm", async () => {
    const m = makeKv();
    const d = await decideFastPath(T, "free", { kv: m.kv, yearMonth: YM });
    expect(d.arm).toBe(false);
    expect(d.reason).toBe("kv-miss");
  });

  it("far under cap → arms", async () => {
    const m = makeKv({ [requestKvKey(T, YM)]: JSON.stringify({ count: 1000 }) });
    const d = await decideFastPath(T, "free", { kv: m.kv, yearMonth: YM });
    expect(d.arm).toBe(true);
    expect(d.reason).toBe(null);
    expect(d.cachedCount).toBe(1000);
  });

  it("within burstMargin of the cap → does NOT arm (exact enforcement)", async () => {
    // free cap 500 000, margin 25 000 → count 480 000 leaves headroom 20 000 ≤ margin.
    const m = makeKv({ [requestKvKey(T, YM)]: JSON.stringify({ count: 480_000 }) });
    const d = await decideFastPath(T, "free", { kv: m.kv, yearMonth: YM });
    expect(d.arm).toBe(false);
    expect(d.reason).toBe("near-cap");
  });

  it("exactly at the margin boundary → does NOT arm", async () => {
    // headroom == margin (25 000) → `cap - c <= margin` is true → near-cap.
    const m = makeKv({ [requestKvKey(T, YM)]: JSON.stringify({ count: 475_000 }) });
    const d = await decideFastPath(T, "free", { kv: m.kv, yearMonth: YM });
    expect(d.arm).toBe(false);
  });
});

describe("tryFastRequestCount", () => {
  it("armed → schedules increment, refreshes KV from the RETURNING count (never +1)", async () => {
    const m = makeKv({ [requestKvKey(T, YM)]: JSON.stringify({ count: 1000 }) });
    const inc = makeInc(4242); // authoritative D1 count is unrelated to cached 1000
    const r = await tryFastRequestCount(DB, T, "free", {
      kv: m.kv,
      yearMonth: YM,
      increment: inc.increment,
    });
    expect(r).not.toBeNull();
    expect(inc.calls()).toBe(1);
    // KV holds the AUTHORITATIVE returned count, not cached+1 (never over-count).
    expect(m.store.get(requestKvKey(T, YM))).toBe(JSON.stringify({ count: 4242 }));
  });

  it("not armed (near cap) → returns null and does NOT increment", async () => {
    const m = makeKv({ [requestKvKey(T, YM)]: JSON.stringify({ count: 490_000 }) });
    const inc = makeInc(490_001);
    const r = await tryFastRequestCount(DB, T, "free", {
      kv: m.kv,
      yearMonth: YM,
      increment: inc.increment,
    });
    expect(r).toBeNull();
    expect(inc.calls()).toBe(0); // caller will run the exact sync path instead
  });

  it("uncapped tier arms and meters async without a cached count", async () => {
    const m = makeKv();
    const inc = makeInc(88);
    const r = await tryFastRequestCount(DB, T, "enterprise", {
      kv: m.kv,
      yearMonth: YM,
      increment: inc.increment,
    });
    expect(r).not.toBeNull();
    expect(inc.calls()).toBe(1);
  });

  it("optimistic L1 bump tightens headroom for the next same-isolate read", async () => {
    // Seed just above the near-cap boundary so a single +1 crosses it.
    // cap 500 000, margin 25 000 → boundary at count 475 000. Seed 474 999:
    // first call arms (headroom 25 001 > margin); the +1 bump → 475 000, which is
    // now within-margin, so the immediate next decide must NOT arm.
    const m = makeKv({ [requestKvKey(T, YM)]: JSON.stringify({ count: 474_999 }) });
    const inc = makeInc(474_999);
    const first = await tryFastRequestCount(DB, T, "free", {
      kv: m.kv,
      yearMonth: YM,
      nowMs: 1000,
      increment: inc.increment,
    });
    expect(first).not.toBeNull();
    const next = await decideFastPath(T, "free", { kv: m.kv, yearMonth: YM, nowMs: 1001 });
    expect(next.arm).toBe(false); // L1 now reads 475 000 → within margin
  });
});

describe("populateRequestCountKv", () => {
  it("writes the authoritative count to KV and seeds L1", async () => {
    const m = makeKv();
    populateRequestCountKv(T, 314, { kv: m.kv, yearMonth: YM, nowMs: 1000 });
    // KV write is fire-and-forget; give the microtask a tick.
    await Promise.resolve();
    expect(m.store.get(requestKvKey(T, YM))).toBe(JSON.stringify({ count: 314 }));
    // L1 seeded → an immediate decide reads it (kv present, but L1 short-circuits).
    const d = await decideFastPath(T, "free", { kv: m.kv, yearMonth: YM, nowMs: 1001 });
    expect(d.cachedCount).toBe(314);
    expect(d.arm).toBe(true);
  });
});

describe("month scoping", () => {
  it("a different month is a distinct key (no cross-month leak)", async () => {
    const m = makeKv({ [requestKvKey(T, "2026-07")]: JSON.stringify({ count: 1000 }) });
    // Querying the NEW month sees no value → miss.
    const d = await decideFastPath(T, "free", { kv: m.kv, yearMonth: "2026-08" });
    expect(d.reason).toBe("kv-miss");
  });
});
