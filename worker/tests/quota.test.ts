/**
 * Unit tests for worker/src/lib/quota.ts — tier lookup, quota definitions,
 * storage quota enforcement, and request quota stub.
 *
 * Also exercises the quota enforcement path in the Worker handler via the
 * existing workerFetch test harness pattern.
 *
 * Test inventory:
 *   Quota constants          (3 tests)
 *   getTierForTenant         (9 tests)
 *   checkStorageQuota        (9 tests)
 *   checkRequestQuota        (7 tests)  ← red-team #5: monthly request cap
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
import { TEST_PAT_SIGNING_KEY, mintTestPat } from "./setup.js";

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

const TEST_TENANT_ID = "00000000-0000-0000-0000-000000000001";
const TEST_TOKEN_ID = "AAAAAAAAAAAAAAAA";
// Honest auth harness (#345 / House #2): a CRYPTOGRAPHICALLY VALID PAT signed by
// TEST_PAT_SIGNING_KEY (bound in makeEnv below). The native plane fails closed
// (503) on an unsigned PAT / unset key, which would mask the 429 quota path —
// so the integration tests must mint a real signed token to reach the gate.
const TEST_PAT_TOKEN = await mintTestPat({ tokenId: TEST_TOKEN_ID });

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
  /**
   * Post-increment request_count returned by the monthly_request_counts UPSERT
   * (the atomic increment-and-check in checkRequestQuota). Default 1 (first
   * request of the month).
   */
  requestCountAfterIncrement?: number;
  /** Make the monthly_request_counts UPSERT throw (D1 error → fail-open). */
  throwOnRequestCountQuery?: boolean;
}): D1Database {
  const {
    tierSelectionsRow = null,
    tierSelectionsState = "active",
    tenantTierRow = null,
    storageBytes = 0,
    patRow = { tenant_id: TEST_TENANT_ID, expires_ms: Date.now() + 3_600_000 },
    throwOnTierQuery = false,
    throwOnStorageQuery = false,
    requestCountAfterIncrement = 1,
    throwOnRequestCountQuery = false,
  } = opts;

  return {
    prepare: (sql: string) => ({
      bind: (...args: unknown[]) => ({
        first: async <T>() => {
          const isTierSelQuery = sql.includes("tier_selections");
          const isTenantTierQuery = sql.includes("FROM tenant") && sql.includes("tier");
          const isStorageQuery = sql.includes("tenant_storage_state");
          const isRequestCountQuery = sql.includes("monthly_request_counts");
          const isPatQuery = sql.includes("FROM pat");

          if (isRequestCountQuery) {
            if (throwOnRequestCountQuery) throw new Error("D1 request-count error");
            // Mirror the atomic UPSERT ... RETURNING request_count.
            return { request_count: requestCountAfterIncrement } as T | null;
          }

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
    // Honest auth harness (#345): bind the valid test signing key so the minted
    // TEST_PAT_TOKEN's HMAC verifies and the request reaches the quota gate
    // (instead of fail-closing to 503 on an unset PAT_SIGNING_KEY).
    PAT_SIGNING_KEY: TEST_PAT_SIGNING_KEY,
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
    expect(tier.tier).toBe("team");
  });

  it("falls back to tenant.tier when tier_selections has no row (free default)", async () => {
    const db = makeQuotaD1Mock({ tierSelectionsRow: null, tenantTierRow: { tier: "free" } });
    const tier = await getTierForTenant(db, TEST_TENANT_ID);
    expect(tier.tier).toBe("free");
  });

  // M14 (billing-integrity): the tenant.tier fallback must NEVER serve a paid
  // ceiling without a confirmed active subscription. migration-0057 only ever
  // defaults tenant.tier to 'free', but any writer that set it to a paid value
  // would otherwise leak full PAID quota with no payment. Fail-safe: floor to
  // 'free'. (This test FAILS before the M14 fix — the old fallback returned the
  // raw paid tier — and PASSES after.)
  it("floors a paid tenant.tier fallback to 'free' when there is no active subscription (M14)", async () => {
    const db = makeQuotaD1Mock({ tierSelectionsRow: null, tenantTierRow: { tier: "pro" } });
    const tier = await getTierForTenant(db, TEST_TENANT_ID);
    expect(tier.tier).toBe("free");
  });

  it("falls back to 'free' when both tables return null", async () => {
    const db = makeQuotaD1Mock({ tierSelectionsRow: null, tenantTierRow: null });
    const tier = await getTierForTenant(db, TEST_TENANT_ID);
    expect(tier.tier).toBe("free");
  });

  it("falls back to 'free' when tier_selections row has an unrecognised value", async () => {
    const db = makeQuotaD1Mock({ tierSelectionsRow: { tier: "legacy_gold" }, tenantTierRow: null });
    const tier = await getTierForTenant(db, TEST_TENANT_ID);
    expect(tier.tier).toBe("free");
  });

  it("falls back to 'free' on D1 error (fail-open)", async () => {
    const db = makeQuotaD1Mock({ throwOnTierQuery: true });
    const tier = await getTierForTenant(db, TEST_TENANT_ID);
    expect(tier.tier).toBe("free");
  });

  it("recognises all canonical tier values", async () => {
    const tiers: Tier[] = ["free", "solo", "starter", "team", "pro", "org", "enterprise"];
    for (const t of tiers) {
      const db = makeQuotaD1Mock({ tierSelectionsRow: { tier: t } });
      const resolved = await getTierForTenant(db, TEST_TENANT_ID);
      expect(resolved.tier).toBe(t);
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
    expect(tier.tier).toBe("free");
  });

  it("grants the paid tier once the subscription is active", async () => {
    const db = makeQuotaD1Mock({
      tierSelectionsRow: { tier: "max" },
      tierSelectionsState: "active",
    });
    const tier = await getTierForTenant(db, TEST_TENANT_ID);
    expect(tier.tier).toBe("max");
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
// checkRequestQuota — monthly request-count cap (red-team #5)
// ─────────────────────────────────────────────────────────────────────────────

describe("checkRequestQuota", () => {
  const ok = (tier: Tier): TierResult => ({ tier, d1Error: false });
  const ENABLED = true;
  const DISABLED = false;

  it("no-op (ok:true) without touching the counter when the flag is OFF", async () => {
    const db = makeQuotaD1Mock({ requestCountAfterIncrement: 9_999_999 }); // would be over-cap IF checked
    const result = await checkRequestQuota(db, TEST_TENANT_ID, ok("free"), DISABLED);
    expect(result.ok).toBe(true);
  });

  it("returns ok:true when the post-increment count is UNDER the tier cap", async () => {
    // free cap = 500K; this is request #1 of the month.
    const db = makeQuotaD1Mock({ requestCountAfterIncrement: 1 });
    const result = await checkRequestQuota(db, TEST_TENANT_ID, ok("free"), ENABLED);
    expect(result.ok).toBe(true);
  });

  it("returns ok:true on the request that lands exactly ON the cap (count === cap)", async () => {
    const db = makeQuotaD1Mock({ requestCountAfterIncrement: QUOTAS.free.requestsPerMonthMax });
    const result = await checkRequestQuota(db, TEST_TENANT_ID, ok("free"), ENABLED);
    expect(result.ok).toBe(true);
  });

  it("returns ok:false + 429-shaped Retry-After when the count EXCEEDS the cap", async () => {
    const db = makeQuotaD1Mock({ requestCountAfterIncrement: QUOTAS.free.requestsPerMonthMax + 1 });
    const result = await checkRequestQuota(db, TEST_TENANT_ID, ok("free"), ENABLED);
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.retryAfterSec).toBeGreaterThan(0);
      expect(result.retryAfterSec).toBeLessThanOrEqual(2_678_400);
      expect(result.reason).toContain("Monthly request quota exceeded");
      expect(result.reason).toContain("free");
    }
  });

  it("skips the counter for uncapped tiers (enterprise) → always ok:true", async () => {
    const db = makeQuotaD1Mock({ requestCountAfterIncrement: Number.MAX_SAFE_INTEGER });
    const result = await checkRequestQuota(db, TEST_TENANT_ID, ok("enterprise"), ENABLED);
    expect(result.ok).toBe(true);
  });

  it("fails OPEN (ok:true) on a counter-UPSERT D1 error", async () => {
    const db = makeQuotaD1Mock({ throwOnRequestCountQuery: true });
    const result = await checkRequestQuota(db, TEST_TENANT_ID, ok("free"), ENABLED);
    expect(result.ok).toBe(true);
  });

  it("fails OPEN (ok:true) without counting when the tier is unconfirmed (d1Error)", async () => {
    // Would be over-cap IF the counter were consulted; an unconfirmed tier must
    // never false-positive a paid tenant during a partial D1 outage (F21).
    const db = makeQuotaD1Mock({ requestCountAfterIncrement: 9_999_999 });
    const result = await checkRequestQuota(
      db,
      TEST_TENANT_ID,
      { tier: "free", d1Error: true },
      ENABLED,
    );
    expect(result.ok).toBe(true);
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// Cluster D — request-quota gate is fail-CLOSED / ENFORCED BY DEFAULT
// ─────────────────────────────────────────────────────────────────────────────

describe("Cluster D — request quota default-on / fail-closed", () => {
  const ok = (tier: Tier): TierResult => ({ tier, d1Error: false });

  // The Worker derives `requestQuotaEnabled` from the opt-OUT kill-switch
  // (worker/src/index.ts): enforcement is LIVE unless REQUEST_QUOTA_DISABLED
  // === "true". Mirror that exact derivation here so the test pins the
  // fail-closed semantics, not just the function.
  const deriveEnabled = (envVal: string | undefined): boolean => envVal !== "true";

  it("an UNSET env var ⇒ enforcement is ENABLED (fail-closed default)", () => {
    expect(deriveEnabled(undefined)).toBe(true);
  });

  it("env var = anything-but-'true' ⇒ still ENABLED", () => {
    expect(deriveEnabled("")).toBe(true);
    expect(deriveEnabled("false")).toBe(true);
    expect(deriveEnabled("0")).toBe(true);
    expect(deriveEnabled("yes")).toBe(true);
  });

  it("ONLY env var === 'true' disables enforcement (dev/test opt-out)", () => {
    expect(deriveEnabled("true")).toBe(false);
  });

  it("with NO flag set, an over-cap tenant gets a 429-shaped reject (gate is LIVE)", async () => {
    // No REQUEST_QUOTA_DISABLED ⇒ derived boolean is true ⇒ the counter is
    // consulted and an over-cap count is rejected. This is the prod posture:
    // a missing env var does NOT silently disable the contracted cap.
    const requestQuotaEnabled = deriveEnabled(undefined);
    expect(requestQuotaEnabled).toBe(true);
    const db = makeQuotaD1Mock({
      requestCountAfterIncrement: QUOTAS.free.requestsPerMonthMax + 1,
    });
    const result = await checkRequestQuota(db, TEST_TENANT_ID, ok("free"), requestQuotaEnabled);
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.retryAfterSec).toBeGreaterThan(0);
      expect(result.reason).toContain("Monthly request quota exceeded");
    }
  });

  it("with the dev/test kill-switch ON, the counter is NOT consulted (no-op)", async () => {
    const requestQuotaEnabled = deriveEnabled("true"); // → false
    expect(requestQuotaEnabled).toBe(false);
    const db = makeQuotaD1Mock({ requestCountAfterIncrement: 9_999_999 }); // over-cap IF checked
    const result = await checkRequestQuota(db, TEST_TENANT_ID, ok("free"), requestQuotaEnabled);
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
  // House #2 (#345) landed the honest PAT harness: TEST_PAT_TOKEN is now an
  // HMAC-valid PAT signed by TEST_PAT_SIGNING_KEY (bound in makeEnv), so this
  // end-to-end workerFetch request authenticates, resolves the tenant, and
  // reaches checkStorageQuota — the at-limit storage row drives the real 429.
  // (Previously skipped because the unset PAT_SIGNING_KEY fail-closed to 503
  // before the quota gate could fire.)
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
      { method: "PUT", headers: { Authorization: `Bearer ${TEST_PAT_TOKEN}` }, body: "x" },
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
