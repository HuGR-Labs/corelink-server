/**
 * Unit tests for worker/src/lib/quota.ts — tier lookup, quota definitions,
 * storage quota enforcement, and request quota stub.
 *
 * Also exercises the quota enforcement path in the Worker handler via the
 * existing workerFetch test harness pattern.
 *
 * Test inventory (18 tests):
 *   Quota constants          (3 tests)
 *   getTierForTenant         (6 tests)
 *   checkStorageQuota        (5 tests)
 *   secondsUntilNextMonth    (1 test)
 *   Worker 429 integration   (3 tests)
 */

import { describe, it, expect } from "vitest";
import type { D1Database, DurableObjectNamespace } from "@cloudflare/workers-types";
import {
  QUOTAS,
  getTierForTenant,
  checkStorageQuota,
  checkRequestQuota,
  secondsUntilNextMonthStart,
  type Tier,
} from "../src/lib/quota.js";
import workerHandler from "../src/index.js";
import type { Env } from "../src/index.js";

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

const TEST_TENANT_ID = "00000000-0000-0000-0000-000000000001";
const TEST_TOKEN_ID = "AAAAAAAAAAAAAAAA";
const TEST_PAT_TOKEN =
  "corelink_pat_" +
  TEST_TOKEN_ID +
  "." +
  "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA" +
  "." +
  "AAAAAAAAAAAAAAAAAAAAAA";

/** Build a minimal D1 mock that routes queries by SQL keyword. */
function makeQuotaD1Mock(opts: {
  tierSelectionsRow?: { tier: string } | null;
  tenantTierRow?: { tier: string } | null;
  storageBytes?: number | null;
  patRow?: { tenant_id: string; expires_ms: number } | null;
  throwOnTierQuery?: boolean;
  throwOnStorageQuery?: boolean;
}): D1Database {
  const {
    tierSelectionsRow = null,
    tenantTierRow = null,
    storageBytes = 0,
    patRow = { tenant_id: TEST_TENANT_ID, expires_ms: Date.now() + 3_600_000 },
    throwOnTierQuery = false,
    throwOnStorageQuery = false,
  } = opts;

  return {
    prepare: (sql: string) => ({
      bind: (...args: unknown[]) => ({
        first: async <T>() => {
          const isTierSelQuery = sql.includes("tier_selections");
          const isTenantTierQuery = sql.includes("FROM tenant") && sql.includes("tier");
          const isStorageQuery = sql.includes("tenant_storage_state");
          const isPatQuery = sql.includes("FROM pat");

          if (isTierSelQuery) {
            if (throwOnTierQuery) throw new Error("D1 tier_selections error");
            return (tierSelectionsRow ?? null) as T | null;
          }
          if (isTenantTierQuery) {
            if (throwOnTierQuery) throw new Error("D1 tenant.tier error");
            return (tenantTierRow ?? null) as T | null;
          }
          if (isStorageQuery) {
            if (throwOnStorageQuery) throw new Error("D1 storage error");
            if (storageBytes === null) return null as T | null;
            return { total_bytes: storageBytes } as T | null;
          }
          if (isPatQuery) {
            const tokenId = args[0] as string;
            if (patRow !== null && tokenId === TEST_TOKEN_ID) {
              return patRow as T | null;
            }
            return null as T | null;
          }
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
    withSession: () => null as never,
    dump: async () => new ArrayBuffer(0),
  } as unknown as D1Database;
}

/** Build a minimal Worker Env for integration tests. */
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
  };
}

async function workerFetch(url: string, init?: RequestInit, env?: Env): Promise<Response> {
  const req = new Request(url, init);
  const resolvedEnv = env ?? makeEnv(makeQuotaD1Mock({}));
  const ctx = { waitUntil: () => {}, passThroughOnException: () => {} } as unknown as ExecutionContext;
  return workerHandler.fetch!(req, resolvedEnv, ctx);
}

// ─────────────────────────────────────────────────────────────────────────────
// QUOTAS constant definitions
// ─────────────────────────────────────────────────────────────────────────────

describe("QUOTAS constant definitions", () => {
  it("free tier has 10 GB storage ceiling and 1M request cap", () => {
    expect(QUOTAS.free.storageBytesMax).toBe(10 * 1_073_741_824);
    expect(QUOTAS.free.requestsPerMonthMax).toBe(1_000_000);
  });

  it("solo and starter map to the same 100 GB ceiling with no request cap", () => {
    expect(QUOTAS.solo.storageBytesMax).toBe(100 * 1_073_741_824);
    expect(QUOTAS.solo.requestsPerMonthMax).toBe(Number.MAX_SAFE_INTEGER);
    expect(QUOTAS.starter.storageBytesMax).toBe(QUOTAS.solo.storageBytesMax);
  });

  it("enterprise has no caps (MAX_SAFE_INTEGER)", () => {
    expect(QUOTAS.enterprise.storageBytesMax).toBe(Number.MAX_SAFE_INTEGER);
    expect(QUOTAS.enterprise.requestsPerMonthMax).toBe(Number.MAX_SAFE_INTEGER);
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// getTierForTenant
// ─────────────────────────────────────────────────────────────────────────────

describe("getTierForTenant", () => {
  it("returns tier from tier_selections when present", async () => {
    const db = makeQuotaD1Mock({ tierSelectionsRow: { tier: "team" } });
    const tier = await getTierForTenant(db, TEST_TENANT_ID);
    expect(tier).toBe("team");
  });

  it("falls back to tenant.tier when tier_selections has no row", async () => {
    const db = makeQuotaD1Mock({ tierSelectionsRow: null, tenantTierRow: { tier: "solo" } });
    const tier = await getTierForTenant(db, TEST_TENANT_ID);
    expect(tier).toBe("solo");
  });

  it("falls back to 'free' when both tables return null", async () => {
    const db = makeQuotaD1Mock({ tierSelectionsRow: null, tenantTierRow: null });
    const tier = await getTierForTenant(db, TEST_TENANT_ID);
    expect(tier).toBe("free");
  });

  it("falls back to 'free' when tier_selections row has an unrecognised value", async () => {
    const db = makeQuotaD1Mock({ tierSelectionsRow: { tier: "legacy_gold" }, tenantTierRow: null });
    const tier = await getTierForTenant(db, TEST_TENANT_ID);
    expect(tier).toBe("free");
  });

  it("falls back to 'free' on D1 error (fail-open)", async () => {
    const db = makeQuotaD1Mock({ throwOnTierQuery: true });
    const tier = await getTierForTenant(db, TEST_TENANT_ID);
    expect(tier).toBe("free");
  });

  it("recognises all canonical tier values", async () => {
    const tiers: Tier[] = ["free", "solo", "starter", "team", "pro", "org", "enterprise"];
    for (const t of tiers) {
      const db = makeQuotaD1Mock({ tierSelectionsRow: { tier: t } });
      const resolved = await getTierForTenant(db, TEST_TENANT_ID);
      expect(resolved).toBe(t);
    }
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// checkStorageQuota
// ─────────────────────────────────────────────────────────────────────────────

describe("checkStorageQuota", () => {
  const FREE_MAX = 10 * 1_073_741_824; // 10 GB

  it("returns ok:true when bytes_used is below the free-tier ceiling", async () => {
    const db = makeQuotaD1Mock({ storageBytes: FREE_MAX - 1 });
    const result = await checkStorageQuota(db, TEST_TENANT_ID, "free");
    expect(result.ok).toBe(true);
  });

  it("returns ok:false with Retry-After when bytes_used >= free-tier ceiling", async () => {
    const db = makeQuotaD1Mock({ storageBytes: FREE_MAX });
    const result = await checkStorageQuota(db, TEST_TENANT_ID, "free");
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.retryAfterSec).toBeGreaterThan(0);
      expect(result.retryAfterSec).toBeLessThanOrEqual(2_678_400);
      expect(result.reason).toContain("Storage quota exceeded");
      expect(result.reason).toContain("free");
    }
  });

  it("returns ok:true for enterprise tier regardless of bytes_used (no cap)", async () => {
    const db = makeQuotaD1Mock({ storageBytes: Number.MAX_SAFE_INTEGER - 1 });
    const result = await checkStorageQuota(db, TEST_TENANT_ID, "enterprise");
    expect(result.ok).toBe(true);
  });

  it("returns ok:true on D1 error (fail-open)", async () => {
    const db = makeQuotaD1Mock({ throwOnStorageQuery: true });
    const result = await checkStorageQuota(db, TEST_TENANT_ID, "free");
    expect(result.ok).toBe(true);
  });

  it("treats null SUM result (no rows) as 0 bytes — within quota", async () => {
    const db = makeQuotaD1Mock({ storageBytes: null });
    const result = await checkStorageQuota(db, TEST_TENANT_ID, "free");
    expect(result.ok).toBe(true);
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// secondsUntilNextMonthStart
// ─────────────────────────────────────────────────────────────────────────────

describe("secondsUntilNextMonthStart", () => {
  it("returns a positive value <= 31 days in seconds", () => {
    const secs = secondsUntilNextMonthStart();
    expect(secs).toBeGreaterThan(0);
    expect(secs).toBeLessThanOrEqual(2_678_400);
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// Worker handler — 429 integration
// ─────────────────────────────────────────────────────────────────────────────

describe("Worker quota enforcement — HTTP 429", () => {
  it("returns 429 with Retry-After when free-tier storage is exhausted", async () => {
    const FREE_MAX = 10 * 1_073_741_824;
    const d1 = makeQuotaD1Mock({
      storageBytes: FREE_MAX, // at-limit
      tierSelectionsRow: null,
      tenantTierRow: null, // defaults to 'free'
    });
    const env = makeEnv(d1);
    // Use /v1/cas/blobs/... — tenant is resolved from PAT (not URL path),
    // so there is no tenant-mismatch 403 before the quota check fires.
    const resp = await workerFetch(
      "http://localhost/v1/cas/blobs/sha256:abc123/1024",
      { headers: { Authorization: `Bearer ${TEST_PAT_TOKEN}` } },
      env,
    );
    expect(resp.status).toBe(429);
    const retryAfter = resp.headers.get("retry-after");
    expect(retryAfter).not.toBeNull();
    expect(Number(retryAfter)).toBeGreaterThan(0);
    const body = await resp.json() as { error: string; message: string };
    expect(body.error).toBe("QUOTA_EXCEEDED");
    expect(body.message).toContain("Storage quota exceeded");
  });

  it("does NOT return 429 when free-tier storage is under limit", async () => {
    const FREE_MAX = 10 * 1_073_741_824;
    const d1 = makeQuotaD1Mock({
      storageBytes: FREE_MAX - 1_000, // under limit
      tierSelectionsRow: null,
      tenantTierRow: null, // defaults to 'free'
    });
    const env = makeEnv(d1);
    // Same PAT-resolved route — quota passes, DO stub returns 503.
    const resp = await workerFetch(
      "http://localhost/v1/cas/blobs/sha256:abc123/1024",
      { headers: { Authorization: `Bearer ${TEST_PAT_TOKEN}` } },
      env,
    );
    // The DO stub returns 503 — that's fine, it means quota passed
    expect(resp.status).not.toBe(429);
  });

  it("does NOT enforce quota for _anonymous (signup) routes", async () => {
    const FREE_MAX = 10 * 1_073_741_824;
    const d1 = makeQuotaD1Mock({ storageBytes: FREE_MAX * 100 }); // way over limit
    const env = makeEnv(d1);
    // Signup route bypasses PAT auth and quota
    const resp = await workerFetch("http://localhost/v1/signup/pilot/sometoken", {}, env);
    // Should NOT be 429 (signup routes get _anonymous tenant, quota skipped)
    expect(resp.status).not.toBe(429);
  });
});

// ExecutionContext type alias for convenience (mirrors existing test pattern)
type ExecutionContext = {
  waitUntil: (_p: Promise<unknown>) => void;
  passThroughOnException: () => void;
};
