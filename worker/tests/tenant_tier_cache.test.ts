/**
 * Tests for the tenant TIER three-tier cache (latency WP slice 2 — collapse the
 * `wdb` quota-trio tier read on the CAS/AC hot path).
 *
 * Unit tests on `resolveTenantTierCached`: the L1 isolate → L2 KV → L3 D1 read,
 * single-flight, the 60 s KV TTL, and its FAIL-OPEN posture (an unconfirmed
 * `d1Error` result is returned unchanged and NEVER cached).
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import type { KvReader } from "../src/lib/pat_verify_cache.js";
import type { TierResult, TierD1 } from "../src/lib/tenant_tier_cache.js";
import {
  resolveTenantTierCached,
  tierKvKey,
  KV_TIER_TTL_S,
  __resetTierCacheForTests,
} from "../src/lib/tenant_tier_cache.js";

const T = "00000000-0000-0000-0000-000000000042";
// A stand-in D1 handle — the injected `fetch` never touches it.
const DB = {} as unknown as TierD1;

beforeEach(() => {
  __resetTierCacheForTests();
});

/** In-memory KV mock (the L2 tier); counts get/put and can fault. */
function makeKv(opts: { seed?: Record<string, string>; getThrows?: boolean; putThrows?: boolean } = {}): {
  kv: KvReader;
  gets: () => number;
  puts: () => number;
  store: Map<string, string>;
} {
  const store = new Map<string, string>(Object.entries(opts.seed ?? {}));
  let gets = 0;
  let puts = 0;
  const kv: KvReader = {
    get: async (k: string) => {
      gets += 1;
      if (opts.getThrows) throw new Error("KV get fault");
      return store.get(k) ?? null;
    },
    put: async (k: string, v: string) => {
      puts += 1;
      if (opts.putThrows) throw new Error("KV put fault");
      store.set(k, v);
    },
  };
  return { kv, gets: () => gets, puts: () => puts, store };
}

/** A fetch spy for the L3 D1 layer: returns a fixed TierResult, counts calls. */
function makeFetch(result: TierResult): {
  fetch: (db: TierD1, tenantId: string) => Promise<TierResult>;
  calls: () => number;
} {
  let calls = 0;
  return {
    fetch: async () => {
      calls += 1;
      return result;
    },
    calls: () => calls,
  };
}

describe("resolveTenantTierCached", () => {
  it("L1 hit: same tenant twice → D1 fetched ONCE", async () => {
    const f = makeFetch({ tier: "pro", d1Error: false });
    const a = await resolveTenantTierCached(DB, T, { fetch: f.fetch });
    const b = await resolveTenantTierCached(DB, T, { fetch: f.fetch });
    expect(a).toEqual({ tier: "pro", d1Error: false });
    expect(b).toEqual({ tier: "pro", d1Error: false });
    expect(f.calls()).toBe(1);
  });

  it("KV L2 hit → D1 NOT consulted + populates L1", async () => {
    const { kv, gets } = makeKv({ seed: { [tierKvKey(T)]: JSON.stringify({ tier: "team" }) } });
    const f = makeFetch({ tier: "free", d1Error: false }); // would differ if D1 were hit
    const r = await resolveTenantTierCached(DB, T, { kv, fetch: f.fetch });
    expect(r).toEqual({ tier: "team", d1Error: false });
    expect(f.calls()).toBe(0);
    expect(gets()).toBe(1);
    // second call now served from L1 — no KV get either
    const r2 = await resolveTenantTierCached(DB, T, { kv, fetch: f.fetch });
    expect(r2.tier).toBe("team");
    expect(gets()).toBe(1);
  });

  it("KV miss → D1 fetch + writes BOTH L1 and KV", async () => {
    const { kv, puts, store } = makeKv();
    const f = makeFetch({ tier: "starter", d1Error: false });
    const r = await resolveTenantTierCached(DB, T, { kv, fetch: f.fetch });
    expect(r.tier).toBe("starter");
    expect(f.calls()).toBe(1);
    expect(puts()).toBe(1);
    expect(store.get(tierKvKey(T))).toBe(JSON.stringify({ tier: "starter" }));
  });

  it("d1Error result is returned unchanged and NEVER cached", async () => {
    const { kv, puts } = makeKv();
    const f = makeFetch({ tier: "free", d1Error: true });
    const r = await resolveTenantTierCached(DB, T, { kv, fetch: f.fetch });
    expect(r).toEqual({ tier: "free", d1Error: true });
    expect(puts()).toBe(0); // never written to KV
    // next call must RE-FETCH (not served from a poisoned cache)
    const r2 = await resolveTenantTierCached(DB, T, { kv, fetch: f.fetch });
    expect(r2.d1Error).toBe(true);
    expect(f.calls()).toBe(2);
  });

  it("malformed KV value → miss → D1", async () => {
    const { kv } = makeKv({ seed: { [tierKvKey(T)]: "not json" } });
    const f = makeFetch({ tier: "pro", d1Error: false });
    const r = await resolveTenantTierCached(DB, T, { kv, fetch: f.fetch });
    expect(r.tier).toBe("pro");
    expect(f.calls()).toBe(1);
  });

  it("unknown tier string in KV → miss → D1", async () => {
    const { kv } = makeKv({ seed: { [tierKvKey(T)]: JSON.stringify({ tier: "platinum-bogus" }) } });
    const f = makeFetch({ tier: "team", d1Error: false });
    const r = await resolveTenantTierCached(DB, T, { kv, fetch: f.fetch });
    expect(r.tier).toBe("team");
    expect(f.calls()).toBe(1);
  });

  it("KV get fault → falls through to D1 (fail-safe)", async () => {
    const { kv } = makeKv({ getThrows: true });
    const f = makeFetch({ tier: "max", d1Error: false });
    const r = await resolveTenantTierCached(DB, T, { kv, fetch: f.fetch });
    expect(r.tier).toBe("max");
    expect(f.calls()).toBe(1);
  });

  it("KV put fault → request still succeeds", async () => {
    const { kv } = makeKv({ putThrows: true });
    const f = makeFetch({ tier: "pro", d1Error: false });
    const r = await resolveTenantTierCached(DB, T, { kv, fetch: f.fetch });
    expect(r.tier).toBe("pro");
  });

  it("write-behind is handed to waitUntil when provided", async () => {
    const { kv } = makeKv();
    const f = makeFetch({ tier: "starter", d1Error: false });
    const waitUntil = vi.fn();
    await resolveTenantTierCached(DB, T, { kv, fetch: f.fetch, waitUntil });
    expect(waitUntil).toHaveBeenCalledTimes(1);
  });

  it("single-flight: concurrent misses for one tenant share ONE D1 fetch", async () => {
    let calls = 0;
    const slowFetch = async (): Promise<TierResult> => {
      calls += 1;
      await new Promise((r) => setTimeout(r, 5));
      return { tier: "pro", d1Error: false };
    };
    const [a, b, c] = await Promise.all([
      resolveTenantTierCached(DB, T, { fetch: slowFetch }),
      resolveTenantTierCached(DB, T, { fetch: slowFetch }),
      resolveTenantTierCached(DB, T, { fetch: slowFetch }),
    ]);
    expect([a.tier, b.tier, c.tier]).toEqual(["pro", "pro", "pro"]);
    expect(calls).toBe(1);
  });

  it("L1 entry expires after its TTL → re-fetch", async () => {
    const f = makeFetch({ tier: "pro", d1Error: false });
    await resolveTenantTierCached(DB, T, { fetch: f.fetch, nowMs: 1_000 });
    // within TTL: served from L1
    await resolveTenantTierCached(DB, T, { fetch: f.fetch, nowMs: 1_000 + 4_000 });
    expect(f.calls()).toBe(1);
    // past the 5 s L1 TTL: re-fetch
    await resolveTenantTierCached(DB, T, { fetch: f.fetch, nowMs: 1_000 + 6_000 });
    expect(f.calls()).toBe(2);
  });

  it("KV TTL is pinned to the uniform 60 s auth-freshness window (ADR-0070)", () => {
    expect(KV_TIER_TTL_S).toBe(60);
  });
});
