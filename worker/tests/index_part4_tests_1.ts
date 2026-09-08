/**
 * Unit tests for worker/src/index.ts — route resolution, auth middleware,
 * error mapping, CORS, and timing-pad paths.
 *
 * Tests import and exercise the worker's exported handler directly.
 * This avoids the cloudflare:test / SELF.fetch dependency (which requires
 * a working cloudflare-pool miniflare setup) and instead tests the pure
 * fetch handler logic with real Node.js Web APIs.
 *
 * Node.js v22+ provides native fetch, Request, Response, Headers, crypto,
 * URL — no polyfills needed.
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import type { D1Database } from "@cloudflare/workers-types";
import workerHandler from "../src/index.js";
import type { Env } from "../src/index.js";
import {
  TEST_PAT_SIGNING_KEY,
  TEST_PAT_TOKEN_ID,
  mintTestPat,
} from "./setup.js";
import { __resetTenantResidencyCacheForTest } from "../src/lib/tenant_residency_cache.js";
import { __resetTierCacheForTests } from "../src/lib/tenant_tier_cache.js";
import { batchViaFirst } from "./d1_batch_mock.js";

// The residency + tier resolvers each keep a per-isolate L1 cache keyed by
// tenant_id. Many cases here reuse TEST_TENANT_ID with DIFFERENT mock-D1 verdicts
// (a resolved region, a missing binding, a D1 throw; a tier), so a cached
// decision from one case must not leak into the next — reset both before every
// case (mirrors the uncached direct-D1 reads these tests were written against).
beforeEach(() => {
  __resetTenantResidencyCacheForTest();
  __resetTierCacheForTests();
});

// ──────────────────────────────────────────────────────────────────────────────
// Test helpers
// ──────────────────────────────────────────────────────────────────────────────

/** Minimal mock ExecutionContext */
function makeCtx(): ExecutionContext {
  return {
    waitUntil: (_p: Promise<unknown>) => {},
    passThroughOnException: () => {},
  } as unknown as ExecutionContext;
}

// ──────────────────────────────────────────────────────────────────────────────
// Canonical test PAT — CRYPTOGRAPHICALLY VALID under TEST_PAT_SIGNING_KEY.
//
// The native plane's sole possession gate is a 128-bit truncated HMAC-SHA256
// over `<token_id>.<random_secret>` keyed by PAT_SIGNING_KEY (src/index.ts).
// Previously this token carried an all-`A` placeholder sig and the test env left
// PAT_SIGNING_KEY UNSET, so extractAuth failed CLOSED (503) before the HMAC/D1
// logic — "passing" PAT tests were passing on the misconfig, not the auth path.
// We now mint a REAL signed token (and makeEnv() binds the matching key below)
// so PAT-gated tests reach the real auth/route logic. The no-key ⇒ 503
// fail-closed path is covered by an EXPLICIT negative test (see below).
//
// Format: corelink_pat_<16-char-Crockford-b32>.<43-char-base64url>.<22-char-base64url>
// Total:  9 + 3 + 1 + 16 + 1 + 43 + 1 + 22 = 96 chars
// ──────────────────────────────────────────────────────────────────────────────
const TEST_TOKEN_ID = TEST_PAT_TOKEN_ID; // 16 Crockford b32 chars
const TEST_PAT_TOKEN = await mintTestPat();
// Sanity: 96 chars total
if (TEST_PAT_TOKEN.length !== 96) {
  throw new Error(`TEST_PAT_TOKEN length ${TEST_PAT_TOKEN.length} !== 96`);
}

const TEST_TENANT_ID = "00000000-0000-0000-0000-000000000001";

/**
 * Build a mock D1 database that returns the given row (or null) for the
 * token_id query. Used to simulate D1 PAT store responses in unit tests.
 */
function makeD1Mock(
  // `scope` is optional so existing fixtures (which omit it) double as the
  // "older row with NULL/absent scope" case (H1). When present it is forwarded
  // verbatim to the DO via x-corelink-scope. `revoked_at_ms` (migration 0063)
  // is optional likewise: absent/NULL = active, non-NULL = soft-revoked.
  tokenIdToRow: Map<
    string,
    {
      tenant_id: string;
      expires_ms: number;
      scope?: string | null;
      revoked_at_ms?: number | null;
    }
  >,
): D1Database {
  return {
    prepare: (sql: string) => ({
      bind: (...args: unknown[]) => ({
        first: async <T>() => {
          const tokenId = args[0] as string;
          const row = tokenIdToRow.get(tokenId);
          // Simulate D1 semantics for the soft-revocation predicate
          // (migration 0063): when the worker's SQL filters on
          // `revoked_at_ms IS NULL`, a revoked row must NOT be returned.
          if (
            sql.includes("revoked_at_ms IS NULL") &&
            row?.revoked_at_ms != null
          ) {
            return null as T | null;
          }
          return (row ?? null) as T | null;
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
}

/** Mock Env with a CORELINK_SERVER DO namespace that returns an error stub */
function makeEnv(doStubStatus = 503, d1Override?: D1Database): Env {
  const stub = {
    fetch: async (req: Request): Promise<Response> => {
      // Echo x-request-id back like the real CoreLinkServer DO does on its 503
      // (durable_object.ts) — the OCI pass-through forwards the DO response
      // verbatim (it does NOT re-stamp x-request-id the way the PAT path does),
      // so the DO is the one that must carry the correlation id on OCI routes.
      return new Response(
        JSON.stringify({ error: "CONTAINER_UNAVAILABLE", message: "test stub", request_id: "stub" }),
        {
          status: doStubStatus,
          headers: {
            "Content-Type": "application/json",
            "X-Request-Id": req.headers.get("x-request-id") ?? "stub",
          },
        },
      );
    },
  };

  const namespace = {
    idFromName: (_name: string) => ({ toString: () => "stub-id" }),
    get: (_id: unknown) => stub,
    idFromString: (_s: string) => ({ toString: () => "stub-id" }),
    newUniqueId: () => ({ toString: () => "stub-unique-id" }),
    jurisdiction: (_j: string) => namespace,
  } as unknown as DurableObjectNamespace;

  // Default D1 mock: recognises TEST_TOKEN_ID as a valid, non-expired PAT.
  const d1 = d1Override ?? makeD1Mock(new Map([
    [TEST_TOKEN_ID, { tenant_id: TEST_TENANT_ID, expires_ms: Date.now() + 3_600_000 }],
  ]));

  return {
    CORELINK_SERVER: namespace,
    ENVIRONMENT: "test",
    CONFIG_DB: d1,
    // F18: the native-plane possession gate (extractAuth) fails CLOSED (503) when
    // PAT_SIGNING_KEY is absent/short. Bind the fixed valid test key so PAT-gated
    // tests reach the real HMAC + D1 auth logic instead of short-circuiting to 503.
    // The fail-closed path is covered explicitly by the negative test below.
    PAT_SIGNING_KEY: TEST_PAT_SIGNING_KEY,
  };
}

/** Env whose _system DO returns a realistic native container health body. */
function makeContainerHealthEnv(
  body = { status: "ok", storage: "r2", topology: "regional" },
  status = 200,
): { env: Env; requests: Request[] } {
  const requests: Request[] = [];
  const stub = {
    fetch: async (request: Request): Promise<Response> => {
      requests.push(request);
      return new Response(JSON.stringify(body), {
        status,
        headers: { "Content-Type": "application/json" },
      });
    },
  };
  const namespace = {
    idFromName: (_name: string) => ({ toString: () => "health-stub-id" }),
    get: (_id: unknown) => stub,
    idFromString: (_s: string) => ({ toString: () => "health-stub-id" }),
    newUniqueId: () => ({ toString: () => "health-stub-id" }),
    jurisdiction: (_j: string) => namespace,
  } as unknown as DurableObjectNamespace;
  return { env: { ...makeEnv(), CORELINK_SERVER: namespace }, requests };
}

/** Invoke the worker fetch handler */
async function workerFetch(
  url: string,
  init?: RequestInit,
  envOverride?: Partial<Env>,
): Promise<Response> {
  const req = new Request(url, init);
  const env = { ...makeEnv(), ...envOverride };
  const ctx = makeCtx();
  return workerHandler.fetch!(req, env, ctx);
}

// ──────────────────────────────────────────────────────────────────────────────
// Health endpoint
// ──────────────────────────────────────────────────────────────────────────────

describe("WP-T1: tenant-derived DO routing", () => {
  it("forwards x-corelink-tenant-id header to DO (DoD 4)", async () => {
    // The DO receives the PAT-resolved tenant via x-corelink-tenant-id so it
    // can bind tenantId in lifecycle state (resolves the null tenantId).
    let capturedTenantHeader: string | null = null;
    const capturingEnv: Partial<Env> = {
      CORELINK_SERVER: {
        idFromName: (_name: string) => ({ toString: () => "stub-id" }),
        get: (_id: unknown) => ({
          fetch: async (req: Request): Promise<Response> => {
            capturedTenantHeader = req.headers.get("x-corelink-tenant-id");
            return new Response(JSON.stringify({ ok: true }), {
              status: 200,
              headers: { "Content-Type": "application/json" },
            });
          },
        }),
        idFromString: (_s: string) => ({ toString: () => "stub-id" }),
        newUniqueId: () => ({ toString: () => "stub-unique-id" }),
        jurisdiction: (_j: string) => capturingEnv.CORELINK_SERVER,
      } as unknown as DurableObjectNamespace,
    };

    // TEST_PAT_TOKEN resolves to TEST_TENANT_ID via the default D1 mock (WP-A1).
    // URL tenant segment matches TEST_TENANT_ID so the spoof gate lets it through.
    await workerFetch(`http://localhost/api/v2/${TEST_TENANT_ID}/path`, {
      headers: { Authorization: `Bearer ${TEST_PAT_TOKEN}` },
    }, capturingEnv);

    // Worker forwards the PAT-resolved tenant via x-corelink-tenant-id (DoD 4).
    expect(capturedTenantHeader).not.toBeNull();
    expect(capturedTenantHeader).toBe(TEST_TENANT_ID);
    expect(capturedTenantHeader).not.toBe("_pending");
  });

  it("idFromName receives auth.tenantId (not URL segment) — DoD routing logic", async () => {
    // The routing must call idFromName(auth.tenantId), NOT idFromName(url-segment).
    // Request 1: URL tenant matches TEST_TENANT_ID → reaches DO; idFromName must
    //   be called with TEST_TENANT_ID (the auth tenant), never the URL string.
    // Request 2: an "_anonymous" route (/v8/artifacts, Turborepo) carries no URL
    //   tenant → the spoof gate is bypassed and routing still uses the auth
    //   tenant. This proves the routing parameter is auth-derived regardless of
    //   the URL path shape. (NB: bare /v2/ is NO LONGER usable here — under PR
    //   #169 it is the OCI pass-through and routes to the dedicated "_oci" DO,
    //   not the auth tenant; /v8/artifacts is the PAT-gated _anonymous route.)
    const namesUsed: string[] = [];
    const capturingEnv: Partial<Env> = {
      CORELINK_SERVER: {
        idFromName: (name: string) => {
          namesUsed.push(name);
          return { toString: () => `do-${name}` };
        },
        get: (_id: unknown) => ({
          fetch: async (_req: Request): Promise<Response> =>
            new Response(JSON.stringify({ ok: true }), {
              status: 200,
              headers: { "Content-Type": "application/json" },
            }),
        }),
        idFromString: (_s: string) => ({ toString: () => "stub-id" }),
        newUniqueId: () => ({ toString: () => "stub-unique-id" }),
        jurisdiction: (_j: string) => capturingEnv.CORELINK_SERVER,
      } as unknown as DurableObjectNamespace,
    };

    // Request 1: matching URL tenant. Request 2: /v8/artifacts (_anonymous route).
    await workerFetch(`http://localhost/api/v2/${TEST_TENANT_ID}/blobs`, {
      headers: { Authorization: `Bearer ${TEST_PAT_TOKEN}` },
    }, capturingEnv);
    await workerFetch("http://localhost/v8/artifacts/hash123", {
      headers: { Authorization: `Bearer ${TEST_PAT_TOKEN}` },
    }, capturingEnv);

    // idFromName must have been called twice, each with the auth tenant
    // (TEST_TENANT_ID) — NOT a URL string. This proves T1 routes by auth, not URL.
    expect(namesUsed).toHaveLength(2);
    expect(namesUsed[0]).toBe(TEST_TENANT_ID);
    expect(namesUsed[1]).toBe(TEST_TENANT_ID);
    // The old "_pending_auth" shared-DO key must NEVER appear (DoD 3).
    expect(namesUsed).not.toContain("_pending_auth");
    expect(namesUsed).not.toContain("_pending");
    // The "_anonymous" URL marker must NOT be used directly for DO routing.
    expect(namesUsed).not.toContain("_anonymous");
  });

  it("no _pending_auth shared DO is used for authenticated requests (DoD 3)", async () => {
    // Regression: before WP-T1, auth failures fell back to
    // idFromName("_pending_auth") — a single shared DO for ALL tenants.
    // After WP-T1 this key must never appear in any idFromName call.
    const namesUsed: string[] = [];
    const capturingEnv: Partial<Env> = {
      CORELINK_SERVER: {
        idFromName: (name: string) => {
          namesUsed.push(name);
          return { toString: () => `do-${name}` };
        },
        get: (_id: unknown) => ({
          fetch: async (_req: Request): Promise<Response> =>
            new Response(JSON.stringify({ ok: true }), {
              status: 200,
              headers: { "Content-Type": "application/json" },
            }),
        }),
        idFromString: (_s: string) => ({ toString: () => "stub-id" }),
        newUniqueId: () => ({ toString: () => "stub-unique-id" }),
        jurisdiction: (_j: string) => capturingEnv.CORELINK_SERVER,
      } as unknown as DurableObjectNamespace,
    };

    // npm route: first segment is the tenant namespace; match TEST_TENANT_ID.
    await workerFetch(`http://localhost/npm/${TEST_TENANT_ID}/my-package`, {
      headers: { Authorization: `Bearer ${TEST_PAT_TOKEN}` },
    }, capturingEnv);

    // "_pending_auth" must never be passed to idFromName.
    expect(namesUsed).not.toContain("_pending_auth");
    // Routing key must be the auth-resolved tenant (TEST_TENANT_ID), not a stub.
    expect(namesUsed).toContain(TEST_TENANT_ID);
    expect(namesUsed).not.toContain("_pending");
  });

  it("path-spoof: real-resolved tenant ≠ URL tenant → 403 FORBIDDEN (REAPI)", async () => {
    // The spoof gate is ACTIVE now that WP-A1 resolves a real tenant.
    // TEST_PAT_TOKEN resolves to TEST_TENANT_ID; the URL claims "attacker-tenant".
    // PAT tenant ≠ URL tenant → the request must be rejected with 403 and the DO
    // must NOT be contacted (P1-2 path-spoof defence).
    let doContacted = false;
    const spyEnv: Partial<Env> = {
      CORELINK_SERVER: {
        idFromName: (_name: string) => ({ toString: () => "stub-id" }),
        get: (_id: unknown) => ({
          fetch: async (_req: Request): Promise<Response> => {
            doContacted = true;
            return new Response("{}", { status: 200 });
          },
        }),
        idFromString: (_s: string) => ({ toString: () => "stub-id" }),
        newUniqueId: () => ({ toString: () => "stub-unique-id" }),
        jurisdiction: (_j: string) => spyEnv.CORELINK_SERVER,
      } as unknown as DurableObjectNamespace,
    };

    const resp = await workerFetch("http://localhost/api/v2/attacker-tenant/path", {
      headers: { Authorization: `Bearer ${TEST_PAT_TOKEN}` },
    }, spyEnv);

    // PAT tenant (TEST_TENANT_ID) ≠ URL tenant ("attacker-tenant") → 403, DO untouched.
    expect(doContacted).toBe(false);
    expect(resp.status).toBe(403);
    const body = await resp.json() as { error?: string };
    expect(body.error).toBe("FORBIDDEN");
  });

  it("path-spoof guard: URL tenant matches auth.tenantId → request reaches DO", async () => {
    // Happy path: when the URL tenant matches auth.tenantId exactly, the request
    // must pass through to the DO without spoof rejection.
    let doContacted = false;
    const spyEnv: Partial<Env> = {
      CORELINK_SERVER: {
        idFromName: (_name: string) => ({ toString: () => "stub-id" }),
        get: (_id: unknown) => ({
          fetch: async (_req: Request): Promise<Response> => {
            doContacted = true;
            return new Response("{}", { status: 200 });
          },
        }),
        idFromString: (_s: string) => ({ toString: () => "stub-id" }),
        newUniqueId: () => ({ toString: () => "stub-unique-id" }),
        jurisdiction: (_j: string) => spyEnv.CORELINK_SERVER,
      } as unknown as DurableObjectNamespace,
    };

    // URL tenant segment == TEST_TENANT_ID == the PAT-resolved tenant → match.
    const resp = await workerFetch(`http://localhost/api/v2/${TEST_TENANT_ID}/blobs`, {
      headers: { Authorization: `Bearer ${TEST_PAT_TOKEN}` },
    }, spyEnv);

    expect(doContacted).toBe(true);
    expect(resp.status).toBe(200);
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// Security (H4): server-trust header hygiene on forwarded requests
//
// A client must NEVER be able to SMUGGLE server-trust headers (x-admin-scope,
// x-admin-principal, x-admin-tenant, x-corelink-internal-auth,
// x-corelink-fanout-from) straight to the DO/container. Every forward path that
// clones request.headers MUST strip them before the Worker sets its own values.
// The Worker-established headers (x-corelink-tenant-id / -route-kind /
// -token-prefix) MUST still survive (the Worker overwrites them).
// ──────────────────────────────────────────────────────────────────────────────
