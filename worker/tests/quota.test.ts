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
  type TierResult,
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
  /** Simulated `subscription_state` of the tier_selections row (default 'active'). */
  tierSelectionsState?: string;
  tenantTierRow?: { tier: string } | null;
  storageBytes?: number | null;
  patRow?: { tenant_id: string; expires_ms: number } | null;
  throwOnTierQuery?: boolean;
  throwOnStorageQuery?: boolean;
}): D1Database {
  const {
    tierSelectionsRow = null,
    tierSelectionsState = "active",
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
            // Mirror the SQL's `subscription_state = 'active'` filter: a
            // non-active row (e.g. pending_checkout, written at checkout
            // click time before payment) is INVISIBLE to the enforcement
            // read and must not grant its paid tier.
            if (
              sql.includes("subscription_state = 'active'") &&
              tierSelectionsState !== "active"
            ) {
              return null as T | null;
            }
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
  // Numbers follow the signed launch rate card (6-tier ladder, PR #209 —
  // worker/src/lib/quota.ts header / apps/docs/src/lib/pricing.ts TIER_RATE_CARD).
  it("free tier has 10 GB storage ceiling and 500K request cap", () => {
    expect(QUOTAS.free.storageBytesMax).toBe(10 * 1_073_741_824);
    expect(QUOTAS.free.requestsPerMonthMax).toBe(500_000);
  });

  it("solo and starter follow the 6-tier rate card (50 GB / 2M and 150 GB / 6M)", () => {
    expect(QUOTAS.solo.storageBytesMax).toBe(50 * 1_073_741_824);
    expect(QUOTAS.solo.requestsPerMonthMax).toBe(2_000_000);
    expect(QUOTAS.starter.storageBytesMax).toBe(150 * 1_073_741_824);
    expect(QUOTAS.starter.requestsPerMonthMax).toBe(6_000_000);
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

  // SECURITY (billing-state CRITICAL): the checkout backend writes the
  // requested paid tier at click time with subscription_state =
  // 'pending_checkout' BEFORE any payment. The enforcement read must honor
  // tier_selections ONLY when the subscription is active, or a user could
  // select a paid tier, abandon Stripe, and be served full paid quota free.
  it("does NOT grant a pending_checkout (unpaid) paid tier — falls through to free", async () => {
    const db = makeQuotaD1Mock({
      tierSelectionsRow: { tier: "max" },
      tierSelectionsState: "pending_checkout",
      tenantTierRow: null,
    });
    const tier = await getTierForTenant(db, TEST_TENANT_ID);
    expect(tier).toBe("free");
  });

  it("grants the paid tier once the subscription is active", async () => {
    const db = makeQuotaD1Mock({
      tierSelectionsRow: { tier: "max" },
      tierSelectionsState: "active",
    });
    const tier = await getTierForTenant(db, TEST_TENANT_ID);
    expect(tier).toBe("max");
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// checkStorageQuota
// ─────────────────────────────────────────────────────────────────────────────

describe("checkStorageQuota", () => {
  const FREE_MAX = 10 * 1_073_741_824; // 10 GB
  // TierResult helper (F21): a confirmed (non-D1-error) tier resolution.
  const ok = (tier: Tier): TierResult => ({ tier, d1Error: false });
  // A confirmed read (non-mutating) and a confirmed write (mutating).
  const READ = false;
  const WRITE = true;

  it("returns ok:true when bytes_used is below the free-tier ceiling", async () => {
    const db = makeQuotaD1Mock({ storageBytes: FREE_MAX - 1 });
    const result = await checkStorageQuota(db, TEST_TENANT_ID, ok("free"), WRITE);
    expect(result.ok).toBe(true);
  });

  it("returns ok:false with Retry-After when bytes_used >= free-tier ceiling", async () => {
    const db = makeQuotaD1Mock({ storageBytes: FREE_MAX });
    const result = await checkStorageQuota(db, TEST_TENANT_ID, ok("free"), WRITE);
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
    const result = await checkStorageQuota(db, TEST_TENANT_ID, ok("enterprise"), WRITE);
    expect(result.ok).toBe(true);
  });

  it("treats null SUM result (no rows) as 0 bytes — within quota", async () => {
    const db = makeQuotaD1Mock({ storageBytes: null });
    const result = await checkStorageQuota(db, TEST_TENANT_ID, ok("free"), WRITE);
    expect(result.ok).toBe(true);
  });

  // ── CAA-360 #25: verb-aware D1-error posture ──────────────────────────────

  it("fails OPEN on a storage-query D1 error for a READ request", async () => {
    const db = makeQuotaD1Mock({ throwOnStorageQuery: true });
    const result = await checkStorageQuota(db, TEST_TENANT_ID, ok("free"), READ);
    expect(result.ok).toBe(true);
  });

  it("fails CLOSED with a short Retry-After on a storage-query D1 error for a WRITE request", async () => {
    const db = makeQuotaD1Mock({ throwOnStorageQuery: true });
    const result = await checkStorageQuota(db, TEST_TENANT_ID, ok("free"), WRITE);
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.retryAfterSec).toBeGreaterThan(0);
      // Short transient retry — NOT the until-next-month cycle wait.
      expect(result.retryAfterSec).toBeLessThanOrEqual(60);
      expect(result.reason).toContain("unverifiable");
    }
  });

  it("fails OPEN on an unconfirmed (tier-lookup D1 error) tier for a READ request", async () => {
    const db = makeQuotaD1Mock({ storageBytes: FREE_MAX }); // would be over-cap IF checked
    const result = await checkStorageQuota(
      db,
      TEST_TENANT_ID,
      { tier: "free", d1Error: true },
      READ,
    );
    expect(result.ok).toBe(true);
  });

  it("fails CLOSED on an unconfirmed (tier-lookup D1 error) tier for a WRITE request", async () => {
    const db = makeQuotaD1Mock({ storageBytes: 0 }); // would be under-cap IF checked
    const result = await checkStorageQuota(
      db,
      TEST_TENANT_ID,
      { tier: "free", d1Error: true },
      WRITE,
    );
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.reason).toContain("unverifiable");
    }
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
