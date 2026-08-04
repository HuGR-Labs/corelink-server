/**
 * `runQuotaBatch` — the two uncached quota statements in ONE D1 round trip.
 *
 * # What is under test, and why it is worth a file of its own
 *
 * The Worker's quota gate used to await the monthly-counter UPSERT and the
 * storage `SUM(bytes_used)` read one after the other. Measured warm against live
 * prod (n=30, single reused connection, 2026-08-04) those two WERE the whole
 * `wdb` phase — `qmeter` 152/158/163 ms plus `qstor` 120/126/130 ms against a
 * `wdb` of 277/284/302 ms, sum − wdb = 0 on 30 of 30 requests — so the seriality
 * was a full extra ENAM round trip bought for nothing: neither statement reads
 * the other's result.
 *
 * Collapsing them is a LATENCY change and must be nothing else. The counter is a
 * billing/quota counter: its post-increment value is the 429 decision, a lost
 * increment under-bills and lets a tenant past a contracted cap, and a duplicated
 * one wrongly 429s a paying customer. So these tests pin, separately:
 *
 *   1. the round-trip COUNT and the batch's composition (the actual win), and
 *   2. that everything else survives it — what is counted, how often, the
 *      verb-aware D1-failure posture, and the skip rules.
 *
 * The failure this file is really guarding against is the quiet one: a batched
 * path that looks right and answers differently from the serial one it replaced.
 * That is why the D1 mock resolves BATCHED statements through the SAME routing
 * the serial path uses, rather than getting its own hand-written answers.
 */

import { describe, it, expect } from "vitest";
import type { D1Database, DurableObjectNamespace } from "@cloudflare/workers-types";
import { runQuotaBatch, QUOTAS, type TierResult } from "../src/lib/quota.js";
import workerHandler from "../src/index.js";
import type { Env } from "../src/index.js";
import { TEST_PAT_SIGNING_KEY, mintTestPat } from "./setup.js";

const TEST_TENANT_ID = "00000000-0000-0000-0000-000000000001";
const TEST_TOKEN_ID = "AAAAAAAAAAAAAAAA";
const TEST_PAT_TOKEN = await mintTestPat({ tokenId: TEST_TOKEN_ID });

const METER_SQL = "INSERT INTO monthly_request_counts";
const STORAGE_SQL = "SUM(bytes_used)";

/** A confirmed (non-D1-error) tier resolution. */
const confirmed = (tier: TierResult["tier"]): TierResult => ({ tier, d1Error: false });
/** An OUTAGE-derived tier resolution (F21): the tier is not a confirmed value. */
const unconfirmed: TierResult = { tier: "free", d1Error: true };

const MUTATING = true;
const READING = false;

interface MockOpts {
  /** Post-increment count the metering UPSERT returns. */
  requestCount?: number;
  /** Bytes the storage SUM returns (`null` ⇒ no row at all). */
  storageBytes?: number | null;
  /** Make `db.batch()` reject, as D1 does when the transaction fails. */
  throwOnBatch?: boolean;
  /** Return FEWER results than statements — a malformed/partial batch response. */
  truncateBatchResults?: boolean;
}

interface MockCalls {
  /** SQL of every statement submitted to `db.batch()`, one array per call. */
  batches: string[][];
  /** SQL of every statement executed OUTSIDE a batch (a serial round trip). */
  serial: string[];
}

/**
 * D1 mock recording exactly how each statement reached the database. `batch`
 * resolves through the same per-SQL routing as the serial `first()` path, so a
 * batched read and a serial read of the same statement cannot disagree — the
 * mock cannot hide a divergence between the two code paths.
 */
function makeD1(opts: MockOpts = {}): { db: D1Database; calls: MockCalls } {
  const {
    requestCount = 1,
    storageBytes = 0,
    throwOnBatch = false,
    truncateBatchResults = false,
  } = opts;
  const calls: MockCalls = { batches: [], serial: [] };

  const rowFor = (sql: string): unknown => {
    if (sql.includes(METER_SQL)) return { request_count: requestCount };
    if (sql.includes(STORAGE_SQL)) {
      return storageBytes === null ? null : { total_bytes: storageBytes };
    }
    if (sql.includes("FROM pat")) {
      return { tenant_id: TEST_TENANT_ID, expires_ms: Date.now() + 3_600_000, scope: "cas:rw" };
    }
    return null;
  };

  const db = {
    prepare: (sql: string) => ({
      bind: (..._args: unknown[]) => ({
        __sql: sql,
        first: async <T>() => {
          calls.serial.push(sql);
          return rowFor(sql) as T | null;
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
    batch: async (statements: Array<{ __sql: string }>) => {
      calls.batches.push(statements.map((s) => s.__sql));
      if (throwOnBatch) throw new Error("D1_ERROR: batch failed");
      const results = statements.map((s) => {
        const row = rowFor(s.__sql);
        return {
          success: true as const,
          meta: {} as never,
          results: row === null ? [] : [row],
        };
      });
      return truncateBatchResults ? results.slice(0, 1) : results;
    },
    exec: async () => ({ count: 0, duration: 0 }),
    withSession() {
      return this;
    },
    dump: async () => new ArrayBuffer(0),
  } as unknown as D1Database;

  return { db, calls };
}

// ─────────────────────────────────────────────────────────────────────────────
// The round trip itself
// ─────────────────────────────────────────────────────────────────────────────

describe("runQuotaBatch — one round trip", () => {
  it("sends BOTH statements in a single batch and nothing serially", async () => {
    const { db, calls } = makeD1();
    const result = await runQuotaBatch(db, TEST_TENANT_ID, confirmed("free"), {
      meter: true,
      isMutating: MUTATING,
    });

    expect(
      calls.batches.length,
      `expected ONE db.batch round trip, saw ${calls.batches.length}: ${JSON.stringify(calls.batches)}`,
    ).toBe(1);
    expect(calls.batches[0]).toHaveLength(2);
    expect(calls.batches[0]![0]).toContain(METER_SQL);
    expect(calls.batches[0]![1]).toContain(STORAGE_SQL);
    expect(
      calls.serial,
      `a quota statement still went out on its own round trip: ${JSON.stringify(calls.serial)}`,
    ).toHaveLength(0);
    expect(result.ranD1).toBe(true);
  });

  it("counts the request EXACTLY once — the UPSERT appears once, never twice", async () => {
    // The UPSERT is additive (`request_count + 1`), not idempotent: a second
    // execution over-bills and can wrongly 429 a paying tenant.
    const { db, calls } = makeD1({ requestCount: 42 });
    const result = await runQuotaBatch(db, TEST_TENANT_ID, confirmed("free"), {
      meter: true,
      isMutating: READING,
    });

    const meterStatements = [...calls.batches.flat(), ...calls.serial].filter((sql) =>
      sql.includes(METER_SQL),
    );
    expect(meterStatements).toHaveLength(1);
    // ...and the post-increment count from RETURNING is what the caller enforces on.
    expect(result.increment).toEqual({ counted: true, count: 42 });
  });

  it("omits the metering statement when the caller says not to meter (fan-out / kill-switch)", async () => {
    const { db, calls } = makeD1();
    const result = await runQuotaBatch(db, TEST_TENANT_ID, confirmed("free"), {
      meter: false,
      isMutating: MUTATING,
    });

    expect(calls.batches).toHaveLength(1);
    expect(calls.batches[0]).toEqual([expect.stringContaining(STORAGE_SQL)]);
    expect(result.increment).toEqual({ counted: false, count: 0 });
    expect(result.storage.ok).toBe(true);
  });

  it("omits the storage statement for an unlimited-storage tier", async () => {
    const { db, calls } = makeD1();
    expect(QUOTAS.enterprise.storageBytesMax).toBe(Number.MAX_SAFE_INTEGER);

    const result = await runQuotaBatch(db, TEST_TENANT_ID, confirmed("enterprise"), {
      meter: true,
      isMutating: MUTATING,
    });

    expect(calls.batches).toHaveLength(1);
    expect(calls.batches[0]).toEqual([expect.stringContaining(METER_SQL)]);
    expect(result.storage.ok).toBe(true);
    expect(result.increment.counted).toBe(true);
  });

  it("issues NO round trip at all when both statements are skipped", async () => {
    const { db, calls } = makeD1();
    const result = await runQuotaBatch(db, TEST_TENANT_ID, confirmed("enterprise"), {
      meter: false,
      isMutating: READING,
    });

    expect(calls.batches, "an empty batch is a round trip that buys nothing").toHaveLength(0);
    expect(calls.serial).toHaveLength(0);
    expect(result.ranD1, "ranD1 must be false so the phase is omitted, not reported as 0").toBe(
      false,
    );
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// Verdicts — unchanged from the serial pair
// ─────────────────────────────────────────────────────────────────────────────

describe("runQuotaBatch — verdicts", () => {
  it("passes a tenant under the storage cap", async () => {
    const { db } = makeD1({ storageBytes: 1_000 });
    const result = await runQuotaBatch(db, TEST_TENANT_ID, confirmed("free"), {
      meter: true,
      isMutating: MUTATING,
    });
    expect(result.storage).toEqual({ ok: true });
  });

  it("rejects a tenant AT/over the storage cap with the unchanged reason string", async () => {
    const max = QUOTAS.free.storageBytesMax;
    const { db } = makeD1({ storageBytes: max });
    const result = await runQuotaBatch(db, TEST_TENANT_ID, confirmed("free"), {
      meter: true,
      isMutating: MUTATING,
    });

    expect(result.storage.ok).toBe(false);
    if (result.storage.ok) return;
    expect(result.storage.reason).toBe(
      `Storage quota exceeded: ${max} bytes used, limit is ${max} bytes (tier: free)`,
    );
    expect(result.storage.retryAfterSec).toBeGreaterThan(0);
  });

  it("treats a missing storage row as 0 bytes used (a tenant that has stored nothing)", async () => {
    const { db } = makeD1({ storageBytes: null });
    const result = await runQuotaBatch(db, TEST_TENANT_ID, confirmed("free"), {
      meter: true,
      isMutating: MUTATING,
    });
    expect(result.storage).toEqual({ ok: true });
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// D1 failure — the posture that must NOT change
// ─────────────────────────────────────────────────────────────────────────────

describe("runQuotaBatch — D1 failure posture (CAA-360 #25 / F21)", () => {
  it("fails OPEN on a READ when the batch throws, and does not count the request", async () => {
    const { db, calls } = makeD1({ throwOnBatch: true });
    const result = await runQuotaBatch(db, TEST_TENANT_ID, confirmed("free"), {
      meter: true,
      isMutating: READING,
    });

    expect(result.storage, "a read must stay available during a store outage").toEqual({ ok: true });
    expect(
      result.increment,
      "an uncounted request is the fail-open direction this counter already tolerates; " +
        "counting one that may not have committed would be the over-bill direction",
    ).toEqual({ counted: false, count: 0 });
    expect(result.ranD1, "a round trip was attempted, so the phase must still report").toBe(true);
    expect(calls.batches).toHaveLength(1);
  });

  it("fails CLOSED on a WRITE when the batch throws, with the short Retry-After", async () => {
    const { db } = makeD1({ throwOnBatch: true });
    const result = await runQuotaBatch(db, TEST_TENANT_ID, confirmed("free"), {
      meter: true,
      isMutating: MUTATING,
    });

    expect(result.storage.ok).toBe(false);
    if (result.storage.ok) return;
    expect(result.storage.reason).toBe(
      "storage quota temporarily unverifiable (store error); retry",
    );
    expect(result.storage.retryAfterSec).toBe(2);
  });

  it("does NOT read a missing batch result as '0 bytes used' — a truncated response fails closed on a write", async () => {
    // The trap this guards: `results[1]` absent, read as an empty SUM, is
    // indistinguishable from a tenant storing nothing — i.e. fail-OPEN past the
    // storage cap on a malformed response.
    const { db } = makeD1({ truncateBatchResults: true });
    const result = await runQuotaBatch(db, TEST_TENANT_ID, confirmed("free"), {
      meter: true,
      isMutating: MUTATING,
    });

    expect(result.storage.ok).toBe(false);
    if (result.storage.ok) return;
    expect(result.storage.reason).toBe(
      "storage quota temporarily unverifiable (store error); retry",
    );
  });

  it("skips the storage read entirely when the TIER is unconfirmed (F21), but still meters", async () => {
    const { db, calls } = makeD1();
    const read = await runQuotaBatch(db, TEST_TENANT_ID, unconfirmed, {
      meter: true,
      isMutating: READING,
    });

    // No SUM: combining an error-derived 'free' cap with a real byte count is
    // exactly the false-positive 429 F21 exists to prevent.
    expect(calls.batches[0]!.some((sql) => sql.includes(STORAGE_SQL))).toBe(false);
    expect(calls.batches[0]!.some((sql) => sql.includes(METER_SQL))).toBe(true);
    expect(read.storage).toEqual({ ok: true });

    const { db: db2 } = makeD1();
    const write = await runQuotaBatch(db2, TEST_TENANT_ID, unconfirmed, {
      meter: true,
      isMutating: MUTATING,
    });
    expect(write.storage.ok, "a byte-adding write on an unconfirmed cap fails closed").toBe(false);
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// End to end through the Worker
// ─────────────────────────────────────────────────────────────────────────────

function makeEnv(db: D1Database, doStatus = 200): Env {
  const stub = {
    fetch: async (): Promise<Response> =>
      new Response(JSON.stringify({ ok: true }), {
        status: doStatus,
        headers: { "Content-Type": "application/json" },
      }),
  };
  return {
    CORELINK_SERVER: {
      idFromName: (_n: string) => ({ toString: () => "id" }),
      get: (_id: unknown) => stub,
      idFromString: (_s: string) => ({ toString: () => "id" }),
      newUniqueId: () => ({ toString: () => "unique-id" }),
      jurisdiction: function (_j: string) {
        return this;
      },
    } as unknown as DurableObjectNamespace,
    ENVIRONMENT: "test",
    CONFIG_DB: db,
    PAT_SIGNING_KEY: TEST_PAT_SIGNING_KEY,
  } as Env;
}

async function fetchAs(method: string, env: Env): Promise<Response> {
  const ctx = {
    waitUntil: () => {},
    passThroughOnException: () => {},
  } as unknown as ExecutionContext;
  return workerHandler.fetch!(
    new Request(`http://localhost/v1/cas/${TEST_TENANT_ID}/${"a".repeat(64)}`, {
      method,
      headers: { Authorization: `Bearer ${TEST_PAT_TOKEN}` },
      ...(method === "PUT" ? { body: "hello" } : {}),
    }),
    env,
    ctx,
  );
}

describe("Worker quota gate — batched", () => {
  it("spends exactly ONE D1 round trip on the two quota statements per request", async () => {
    const { db, calls } = makeD1();
    const resp = await fetchAs("PUT", makeEnv(db));

    expect(resp.status).toBe(200);
    expect(
      calls.batches,
      `the quota gate issued ${calls.batches.length} batch round trips: ${JSON.stringify(calls.batches)}`,
    ).toHaveLength(1);
    expect(calls.batches[0]).toHaveLength(2);
    expect(
      calls.serial.filter((sql) => sql.includes(METER_SQL) || sql.includes(STORAGE_SQL)),
      "a quota statement escaped the batch onto its own round trip",
    ).toHaveLength(0);
  });

  it("keeps the D1-outage posture end to end: PUT fails closed 429, GET passes through", async () => {
    const put = await fetchAs("PUT", makeEnv(makeD1({ throwOnBatch: true }).db));
    expect(put.status).toBe(429);
    expect(put.headers.get("Retry-After")).toBe("2");
    const body = (await put.json()) as { error: string; message: string };
    expect(body.error).toBe("QUOTA_EXCEEDED");
    expect(body.message).toContain("temporarily unverifiable");

    const get = await fetchAs("GET", makeEnv(makeD1({ throwOnBatch: true }).db));
    expect(get.status, "a read must stay available during a D1 outage").toBe(200);
  });

  it("still 429s a tenant over the storage cap", async () => {
    const { db } = makeD1({ storageBytes: QUOTAS.free.storageBytesMax });
    const resp = await fetchAs("PUT", makeEnv(db));
    expect(resp.status).toBe(429);
    const body = (await resp.json()) as { message: string };
    expect(body.message).toContain("Storage quota exceeded");
  });

  it("429s on the REQUEST cap in preference to the storage verdict, from the same round trip", async () => {
    // Over the free request cap AND over the storage cap: the request-count 429
    // is the one the client saw before batching, and must stay the one it sees.
    const { db, calls } = makeD1({
      requestCount: QUOTAS.free.requestsPerMonthMax + 1,
      storageBytes: QUOTAS.free.storageBytesMax,
    });
    const resp = await fetchAs("PUT", makeEnv(db));

    expect(resp.status).toBe(429);
    const body = (await resp.json()) as { message: string };
    expect(body.message).toContain("Monthly request quota exceeded");
    expect(calls.batches).toHaveLength(1);
  });
});
