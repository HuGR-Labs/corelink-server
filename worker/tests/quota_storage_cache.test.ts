/**
 * Tests for the STORAGE-SUM three-tier cache (latency WP-B1) — the last of the
 * quota trio to leave the warm READ path.
 *
 * Covers: L1 → L2 KV → L3 D1 read, single-flight, the 60 s KV TTL, the FAIL-OPEN
 * posture (a `d1Error` SUM is returned unchanged and NEVER cached), and the
 * `checkStorageQuotaCachedRead` verdict — which must be byte-identical to the live
 * `storageResultForBytes` from the same bytes + tier.
 */

import { describe, it, expect, beforeEach } from "vitest";
import type { KvReader } from "../src/lib/pat_verify_cache.js";
import { storageResultForBytes, type Tier } from "../src/lib/quota.js";
import {
  resolveStorageBytesCached,
  checkStorageQuotaCachedRead,
  storageKvKey,
  KV_STORAGE_TTL_S,
  __resetStorageCacheForTests,
  type StorageBytesResult,
  type StorageD1,
} from "../src/lib/quota_storage_cache.js";

const T = "00000000-0000-0000-0000-000000000042";
const DB = {} as unknown as StorageD1;

beforeEach(() => {
  __resetStorageCacheForTests();
});

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

function makeFetch(result: StorageBytesResult): {
  fetch: (db: StorageD1, tenantId: string) => Promise<StorageBytesResult>;
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

describe("resolveStorageBytesCached", () => {
  it("L1 hit within TTL skips D1", async () => {
    const f = makeFetch({ totalBytes: 123, d1Error: false });
    const r1 = await resolveStorageBytesCached(DB, T, { fetch: f.fetch, nowMs: 1000 });
    const r2 = await resolveStorageBytesCached(DB, T, { fetch: f.fetch, nowMs: 2000 });
    expect(r1.totalBytes).toBe(123);
    expect(r2.totalBytes).toBe(123);
    expect(f.calls()).toBe(1); // second served from L1
  });

  it("writes L1 + KV on an L3 miss", async () => {
    const f = makeFetch({ totalBytes: 999, d1Error: false });
    const m = makeKv();
    await resolveStorageBytesCached(DB, T, { fetch: f.fetch, kv: m.kv, nowMs: 1000 });
    expect(m.puts()).toBe(1);
    expect(m.store.get(storageKvKey(T))).toBe(JSON.stringify({ bytes: 999 }));
  });

  it("L2 KV hit populates L1 and skips D1", async () => {
    const f = makeFetch({ totalBytes: 0, d1Error: false });
    const m = makeKv({ seed: { [storageKvKey(T)]: JSON.stringify({ bytes: 555 }) } });
    const r = await resolveStorageBytesCached(DB, T, { fetch: f.fetch, kv: m.kv, nowMs: 1000 });
    expect(r.totalBytes).toBe(555);
    expect(f.calls()).toBe(0); // never hit D1
  });

  it("d1Error is returned unchanged and NEVER cached", async () => {
    const f = makeFetch({ totalBytes: 0, d1Error: true });
    const m = makeKv();
    const r = await resolveStorageBytesCached(DB, T, { fetch: f.fetch, kv: m.kv, nowMs: 1000 });
    expect(r.d1Error).toBe(true);
    expect(m.puts()).toBe(0); // not written to KV
    // Next call re-fetches (not pinned to the error).
    await resolveStorageBytesCached(DB, T, { fetch: f.fetch, kv: m.kv, nowMs: 1100 });
    expect(f.calls()).toBe(2);
  });

  it("single-flight collapses concurrent misses to one fetch", async () => {
    let calls = 0;
    const slow = async (): Promise<StorageBytesResult> => {
      calls += 1;
      await new Promise((r) => setTimeout(r, 5));
      return { totalBytes: 7, d1Error: false };
    };
    const [a, b] = await Promise.all([
      resolveStorageBytesCached(DB, T, { fetch: slow, nowMs: 1000 }),
      resolveStorageBytesCached(DB, T, { fetch: slow, nowMs: 1000 }),
    ]);
    expect(a.totalBytes).toBe(7);
    expect(b.totalBytes).toBe(7);
    expect(calls).toBe(1);
  });

  it("KV get fault falls through to D1 without throwing", async () => {
    const f = makeFetch({ totalBytes: 42, d1Error: false });
    const m = makeKv({ getThrows: true });
    const r = await resolveStorageBytesCached(DB, T, { fetch: f.fetch, kv: m.kv, nowMs: 1000 });
    expect(r.totalBytes).toBe(42);
    expect(f.calls()).toBe(1);
  });

  it("uses the configured KV TTL on write", async () => {
    expect(KV_STORAGE_TTL_S).toBe(60);
  });
});

describe("checkStorageQuotaCachedRead — verdict parity", () => {
  it("unconfirmed tier → ok:true, no fetch (read-side fail-open)", async () => {
    const f = makeFetch({ totalBytes: 1e12, d1Error: false });
    const r = await checkStorageQuotaCachedRead(DB, T, "free", true, true, { fetch: f.fetch });
    expect(r.ok).toBe(true);
    expect(f.calls()).toBe(0);
  });

  it("unlimited-storage tier → ok:true, no SUM", async () => {
    const f = makeFetch({ totalBytes: 1e15, d1Error: false });
    const r = await checkStorageQuotaCachedRead(DB, T, "enterprise", false, false, { fetch: f.fetch });
    expect(r.ok).toBe(true);
    expect(f.calls()).toBe(0);
  });

  it("under-cap bytes → same verdict as storageResultForBytes", async () => {
    const tier: Tier = "free";
    const bytes = 1_000_000; // well under free's 10 GiB
    const f = makeFetch({ totalBytes: bytes, d1Error: false });
    const r = await checkStorageQuotaCachedRead(DB, T, tier, false, true, { fetch: f.fetch });
    expect(r).toEqual(storageResultForBytes(bytes, tier));
    expect(r.ok).toBe(true);
  });

  it("over-cap bytes → ok:false with the same reason string as the live path", async () => {
    const tier: Tier = "free";
    const bytes = 20 * 1_073_741_824; // 20 GiB > free 10 GiB cap
    const f = makeFetch({ totalBytes: bytes, d1Error: false });
    const r = await checkStorageQuotaCachedRead(DB, T, tier, false, true, { fetch: f.fetch });
    const live = storageResultForBytes(bytes, tier);
    expect(r).toEqual(live);
    expect(r.ok).toBe(false);
  });

  it("SUM fault → read-side fail-open (ok:true), not cached", async () => {
    const f = makeFetch({ totalBytes: 0, d1Error: true });
    const r = await checkStorageQuotaCachedRead(DB, T, "free", false, true, { fetch: f.fetch });
    expect(r.ok).toBe(true);
  });
});
