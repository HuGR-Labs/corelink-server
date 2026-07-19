/**
 * Tests for the tenant fast-suspend gate (go-live GAP G4).
 *
 * Two layers:
 *   1. Unit tests on `isTenantSuspended` — the single-flight + TTL cache and
 *      its fail-open-but-never-for-a-known-suspend posture.
 *   2. Integration tests through the Worker handler — a suspended tenant's
 *      valid PAT is 403'd on the customer CAS/AC hot path; an active tenant's
 *      identical PAT passes the gate.
 */

import { describe, it, expect, beforeEach } from "vitest";
import type { D1Database, DurableObjectNamespace } from "@cloudflare/workers-types";
import {
  isTenantSuspended,
  __resetTenantSuspendCacheForTest,
  SUSPEND_CACHE_TTL_MS,
} from "../src/lib/tenant_suspend_gate.js";
import workerHandler from "../src/index.js";
import type { Env } from "../src/index.js";
import { TEST_PAT_SIGNING_KEY, mintTestPat } from "./setup.js";

const TEST_TENANT_ID = "00000000-0000-0000-0000-000000000042";
const TEST_TOKEN_ID = "BBBBBBBBBBBBBBBB";
const TEST_PAT_TOKEN = await mintTestPat({ tokenId: TEST_TOKEN_ID });

type ExecutionContext = {
  waitUntil: (_p: Promise<unknown>) => void;
  passThroughOnException: () => void;
};

beforeEach(() => {
  // The module-level cache is per-isolate — clear it so a decision from one
  // case does not leak into the next (all cases reuse TEST_TENANT_ID).
  __resetTenantSuspendCacheForTest();
});

// ─────────────────────────────────────────────────────────────────────────────
// Unit: isTenantSuspended (cache / single-flight / failure posture)
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Minimal D1 mock for the offboarding query. `state` is the row's
 * `tenant_offboarding_state.state` (or `null` for "no row" = active). Counts
 * how many times the query actually hit D1 (to prove caching / single-flight).
 */
function makeOffboardingD1(opts: {
  state?: string | null;
  throwOnRead?: boolean;
}): { db: D1Database; reads: () => number } {
  let reads = 0;
  const { state = null, throwOnRead = false } = opts;
  const db = {
    prepare: (_sql: string) => ({
      bind: (..._args: unknown[]) => ({
        first: async <T>() => {
          reads += 1;
          if (throwOnRead) throw new Error("D1 offboarding read error");
          return (state === null ? null : { state }) as T | null;
        },
      }),
    }),
  } as unknown as D1Database;
  return { db, reads: () => reads };
}

describe("isTenantSuspended", () => {
  it("returns false for a tenant with NO offboarding row (active)", async () => {
    const { db } = makeOffboardingD1({ state: null });
    expect(await isTenantSuspended(db, TEST_TENANT_ID, 1_000)).toBe(false);
  });

  it("returns true for state='suspended'", async () => {
    const { db } = makeOffboardingD1({ state: "suspended" });
    expect(await isTenantSuspended(db, TEST_TENANT_ID, 1_000)).toBe(true);
  });

  it("returns true for state='erased'", async () => {
    const { db } = makeOffboardingD1({ state: "erased" });
    expect(await isTenantSuspended(db, TEST_TENANT_ID, 1_000)).toBe(true);
  });

  it("returns FALSE for the earlier arms (grace_period / read_only / cancel_requested keep access)", async () => {
    for (const state of ["cancel_requested", "grace_period", "read_only"]) {
      __resetTenantSuspendCacheForTest();
      const { db } = makeOffboardingD1({ state });
      expect(await isTenantSuspended(db, TEST_TENANT_ID, 1_000)).toBe(false);
    }
  });

  it("serves a fresh cache hit without a second D1 read", async () => {
    const { db, reads } = makeOffboardingD1({ state: "suspended" });
    expect(await isTenantSuspended(db, TEST_TENANT_ID, 1_000)).toBe(true);
    // Within TTL → cached, no new read.
    expect(await isTenantSuspended(db, TEST_TENANT_ID, 1_000 + SUSPEND_CACHE_TTL_MS - 1)).toBe(true);
    expect(reads()).toBe(1);
  });

  it("re-reads D1 after the TTL expires", async () => {
    const { db, reads } = makeOffboardingD1({ state: null });
    expect(await isTenantSuspended(db, TEST_TENANT_ID, 1_000)).toBe(false);
    // Past TTL → stale → re-read.
    expect(await isTenantSuspended(db, TEST_TENANT_ID, 1_000 + SUSPEND_CACHE_TTL_MS + 1)).toBe(false);
    expect(reads()).toBe(2);
  });

  it("single-flights concurrent misses into ONE D1 read", async () => {
    const { db, reads } = makeOffboardingD1({ state: "suspended" });
    const [a, b, c] = await Promise.all([
      isTenantSuspended(db, TEST_TENANT_ID, 1_000),
      isTenantSuspended(db, TEST_TENANT_ID, 1_000),
      isTenantSuspended(db, TEST_TENANT_ID, 1_000),
    ]);
    expect([a, b, c]).toEqual([true, true, true]);
    expect(reads()).toBe(1);
  });

  it("fails OPEN (false) on a D1 read error with NO prior knowledge", async () => {
    const { db } = makeOffboardingD1({ throwOnRead: true });
    // A transient D1 fault must not break an active tenant we know nothing about.
    expect(await isTenantSuspended(db, TEST_TENANT_ID, 1_000)).toBe(false);
  });

  it("still DENIES (true) on a D1 read error when the last known value is suspended", async () => {
    // First: a good read establishes the tenant as suspended.
    const { db: goodDb } = makeOffboardingD1({ state: "suspended" });
    expect(await isTenantSuspended(goodDb, TEST_TENANT_ID, 1_000)).toBe(true);
    // Later (past TTL): the refresh read errors — fail-open must NOT grant access
    // to a tenant we already know is suspended.
    const { db: errDb } = makeOffboardingD1({ throwOnRead: true });
    expect(await isTenantSuspended(errDb, TEST_TENANT_ID, 1_000 + SUSPEND_CACHE_TTL_MS + 1)).toBe(true);
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// Integration: Worker handler 403 on the customer CAS/AC path
// ─────────────────────────────────────────────────────────────────────────────

/**
 * D1 mock for the full Worker path: resolves the PAT row, then routes the
 * offboarding / tier / storage queries so an active tenant reaches the DO and a
 * suspended tenant is 403'd at extractAuth.
 */
function makeWorkerD1(opts: { offboardingState?: string | null }): D1Database {
  const { offboardingState = null } = opts;
  return {
    prepare: (sql: string) => ({
      bind: (...args: unknown[]) => ({
        first: async <T>() => {
          if (sql.includes("tenant_offboarding_state")) {
            return (offboardingState === null ? null : { state: offboardingState }) as T | null;
          }
          if (sql.includes("FROM pat")) {
            const tokenId = args[0] as string;
            if (tokenId === TEST_TOKEN_ID) {
              return {
                tenant_id: TEST_TENANT_ID,
                expires_ms: Date.now() + 3_600_000,
                scope: "cas:rw",
                runner_job_ac_key: null,
              } as T | null;
            }
            return null as T | null;
          }
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
    batch: async () => [],
    exec: async () => ({ count: 0, duration: 0 }),
    withSession() { return this; },
    dump: async () => new ArrayBuffer(0),
  } as unknown as D1Database;
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

describe("tenant suspend gate — Worker CAS/AC path", () => {
  it("403s a valid PAT whose tenant is SUSPENDED", async () => {
    const env = makeEnv(makeWorkerD1({ offboardingState: "suspended" }));
    const resp = await workerFetch(
      "http://localhost/v1/cas/blobs/sha256:abc123/1024",
      { headers: { Authorization: `Bearer ${TEST_PAT_TOKEN}` } },
      env,
    );
    expect(resp.status).toBe(403);
    const body = (await resp.json()) as { error: string };
    expect(body.error).toBe("FORBIDDEN");
  });

  it("403s a valid PAT whose tenant is ERASED", async () => {
    const env = makeEnv(makeWorkerD1({ offboardingState: "erased" }));
    const resp = await workerFetch(
      "http://localhost/v1/cas/blobs/sha256:abc123/1024",
      { headers: { Authorization: `Bearer ${TEST_PAT_TOKEN}` } },
      env,
    );
    expect(resp.status).toBe(403);
  });

  it("does NOT 403 an ACTIVE tenant (no offboarding row) — gate passes to the DO", async () => {
    const env = makeEnv(makeWorkerD1({ offboardingState: null }));
    const resp = await workerFetch(
      "http://localhost/v1/cas/blobs/sha256:abc123/1024",
      { headers: { Authorization: `Bearer ${TEST_PAT_TOKEN}` } },
      env,
    );
    // The DO stub returns 503 — reaching it means the suspend gate allowed the
    // request through (it is NOT 403).
    expect(resp.status).not.toBe(403);
  });
});
