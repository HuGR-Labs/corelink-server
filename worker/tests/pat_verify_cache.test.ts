/**
 * Tests for the per-token PAT-verify cache (perf #99).
 *
 * Two layers:
 *   1. Unit tests on `verifyPatRowCached` — the single-flight + 5 s TTL cache,
 *      the POSITIVE-ONLY rule, and the revocation-window bound (a revoked token
 *      is honored WITHIN the TTL and denied AFTER it — never longer).
 *   2. Integration tests through the Worker handler — the SAME valid PAT twice
 *      does exactly ONE `pat` D1 read (the perf win), a revoked PAT is denied
 *      after the TTL, and expiry is still enforced from the cached row.
 */

import { describe, it, expect, beforeEach } from "vitest";
import type { D1Database, DurableObjectNamespace } from "@cloudflare/workers-types";
import {
  verifyPatRowCached,
  __resetPatVerifyCacheForTest,
  PAT_VERIFY_CACHE_TTL_MS,
  type CachedPatRow,
} from "../src/lib/pat_verify_cache.js";
import workerHandler from "../src/index.js";
import type { Env } from "../src/index.js";
import { TEST_PAT_SIGNING_KEY, mintTestPat } from "./setup.js";
import { batchViaFirst } from "./d1_batch_mock.js";

const TEST_TENANT_ID = "00000000-0000-0000-0000-000000000042";
const TEST_TOKEN_ID = "CCCCCCCCCCCCCCCC";
const TEST_PAT_TOKEN = await mintTestPat({ tokenId: TEST_TOKEN_ID });

type ExecutionContext = {
  waitUntil: (_p: Promise<unknown>) => void;
  passThroughOnException: () => void;
};

beforeEach(() => {
  // The module-level cache is per-isolate — clear it so a decision from one
  // case does not leak into the next.
  __resetPatVerifyCacheForTest();
});

// ─────────────────────────────────────────────────────────────────────────────
// Unit: verifyPatRowCached (cache / single-flight / revocation window)
// ─────────────────────────────────────────────────────────────────────────────

const ROW: CachedPatRow = {
  tenant_id: TEST_TENANT_ID,
  expires_ms: 0, // never-expires sentinel
  scope: "cas:rw",
  runner_job_ac_key: null,
                find_only: null,
};

/**
 * Minimal `pat` D1 mock. `present` decides whether the row is returned (a
 * `null` return models an unknown OR revoked token — the `revoked_at_ms IS
 * NULL` filter is in the SQL). `throwOnRead` models a D1 fault. Counts reads to
 * prove caching / single-flight. `present` is a getter so a case can flip
 * revocation between reads.
 */
function makePatD1(opts: {
  present?: () => boolean;
  throwOnRead?: boolean;
}): { db: D1Database; reads: () => number } {
  let reads = 0;
  const present = opts.present ?? (() => true);
  const throwOnRead = opts.throwOnRead ?? false;
  const db = {
    prepare: (_sql: string) => ({
      bind: (..._args: unknown[]) => ({
        first: async <T>() => {
          reads += 1;
          if (throwOnRead) throw new Error("D1 pat read error");
          return (present() ? ROW : null) as T | null;
        },
      }),
    }),
  } as unknown as D1Database;
  return { db, reads: () => reads };
}

describe("verifyPatRowCached", () => {
  it("returns the row for a present, non-revoked token", async () => {
    const { db } = makePatD1({});
    const r = await verifyPatRowCached(db, TEST_TOKEN_ID, { nowMs: 1_000 });
    expect(r).toEqual({ kind: "found", row: ROW, source: "d1" });
  });

  it("returns not_found for an absent/revoked token", async () => {
    const { db } = makePatD1({ present: () => false });
    const r = await verifyPatRowCached(db, TEST_TOKEN_ID, { nowMs: 1_000 });
    expect(r.kind).toBe("not_found");
  });

  it("serves a fresh hit WITHOUT a second D1 read", async () => {
    const { db, reads } = makePatD1({});
    expect((await verifyPatRowCached(db, TEST_TOKEN_ID, { nowMs: 1_000 })).kind).toBe("found");
    expect(
      (await verifyPatRowCached(db, TEST_TOKEN_ID, { nowMs: 1_000 + PAT_VERIFY_CACHE_TTL_MS - 1 })).kind,
    ).toBe("found");
    expect(reads()).toBe(1);
  });

  it("tags the serving tier as `source` (d1 on the read, l1 on the cached hit) for Server-Timing", async () => {
    const { db } = makePatD1({});
    const first = await verifyPatRowCached(db, TEST_TOKEN_ID, { nowMs: 1_000 });
    expect(first).toEqual({ kind: "found", row: ROW, source: "d1" });
    // Second call within the TTL is served from the per-isolate L1 cache.
    const second = await verifyPatRowCached(db, TEST_TOKEN_ID, { nowMs: 1_500 });
    expect(second).toEqual({ kind: "found", row: ROW, source: "l1" });
  });

  it("does NOT cache a negative — a freshly minted token authenticates immediately", async () => {
    // First read: token not yet in D1 (null). Then it is minted (present flips
    // true). A negative must NOT have been cached, so the next read re-hits D1
    // and now finds it — no ≤TTL delay for a new token.
    let minted = false;
    const { db, reads } = makePatD1({ present: () => minted });
    expect((await verifyPatRowCached(db, TEST_TOKEN_ID, { nowMs: 1_000 })).kind).toBe("not_found");
    minted = true;
    expect((await verifyPatRowCached(db, TEST_TOKEN_ID, { nowMs: 1_050 })).kind).toBe("found");
    expect(reads()).toBe(2); // both reads hit D1 — the null was not cached
  });

  // ── The load-bearing revocation-window test ────────────────────────────────
  it("bounds revocation to the TTL: a mid-entry revoke is honored WITHIN the TTL, DENIED after", async () => {
    let revoked = false;
    const { db } = makePatD1({ present: () => !revoked });
    // t=1000: valid → cached (positive).
    expect((await verifyPatRowCached(db, TEST_TOKEN_ID, { nowMs: 1_000 })).kind).toBe("found");
    // t=1001: admin revokes — D1 would now return null, but the fresh cache
    // entry (<TTL) still serves the token. This is the bounded, documented window.
    revoked = true;
    expect(
      (await verifyPatRowCached(db, TEST_TOKEN_ID, { nowMs: 1_000 + PAT_VERIFY_CACHE_TTL_MS - 1 })).kind,
    ).toBe("found");
    // t=1000+TTL: entry expired → re-read D1 → null → DENIED. Never longer than TTL.
    expect(
      (await verifyPatRowCached(db, TEST_TOKEN_ID, { nowMs: 1_000 + PAT_VERIFY_CACHE_TTL_MS + 1 })).kind,
    ).toBe("not_found");
  });

  it("single-flights concurrent misses into ONE D1 read", async () => {
    const { db, reads } = makePatD1({});
    const [a, b, c] = await Promise.all([
      verifyPatRowCached(db, TEST_TOKEN_ID, { nowMs: 1_000 }),
      verifyPatRowCached(db, TEST_TOKEN_ID, { nowMs: 1_000 }),
      verifyPatRowCached(db, TEST_TOKEN_ID, { nowMs: 1_000 }),
    ]);
    expect([a.kind, b.kind, c.kind]).toEqual(["found", "found", "found"]);
    expect(reads()).toBe(1);
  });

  it("surfaces a D1 fault as `error` and does NOT cache it (self-heals)", async () => {
    const { db: errDb } = makePatD1({ throwOnRead: true });
    expect((await verifyPatRowCached(errDb, TEST_TOKEN_ID, { nowMs: 1_000 })).kind).toBe("error");
    // The error was not cached — a subsequent good read resolves normally.
    const { db: goodDb } = makePatD1({});
    expect((await verifyPatRowCached(goodDb, TEST_TOKEN_ID, { nowMs: 1_050 })).kind).toBe("found");
  });

  it("serves a still-FRESH hit through a transient D1 fault (availability, ≤TTL)", async () => {
    // t=1000: good read → cached.
    const { db: goodDb } = makePatD1({});
    expect((await verifyPatRowCached(goodDb, TEST_TOKEN_ID, { nowMs: 1_000 })).kind).toBe("found");
    // t<TTL: D1 now faults, but the fresh entry is served (no error surfaced) —
    // it was DB-confirmed <TTL ago, so this never extends the revocation window.
    const { db: errDb } = makePatD1({ throwOnRead: true });
    expect(
      (await verifyPatRowCached(errDb, TEST_TOKEN_ID, { nowMs: 1_000 + PAT_VERIFY_CACHE_TTL_MS - 1 })).kind,
    ).toBe("found");
  });

  it("does NOT serve an EXPIRED entry through a D1 fault (window stays bounded)", async () => {
    const { db: goodDb } = makePatD1({});
    expect((await verifyPatRowCached(goodDb, TEST_TOKEN_ID, { nowMs: 1_000 })).kind).toBe("found");
    // Past TTL + D1 faults → must surface `error` (503), NOT serve the stale entry.
    const { db: errDb } = makePatD1({ throwOnRead: true });
    expect(
      (await verifyPatRowCached(errDb, TEST_TOKEN_ID, { nowMs: 1_000 + PAT_VERIFY_CACHE_TTL_MS + 1 })).kind,
    ).toBe("error");
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// Read-replica routing + primary fallback (D1 read replication, perf #99)
// ─────────────────────────────────────────────────────────────────────────────
describe("verifyPatRowCached — replica read with primary fallback", () => {
  it("read-after-write: a replica MISS falls back to the primary (fresh mint authenticates immediately)", async () => {
    // The replica has not replicated the just-minted row yet (returns null); the
    // primary has it. Must NOT 401 a brand-new token.
    const { db: replica } = makePatD1({ present: () => false });
    const { db: primary, reads: pReads } = makePatD1({ present: () => true });
    const r = await verifyPatRowCached(replica, TEST_TOKEN_ID, { primaryDb: primary, nowMs: 1_000 });
    expect(r).toEqual({ kind: "found", row: ROW, source: "d1" });
    expect(pReads()).toBe(1); // the primary WAS consulted on the replica miss
  });

  it("a replica FAULT falls back to the primary (availability)", async () => {
    const { db: replica } = makePatD1({ throwOnRead: true });
    const { db: primary } = makePatD1({ present: () => true });
    const r = await verifyPatRowCached(replica, TEST_TOKEN_ID, { primaryDb: primary, nowMs: 1_000 });
    expect(r.kind).toBe("found");
  });

  it("both replica AND primary absent → not_found (genuine unknown)", async () => {
    const { db: replica } = makePatD1({ present: () => false });
    const { db: primary } = makePatD1({ present: () => false });
    expect((await verifyPatRowCached(replica, TEST_TOKEN_ID, { primaryDb: primary, nowMs: 1_000 })).kind).toBe(
      "not_found",
    );
  });

  it("a HIT on the replica does NOT consult the primary (fast path; revocation staleness honored ≤lag)", async () => {
    // A token revoked on the primary but still present on a stale replica is
    // honored — the primary is never read, so the revocation window is the
    // replication lag, not re-tightened here. This is the accepted ≤lag window.
    const { db: replica } = makePatD1({ present: () => true });
    const { db: primary, reads: pReads } = makePatD1({ present: () => false });
    const r = await verifyPatRowCached(replica, TEST_TOKEN_ID, { primaryDb: primary, nowMs: 1_000 });
    expect(r.kind).toBe("found");
    expect(pReads()).toBe(0); // primary NOT consulted on a replica hit
  });

  it("both handles fault → error (503, fail-closed)", async () => {
    const { db: replica } = makePatD1({ throwOnRead: true });
    const { db: primary } = makePatD1({ throwOnRead: true });
    expect((await verifyPatRowCached(replica, TEST_TOKEN_ID, { primaryDb: primary, nowMs: 1_000 })).kind).toBe(
      "error",
    );
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// L2 KV cache (the SAM latency fix: globally-replicated, per-colo edge cache)
// ─────────────────────────────────────────────────────────────────────────────
function makeKv(opts: { seed?: string | null; throwOnGet?: boolean; throwOnPut?: boolean } = {}) {
  let gets = 0;
  let puts = 0;
  let store: string | null = opts.seed ?? null;
  const kv = {
    get: async (_k: string) => {
      gets += 1;
      if (opts.throwOnGet) throw new Error("KV get fault");
      return store;
    },
    put: async (_k: string, v: string) => {
      puts += 1;
      if (opts.throwOnPut) throw new Error("KV put fault");
      store = v;
    },
  };
  return { kv, gets: () => gets, puts: () => puts, current: () => store };
}

describe("verifyPatRowCached — L2 KV cache", () => {
  it("a KV HIT returns the row WITHOUT touching D1 (the fast global path)", async () => {
    const { kv, gets } = makeKv({ seed: JSON.stringify(ROW) });
    const { db, reads: d1Reads } = makePatD1({});
    const r = await verifyPatRowCached(db, TEST_TOKEN_ID, { kv, nowMs: 1_000 });
    expect(r).toEqual({ kind: "found", row: ROW, source: "kv" });
    expect(gets()).toBe(1);
    expect(d1Reads()).toBe(0); // D1 never consulted on a KV hit
  });

  it("a KV MISS reads D1 and POPULATES KV (best-effort write-behind)", async () => {
    const { kv, puts, current } = makeKv({ seed: null });
    const { db } = makePatD1({});
    const r = await verifyPatRowCached(db, TEST_TOKEN_ID, { kv, nowMs: 1_000 });
    expect(r.kind).toBe("found");
    // allow the void kvPut microtask to settle
    await Promise.resolve();
    expect(puts()).toBe(1);
    expect(JSON.parse(current()!)).toMatchObject({ tenant_id: TEST_TENANT_ID });
  });

  it("a MALFORMED KV value falls through to D1 (never trusts junk)", async () => {
    const { kv } = makeKv({ seed: "not-json{" });
    const { db, reads } = makePatD1({});
    expect((await verifyPatRowCached(db, TEST_TOKEN_ID, { kv, nowMs: 1_000 })).kind).toBe("found");
    expect(reads()).toBe(1); // fell through to D1
  });

  it("a KV GET fault falls through to D1 (availability — KV never breaks auth)", async () => {
    const { kv } = makeKv({ throwOnGet: true });
    const { db, reads } = makePatD1({});
    expect((await verifyPatRowCached(db, TEST_TOKEN_ID, { kv, nowMs: 1_000 })).kind).toBe("found");
    expect(reads()).toBe(1);
  });

  it("does NOT populate KV on a D1 miss (a fresh mint must not be shadowed by a negative)", async () => {
    const { kv, puts } = makeKv({ seed: null });
    const { db } = makePatD1({ present: () => false });
    expect((await verifyPatRowCached(db, TEST_TOKEN_ID, { kv, nowMs: 1_000 })).kind).toBe("not_found");
    await Promise.resolve();
    expect(puts()).toBe(0); // negatives are never cached in KV
  });

  it("a KV PUT failure does NOT break auth (the D1 read already succeeded)", async () => {
    const { kv } = makeKv({ seed: null, throwOnPut: true });
    const { db } = makePatD1({});
    expect((await verifyPatRowCached(db, TEST_TOKEN_ID, { kv, nowMs: 1_000 })).kind).toBe("found");
  });

  it("hands the KV write-behind to waitUntil (so it survives the response, not a cancelled void)", async () => {
    // The bug this guards: a bare `void kv.put(...)` is cancelled when the Worker
    // returns, so KV never warms. With waitUntil the write is registered to
    // outlive the response — assert the promise is handed over AND completes.
    const { kv, puts, current } = makeKv({ seed: null });
    const { db } = makePatD1({});
    const registered: Promise<unknown>[] = [];
    const waitUntil = (p: Promise<unknown>) => registered.push(p);
    const r = await verifyPatRowCached(db, TEST_TOKEN_ID, { kv, waitUntil, nowMs: 1_000 });
    expect(r.kind).toBe("found");
    expect(registered).toHaveLength(1); // the put was handed to waitUntil, not awaited inline
    await Promise.all(registered); // the runtime would drain these post-response
    expect(puts()).toBe(1);
    expect(JSON.parse(current()!)).toMatchObject({ tenant_id: TEST_TENANT_ID });
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// Integration: Worker handler — the perf win + revocation + expiry end-to-end
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Full-path D1 mock. Counts `pat`-table reads so the integration test can prove
 * the cache eliminates the per-request pat read. `patPresent`/`expiresMs` are
 * getters so a case can flip revocation/expiry between requests.
 */
function makeWorkerD1(opts: {
  patPresent?: () => boolean;
  expiresMs?: () => number;
}): { db: D1Database; patReads: () => number } {
  let patReads = 0;
  const patPresent = opts.patPresent ?? (() => true);
  const expiresMs = opts.expiresMs ?? (() => 0);
  const db = {
    prepare: (sql: string) => ({
      bind: (...args: unknown[]) => ({
        first: async <T>() => {
          if (sql.includes("FROM pat")) {
            patReads += 1;
            const tokenId = args[0] as string;
            if (tokenId === TEST_TOKEN_ID && patPresent()) {
              return {
                tenant_id: TEST_TENANT_ID,
                expires_ms: expiresMs(),
                scope: "cas:rw",
                runner_job_ac_key: null,
                find_only: null,
              } as T | null;
            }
            return null as T | null;
          }
          if (sql.includes("tenant_offboarding_state")) return null as T | null;
          if (sql.includes("tier_selections")) return null as T | null;
          if (sql.includes("FROM tenant") && sql.includes("tier")) return null as T | null;
          if (sql.includes("tenant_storage_state")) return { total_bytes: 0 } as T | null;
          if (sql.includes("monthly_request_counts")) return { request_count: 1 } as T | null;
          return null as T | null;
        },
        all: async <T>() => ({ success: true as const, meta: {} as never, results: [] as T[] }),
        run: async <T>() => ({ success: true as const, meta: {} as never, results: [] as T[] }),
        raw: async <T>() => [] as T[],
      }),
      first: async <T>() => null as T | null,
      all: async <T>() => ({ success: true as const, meta: {} as never, results: [] as T[] }),
      run: async <T>() => ({ success: true as const, meta: {} as never, results: [] as T[] }),
      raw: async <T>() => [] as T[],
    }),
    // Both quota statements travel as ONE `db.batch` round trip
    // (`runQuotaBatch`); resolve them through this mock's own routing.
    batch: batchViaFirst(),
    exec: async () => ({ count: 0, duration: 0 }),
    withSession() { return this; },
    dump: async () => new ArrayBuffer(0),
  } as unknown as D1Database;
  return { db, patReads: () => patReads };
}

function makeEnv(d1: D1Database, doStubStatus = 503): Env {
  const stub = {
    fetch: async (_req: Request): Promise<Response> =>
      new Response(
        JSON.stringify({ error: "CONTAINER_UNAVAILABLE", message: "stub", request_id: "stub" }),
        { status: doStubStatus, headers: { "Content-Type": "application/json" } },
      ),
  };
  const namespace = {
    idFromName: (_n: string) => ({ toString: () => "stub-id" }),
    get: (_id: unknown) => stub,
    idFromString: (_s: string) => ({ toString: () => "stub-id" }),
    newUniqueId: () => ({ toString: () => "stub-unique-id" }),
    jurisdiction: function (_j: string) { return this; },
  } as unknown as DurableObjectNamespace;
  return {
    CORELINK_SERVER: namespace,
    ENVIRONMENT: "test",
    CONFIG_DB: d1,
    PAT_SIGNING_KEY: TEST_PAT_SIGNING_KEY,
  } as unknown as Env;
}

async function workerFetch(url: string, init: RequestInit, env: Env): Promise<Response> {
  const req = new Request(url, init);
  const ctx = { waitUntil: () => {}, passThroughOnException: () => {} } as unknown as ExecutionContext;
  return workerHandler.fetch!(req, env, ctx);
}

const CAS_URL = "http://localhost/v1/cas/blobs/sha256:abc123/1024";

describe("PAT-verify cache — Worker hot path", () => {
  it("the SAME valid PAT twice does exactly ONE pat D1 read (the perf win)", async () => {
    const { db, patReads } = makeWorkerD1({});
    const env = makeEnv(db);
    const r1 = await workerFetch(CAS_URL, { headers: { Authorization: `Bearer ${TEST_PAT_TOKEN}` } }, env);
    const r2 = await workerFetch(CAS_URL, { headers: { Authorization: `Bearer ${TEST_PAT_TOKEN}` } }, env);
    // Both reach the DO stub (503) — i.e. auth passed both times.
    expect(r1.status).not.toBe(401);
    expect(r2.status).not.toBe(401);
    // The second request was served from the verify cache — no second pat read.
    expect(patReads()).toBe(1);
  });

  it("still enforces expiry from the CACHED row (an expired PAT is 401, cache or not)", async () => {
    // expires_ms in the past → extractAuth's expiry check rejects even on a hit.
    const { db } = makeWorkerD1({ expiresMs: () => 1 });
    const env = makeEnv(db);
    const resp = await workerFetch(CAS_URL, { headers: { Authorization: `Bearer ${TEST_PAT_TOKEN}` } }, env);
    expect(resp.status).toBe(401);
  });

  it("denies a valid PAT once its D1 row is gone (unknown/revoked token → 401)", async () => {
    const { db } = makeWorkerD1({ patPresent: () => false });
    const env = makeEnv(db);
    const resp = await workerFetch(CAS_URL, { headers: { Authorization: `Bearer ${TEST_PAT_TOKEN}` } }, env);
    expect(resp.status).toBe(401);
  });
});
