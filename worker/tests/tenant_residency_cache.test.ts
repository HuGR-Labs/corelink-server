/**
 * Tests for the tenant data-residency (`primary_region`) three-tier cache
 * (latency WP — collapse the warm `wdb` Server-Timing phase).
 *
 * Unit tests on `resolveTenantResidency` — the three-tier read (L1 isolate →
 * L2 KV → L3 D1), the single-flight, and its FAIL-CLOSED posture (the opposite
 * of the suspend gate): an unresolved region 503s, but a known region survives a
 * transient D1 fault.
 */

import { describe, it, expect, beforeEach } from "vitest";
import type { D1Database } from "@cloudflare/workers-types";
import {
  resolveTenantResidency,
  RESIDENCY_UNRESOLVED,
  residencyKvKey,
  RESIDENCY_CACHE_TTL_MS,
  __resetTenantResidencyCacheForTest,
} from "../src/lib/tenant_residency_cache.js";
import type { KvReader } from "../src/lib/pat_verify_cache.js";

const TEST_TENANT_ID = "00000000-0000-0000-0000-000000000042";

beforeEach(() => {
  // The module-level cache is per-isolate — clear it so a decision from one
  // case does not leak into the next (all cases reuse TEST_TENANT_ID).
  __resetTenantResidencyCacheForTest();
});

/**
 * Minimal D1 mock for the `SELECT primary_region FROM tenant` query.
 * `region` is the row's `primary_region` (`null` = no row / no pin). Counts how
 * many times the query actually hit D1 (to prove caching / single-flight).
 */
function makeRegionD1(opts: {
  region?: string | null;
  noRow?: boolean;
  throwOnRead?: boolean;
}): { db: D1Database; reads: () => number } {
  let reads = 0;
  const { region = null, noRow = false, throwOnRead = false } = opts;
  const db = {
    prepare: (_sql: string) => ({
      bind: (..._args: unknown[]) => ({
        first: async <T>() => {
          reads += 1;
          if (throwOnRead) throw new Error("D1 tenant read error");
          if (noRow) return null;
          return { primary_region: region } as T;
        },
      }),
    }),
  } as unknown as D1Database;
  return { db, reads: () => reads };
}

/** In-memory KV mock (the L2 tier). Backed by a Map; counts get/put. */
function makeKv(seed?: Record<string, string>): {
  kv: KvReader;
  gets: () => number;
  puts: () => number;
  store: Map<string, string>;
  setThrowOnGet: (v: boolean) => void;
} {
  const store = new Map<string, string>(Object.entries(seed ?? {}));
  let gets = 0;
  let puts = 0;
  let throwOnGet = false;
  const kv: KvReader = {
    get: async (key: string) => {
      gets += 1;
      if (throwOnGet) throw new Error("KV get fault");
      return store.get(key) ?? null;
    },
    put: async (key: string, value: string, _o?: { expirationTtl?: number }) => {
      puts += 1;
      store.set(key, value);
    },
  };
  return { kv, gets: () => gets, puts: () => puts, store, setThrowOnGet: (v) => (throwOnGet = v) };
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit: resolveTenantResidency (cache / single-flight / failure posture)
// ─────────────────────────────────────────────────────────────────────────────

describe("resolveTenantResidency", () => {
  it("resolves the region for a tenant WITH a pin", async () => {
    const { db } = makeRegionD1({ region: "weur" });
    expect(await resolveTenantResidency(db, TEST_TENANT_ID, { nowMs: 1_000 })).toEqual({
      region: "weur",
    });
  });

  it("resolves { region: null } for a tenant with NO row (IAD-local fall-through)", async () => {
    const { db } = makeRegionD1({ noRow: true });
    expect(await resolveTenantResidency(db, TEST_TENANT_ID, { nowMs: 1_000 })).toEqual({
      region: null,
    });
  });

  it("resolves { region: null } for a row with a null/empty primary_region", async () => {
    const { db } = makeRegionD1({ region: null });
    expect(await resolveTenantResidency(db, TEST_TENANT_ID, { nowMs: 1_000 })).toEqual({
      region: null,
    });
  });

  it("serves a fresh cache hit without a second D1 read", async () => {
    const { db, reads } = makeRegionD1({ region: "sam" });
    expect(await resolveTenantResidency(db, TEST_TENANT_ID, { nowMs: 1_000 })).toEqual({
      region: "sam",
    });
    // Within TTL → cached, no new read.
    expect(
      await resolveTenantResidency(db, TEST_TENANT_ID, {
        nowMs: 1_000 + RESIDENCY_CACHE_TTL_MS - 1,
      }),
    ).toEqual({ region: "sam" });
    expect(reads()).toBe(1);
  });

  it("re-reads D1 after the L1 TTL expires", async () => {
    const { db, reads } = makeRegionD1({ region: "weur" });
    await resolveTenantResidency(db, TEST_TENANT_ID, { nowMs: 1_000 });
    await resolveTenantResidency(db, TEST_TENANT_ID, {
      nowMs: 1_000 + RESIDENCY_CACHE_TTL_MS + 1,
    });
    expect(reads()).toBe(2);
  });

  it("single-flights concurrent misses into ONE D1 read", async () => {
    const { db, reads } = makeRegionD1({ region: "nrt" });
    const [a, b, c] = await Promise.all([
      resolveTenantResidency(db, TEST_TENANT_ID, { nowMs: 1_000 }),
      resolveTenantResidency(db, TEST_TENANT_ID, { nowMs: 1_000 }),
      resolveTenantResidency(db, TEST_TENANT_ID, { nowMs: 1_000 }),
    ]);
    expect([a, b, c]).toEqual([{ region: "nrt" }, { region: "nrt" }, { region: "nrt" }]);
    expect(reads()).toBe(1);
  });

  it("FAILS CLOSED (UNRESOLVED) on a D1 read error with NO prior knowledge", async () => {
    const { db } = makeRegionD1({ throwOnRead: true });
    // The caller must 503 rather than IAD-leak — never fall through on an unknown.
    expect(await resolveTenantResidency(db, TEST_TENANT_ID, { nowMs: 1_000 })).toBe(
      RESIDENCY_UNRESOLVED,
    );
  });

  it("serves the LAST KNOWN region on a D1 read error (residency is immutable)", async () => {
    // First: a good read establishes the tenant's region.
    const { db: goodDb } = makeRegionD1({ region: "weur" });
    expect(await resolveTenantResidency(goodDb, TEST_TENANT_ID, { nowMs: 1_000 })).toEqual({
      region: "weur",
    });
    // Later (past TTL): the refresh read errors — serve the known region, do NOT
    // 503 an established tenant on a transient fault.
    const { db: errDb } = makeRegionD1({ throwOnRead: true });
    expect(
      await resolveTenantResidency(errDb, TEST_TENANT_ID, {
        nowMs: 1_000 + RESIDENCY_CACHE_TTL_MS + 1,
      }),
    ).toEqual({ region: "weur" });
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// Unit: L2 KV tier (the SAM-latency layer)
// ─────────────────────────────────────────────────────────────────────────────

describe("resolveTenantResidency — L2 KV", () => {
  it("KV HIT (region) → returns it WITHOUT consulting D1", async () => {
    const { kv } = makeKv({ [residencyKvKey(TEST_TENANT_ID)]: JSON.stringify({ region: "weur" }) });
    const { db, reads } = makeRegionD1({ region: "sam" }); // D1 says sam — must NOT be consulted
    expect(await resolveTenantResidency(db, TEST_TENANT_ID, { kv, nowMs: 1_000 })).toEqual({
      region: "weur",
    });
    expect(reads()).toBe(0);
  });

  it("KV HIT (null / no-row verdict) → returns { region: null } WITHOUT D1", async () => {
    const { kv } = makeKv({ [residencyKvKey(TEST_TENANT_ID)]: JSON.stringify({ region: null }) });
    const { db, reads } = makeRegionD1({ region: "weur" });
    expect(await resolveTenantResidency(db, TEST_TENANT_ID, { kv, nowMs: 1_000 })).toEqual({
      region: null,
    });
    expect(reads()).toBe(0);
  });

  it("KV MISS → reads D1 and write-behinds the verdict into KV", async () => {
    const { kv, puts, store } = makeKv();
    const { db, reads } = makeRegionD1({ region: "nrt" });
    expect(await resolveTenantResidency(db, TEST_TENANT_ID, { kv, nowMs: 1_000 })).toEqual({
      region: "nrt",
    });
    expect(reads()).toBe(1);
    expect(puts()).toBe(1);
    expect(store.get(residencyKvKey(TEST_TENANT_ID))).toBe(JSON.stringify({ region: "nrt" }));
  });

  it("KV get FAULT → falls through to D1 (never throws)", async () => {
    const { kv, setThrowOnGet } = makeKv();
    setThrowOnGet(true);
    const { db, reads } = makeRegionD1({ region: "weur" });
    expect(await resolveTenantResidency(db, TEST_TENANT_ID, { kv, nowMs: 1_000 })).toEqual({
      region: "weur",
    });
    expect(reads()).toBe(1);
  });

  it("malformed KV value → treated as a miss (falls through to D1)", async () => {
    const { kv } = makeKv({ [residencyKvKey(TEST_TENANT_ID)]: "{not json" });
    const { db, reads } = makeRegionD1({ region: "sam" });
    expect(await resolveTenantResidency(db, TEST_TENANT_ID, { kv, nowMs: 1_000 })).toEqual({
      region: "sam",
    });
    expect(reads()).toBe(1);
  });
});
