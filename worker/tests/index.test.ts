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

describe("GET /health", () => {
  it("returns 200 with status:ok", async () => {
    const resp = await workerFetch("http://localhost/health");
    expect(resp.status).toBe(200);
    const body = await resp.json() as { status: string };
    expect(body.status).toBe("ok");
  });

  it("sets X-Request-Id header", async () => {
    const resp = await workerFetch("http://localhost/health");
    expect(resp.headers.get("x-request-id")).not.toBeNull();
  });

  it("propagates incoming X-Request-Id if valid", async () => {
    const resp = await workerFetch("http://localhost/health", {
      headers: { "x-request-id": "test-id-12345" },
    });
    expect(resp.headers.get("x-request-id")).toBe("test-id-12345");
  });

  it("rejects oversized x-request-id (>128 chars) and generates fresh UUID", async () => {
    const longId = "x".repeat(200);
    const resp = await workerFetch("http://localhost/health", {
      headers: { "x-request-id": longId },
    });
    expect(resp.headers.get("x-request-id")).not.toBe(longId);
    expect(resp.status).toBe(200);
  });

  it("omits env field from JSON body (F19: no env disclosure on unauth endpoints)", async () => {
    // F19 (src/index.ts ~L1428): the deployment environment must NOT be disclosed
    // on unauthenticated endpoints. /health serves only {"status":"ok"} — no `env`.
    const resp = await workerFetch("http://localhost/health");
    const body = await resp.json() as { status: string; env?: string };
    expect(body.status).toBe("ok");
    expect(body.env).toBeUndefined();
  });

  it("does not require Authorization header", async () => {
    const resp = await workerFetch("http://localhost/health");
    expect(resp.status).toBe(200);
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// Container deep-health variants (B-082)
// ──────────────────────────────────────────────────────────────────────────────

describe("container deep-health storage disclosure (B-082)", () => {
  const ADMIN_HEALTH_KEY = "admin-health-key-".repeat(5);

  it("anonymous route strips storage/topology and does not forward query strings", async () => {
    const { env, requests } = makeContainerHealthEnv();
    const response = await workerFetch(
      "http://localhost/_health/container?credential=must-not-authenticate",
      undefined,
      env,
    );
    expect(response.status).toBe(200);
    const body = await response.json() as Record<string, unknown>;
    expect(body).toEqual({ status: "ok" });
    expect(requests).toHaveLength(1);
    expect(new URL(requests[0]!.url).pathname).toBe("/_health");
    expect(new URL(requests[0]!.url).search).toBe("");
  });

  it("authenticated variant preserves storage only with the dedicated admin key", async () => {
    const { env, requests } = makeContainerHealthEnv();
    env.CORELINK_ADMIN_AUTH_KEY = ADMIN_HEALTH_KEY;
    const response = await workerFetch(
      "http://localhost/_health/container/authenticated?credential=ignored",
      { headers: { "X-Corelink-Internal-Auth": ADMIN_HEALTH_KEY } },
      env,
    );
    expect(response.status).toBe(200);
    const body = await response.json() as Record<string, unknown>;
    expect(body).toEqual({ status: "ok", storage: "r2", topology: "regional" });
    expect(requests).toHaveLength(1);
    expect(requests[0]!.headers.get("x-corelink-internal-auth")).toBeNull();
    expect(requests[0]!.headers.get("x-corelink-route-kind")).toBe("health_container_authed");
  });

  it("rejects missing and forged credentials before reaching the DO", async () => {
    const { env, requests } = makeContainerHealthEnv();
    env.CORELINK_ADMIN_AUTH_KEY = ADMIN_HEALTH_KEY;
    const missing = await workerFetch(
      "http://localhost/_health/container/authenticated",
      undefined,
      env,
    );
    const forged = await workerFetch(
      "http://localhost/_health/container/authenticated",
      { headers: { "X-Corelink-Internal-Auth": `${ADMIN_HEALTH_KEY}forged` } },
      env,
    );
    expect(missing.status).toBe(401);
    expect(forged.status).toBe(401);
    expect(requests).toHaveLength(0);
  });

  it("fails closed when the dedicated key is missing, malformed, or only the shared key exists", async () => {
    const noKey = makeContainerHealthEnv();
    expect(
      (await workerFetch(
        "http://localhost/_health/container/authenticated",
        { headers: { "X-Corelink-Internal-Auth": ADMIN_HEALTH_KEY } },
        noKey.env,
      )).status,
    ).toBe(503);

    const shortKey = makeContainerHealthEnv();
    shortKey.env.CORELINK_ADMIN_AUTH_KEY = "too-short";
    expect(
      (await workerFetch(
        "http://localhost/_health/container/authenticated",
        { headers: { "X-Corelink-Internal-Auth": "too-short" } },
        shortKey.env,
      )).status,
    ).toBe(503);

    const sharedOnly = makeContainerHealthEnv();
    sharedOnly.env.CORELINK_INTERNAL_AUTH_KEY = ADMIN_HEALTH_KEY;
    expect(
      (await workerFetch(
        "http://localhost/_health/container/authenticated",
        { headers: { "X-Corelink-Internal-Auth": ADMIN_HEALTH_KEY } },
        sharedOnly.env,
      )).status,
    ).toBe(503);
    expect(sharedOnly.requests).toHaveLength(0);
  });

  it("does not accept a query-string credential and rejects unsupported methods", async () => {
    const { env, requests } = makeContainerHealthEnv();
    env.CORELINK_ADMIN_AUTH_KEY = ADMIN_HEALTH_KEY;
    const queryCredential = await workerFetch(
      `http://localhost/_health/container/authenticated?auth=${encodeURIComponent(ADMIN_HEALTH_KEY)}`,
      undefined,
      env,
    );
    const post = await workerFetch(
      "http://localhost/_health/container/authenticated",
      {
        method: "POST",
        headers: { "X-Corelink-Internal-Auth": ADMIN_HEALTH_KEY },
      },
      env,
    );
    expect(queryCredential.status).toBe(401);
    expect(post.status).toBe(405);
    expect(requests).toHaveLength(0);
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// Not-found — timing-padded
// ──────────────────────────────────────────────────────────────────────────────

describe("GET unknown path (not_found)", () => {
  it("returns 404", async () => {
    const resp = await workerFetch("http://localhost/does-not-exist");
    expect(resp.status).toBe(404);
  });

  it("returns REAPI JSON error envelope", async () => {
    const resp = await workerFetch("http://localhost/no-such-route");
    const body = await resp.json() as { error: string; message: string; request_id: string };
    expect(body.error).toBe("NOT_FOUND");
    expect(typeof body.message).toBe("string");
    expect(typeof body.request_id).toBe("string");
  });

  it("sets X-Request-Id on 404", async () => {
    const resp = await workerFetch("http://localhost/nope");
    expect(resp.headers.get("x-request-id")).not.toBeNull();
  });

  it("returns Content-Type application/json", async () => {
    const resp = await workerFetch("http://localhost/missing");
    expect(resp.headers.get("content-type")).toContain("application/json");
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// Auth middleware
// ──────────────────────────────────────────────────────────────────────────────

describe("auth middleware", () => {
  // ── OCI two-leg auth pass-through (PR #169) ────────────────────────────────
  // The Worker NO LONGER auth-gates or builds OCI error envelopes for /v2/* and
  // /token. It forwards them RAW to the dedicated "_oci" DO; the container does
  // its own two-leg auth and emits ALL OCI responses (401 + Www-Authenticate,
  // error envelopes). With no container bound, the DO stub here returns 503
  // CONTAINER_UNAVAILABLE — so the Worker must pass THAT through verbatim, NOT
  // synthesize a 401/Docker-Distribution OCI envelope of its own.
  it("does NOT 401 /v2/ at the Worker — forwards to the _oci DO (no Worker auth gate)", async () => {
    const resp = await workerFetch("http://localhost/v2/");
    // The default DO stub returns 503; the Worker forwards it verbatim. The key
    // assertion is that the Worker no longer short-circuits OCI with its own 401.
    expect(resp.status).toBe(503);
  });

  it("does NOT synthesize an OCI errors[] envelope for /v2/ — returns the DO body verbatim", async () => {
    const resp = await workerFetch("http://localhost/v2/");
    const body = await resp.json() as Record<string, unknown>;
    // The forwarded DO stub body is the REAPI-shaped CONTAINER_UNAVAILABLE, NOT
    // a Worker-built OCI `errors:[{code:"UNAUTHORIZED"}]` array.
    expect(body["errors"]).toBeUndefined();
    expect(body["error"]).toBe("CONTAINER_UNAVAILABLE");
  });

  it("does NOT set Docker-Distribution-Api-Version on /v2/ from the Worker", async () => {
    const resp = await workerFetch("http://localhost/v2/");
    // The container owns this header now; the Worker (forwarding the DO stub)
    // must not synthesize it.
    expect(resp.headers.get("docker-distribution-api-version")).toBeNull();
  });

  it("routes /token (OCI second leg) to the pass-through, NOT the PAT gate", async () => {
    // /token with OCI Basic auth must reach the _oci DO (→ 503 stub), never the
    // PAT auth gate (which would 401 on a non-Bearer-PAT credential).
    const resp = await workerFetch("http://localhost/token", {
      headers: { Authorization: "Basic dXNlcjpwYXNz" },
    });
    expect(resp.status).toBe(503);
    const body = await resp.json() as Record<string, unknown>;
    expect(body["error"]).toBe("CONTAINER_UNAVAILABLE");
  });

  it("forwards the raw Authorization header to the _oci DO and strips client-trust headers", async () => {
    let captured: { auth: string | null; scope: string | null; tenant: string | null; routeKind: string | null } | null = null;
    const capturingEnv: Partial<Env> = {
      CORELINK_SERVER: {
        idFromName: (name: string) => ({ toString: () => `do-${name}` }),
        get: () => ({
          fetch: async (req: Request): Promise<Response> => {
            captured = {
              auth: req.headers.get("authorization"),
              scope: req.headers.get("x-corelink-scope"),
              tenant: req.headers.get("x-corelink-tenant-id"),
              routeKind: req.headers.get("x-corelink-route-kind"),
            };
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
    await workerFetch("http://localhost/v2/myrepo/blobs/sha256:abc", {
      headers: {
        // OCI Bearer (issued by the container's /token leg) — must reach the DO.
        Authorization: "Bearer oci-hmac-bearer-token",
        // Forged client-trust headers — the Worker MUST strip these.
        "x-corelink-scope": "admin",
        "x-corelink-tenant-id": "attacker-tenant",
      },
    }, capturingEnv);
    expect(captured).not.toBeNull();
    expect(captured!.auth).toBe("Bearer oci-hmac-bearer-token");
    // The Worker injects neither scope nor tenant for OCI, and strips the forged
    // client values (defense-in-depth: tenant-id deleted in the pass-through).
    expect(captured!.scope).toBeNull();
    expect(captured!.tenant).toBeNull();
    expect(captured!.routeKind).toBe("oci_v2");
  });

  it("returns 401 for REAPI /api/v2/ without Authorization", async () => {
    const resp = await workerFetch("http://localhost/api/v2/some/path");
    expect(resp.status).toBe(401);
  });

  it("returns REAPI error field for /api/v2/ 401", async () => {
    const resp = await workerFetch("http://localhost/api/v2/some/path");
    const body = await resp.json() as { error: string };
    expect(body.error).toBe("UNAUTHORIZED");
    // NOT errors array
    expect((body as unknown as { errors?: unknown }).errors).toBeUndefined();
  });

  it("returns 401 for token too short (<32 chars)", async () => {
    const resp = await workerFetch("http://localhost/api/v2/t/path", {
      headers: { Authorization: "Bearer shorttoken" },
    });
    expect(resp.status).toBe(401);
  });

  it("returns 401 for token too long (>256 chars)", async () => {
    const longToken = "a".repeat(300);
    const resp = await workerFetch("http://localhost/api/v2/t/path", {
      headers: { Authorization: `Bearer ${longToken}` },
    });
    expect(resp.status).toBe(401);
  });

  it("returns 401 for wrong scheme (Basic)", async () => {
    const resp = await workerFetch("http://localhost/api/v2/t/path", {
      headers: { Authorization: "Basic dXNlcjpwYXNz" },
    });
    expect(resp.status).toBe(401);
  });

  it("returns 401 for missing Authorization entirely", async () => {
    const resp = await workerFetch("http://localhost/npm/t/package");
    expect(resp.status).toBe(401);
  });

  it("passes valid canonical PAT (found in D1) to DO (expects non-401)", async () => {
    // TEST_PAT_TOKEN has token_id AAAAAAAAAAAAAAAA which the default D1 mock recognises.
    const resp = await workerFetch(`http://localhost/api/v2/${TEST_TENANT_ID}/path`, {
      headers: { Authorization: `Bearer ${TEST_PAT_TOKEN}` },
    });
    expect(resp.status).not.toBe(401);
    // DO mock returns 503
    expect(resp.status).toBe(503);
  });

  it("does not reflect raw token value in error response", async () => {
    const sensitiveToken = "MYSECRETTOKEN123" + "x".repeat(48);
    const resp = await workerFetch("http://localhost/api/v2/t/p", {
      headers: { Authorization: `Bearer ${sensitiveToken}` },
    });
    const text = await resp.text();
    expect(text).not.toContain(sensitiveToken);
    expect(text).not.toContain("MYSECRETTOKEN");
  });

  // ── WP-A1 new tests (DoD 2) ────────────────────────────────────────────────

  it("returns 401 for a random 64-char printable-ASCII string (not canonical PAT format)", async () => {
    // Before WP-A1 this would have returned 503 (passed format check, forwarded to DO).
    // After WP-A1 it must return 401 because it is not `corelink_<env>_…` format.
    const randomJunk = "x".repeat(64);
    const resp = await workerFetch("http://localhost/api/v2/t/path", {
      headers: { Authorization: `Bearer ${randomJunk}` },
    });
    expect(resp.status).toBe(401);
  });

  it("returns 401 for valid-format PAT whose token_id is not in the D1 store", async () => {
    // Build a canonically-shaped PAT with a token_id that the D1 mock does NOT recognise.
    const unknownTokenId = "BBBBBBBBBBBBBBBB"; // 16 valid Crockford b32 chars, not in D1
    const patNotInD1 =
      "corelink_pat_" +
      unknownTokenId +
      "." +
      "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA" + // 43 base64url
      "." +
      "AAAAAAAAAAAAAAAAAAAAAA"; // 22 base64url
    expect(patNotInD1.length).toBe(96); // sanity

    // The default D1 mock only knows AAAAAAAAAAAAAAAA, so BBBBBBBBBBBBBBBB → 401.
    const resp = await workerFetch("http://localhost/api/v2/t/path", {
      headers: { Authorization: `Bearer ${patNotInD1}` },
    });
    expect(resp.status).toBe(401);
  });

  it("returns 401 for valid-format PAT that is expired in D1", async () => {
    const expiredTokenId = "CCCCCCCCCCCCCCCC"; // 16 Crockford b32 chars
    const patExpired =
      "corelink_ci_" + // env=ci → 95 chars total
      expiredTokenId +
      "." +
      "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA" + // 43 base64url
      "." +
      "AAAAAAAAAAAAAAAAAAAAAA"; // 22 base64url
    expect(patExpired.length).toBe(95); // ci → 95

    // D1 mock with expired row for CCCCCCCCCCCCCCCC (expires_ms in the past).
    const d1WithExpired = makeD1Mock(new Map([
      [expiredTokenId, { tenant_id: TEST_TENANT_ID, expires_ms: Date.now() - 1000 }],
    ]));
    const resp = await workerFetch("http://localhost/api/v2/t/path", {
      headers: { Authorization: `Bearer ${patExpired}` },
    }, { CONFIG_DB: d1WithExpired });
    expect(resp.status).toBe(401);
  });

  it("returns 401 for valid-format PAT that is soft-revoked in D1 (revoked_at_ms set)", async () => {
    // WP-2 (dashboard revival): the PAT lookup SQL gains
    // `AND revoked_at_ms IS NULL` (migration 0063), so a revoked row is
    // indistinguishable from an absent one → uniform 401.
    const revokedTokenId = "DDDDDDDDDDDDDDDD"; // 16 Crockford b32 chars
    const patRevoked =
      "corelink_pat_" +
      revokedTokenId +
      "." +
      "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA" + // 43 base64url
      "." +
      "AAAAAAAAAAAAAAAAAAAAAA"; // 22 base64url
    expect(patRevoked.length).toBe(96); // sanity

    // D1 mock with a NON-expired but soft-revoked row for DDDDDDDDDDDDDDDD.
    const d1WithRevoked = makeD1Mock(new Map([
      [revokedTokenId, {
        tenant_id: TEST_TENANT_ID,
        expires_ms: Date.now() + 3_600_000,
        scope: "cas:rw",
        revoked_at_ms: Date.now() - 1000,
      }],
    ]));
    const resp = await workerFetch("http://localhost/api/v2/t/path", {
      headers: { Authorization: `Bearer ${patRevoked}` },
    }, { CONFIG_DB: d1WithRevoked });
    expect(resp.status).toBe(401);
  });

  it("still accepts a PAT whose row has NULL revoked_at_ms (active key)", async () => {
    // Guard the inverse: an explicit NULL revoked_at_ms (the post-0063
    // shape of every active row) must keep resolving.
    const d1ActiveNullRevoked = makeD1Mock(new Map([
      [TEST_TOKEN_ID, {
        tenant_id: TEST_TENANT_ID,
        expires_ms: Date.now() + 3_600_000,
        revoked_at_ms: null,
      }],
    ]));
    const resp = await workerFetch(`http://localhost/api/v2/${TEST_TENANT_ID}/path`, {
      headers: { Authorization: `Bearer ${TEST_PAT_TOKEN}` },
    }, { CONFIG_DB: d1ActiveNullRevoked });
    expect(resp.status).not.toBe(401);
    expect(resp.status).toBe(503); // DO stub
  });

  it("resolves real tenant_id from D1 and passes it to the DO via trusted header", async () => {
    let capturedTenantId: string | null = null;
    const capturingDO: Partial<Env> = {
      CORELINK_SERVER: {
        idFromName: (_n: string) => ({ toString: () => "stub-id" }),
        get: () => ({
          fetch: async (req: Request): Promise<Response> => {
            capturedTenantId = req.headers.get("x-corelink-tenant-id");
            return new Response(JSON.stringify({ ok: true }), {
              status: 200,
              headers: { "Content-Type": "application/json" },
            });
          },
        }),
        idFromString: (_s: string) => ({ toString: () => "stub-id" }),
        newUniqueId: () => ({ toString: () => "stub-unique-id" }),
        jurisdiction: (_j: string) => capturingDO.CORELINK_SERVER,
      } as unknown as DurableObjectNamespace,
    };

    await workerFetch(`http://localhost/api/v2/${TEST_TENANT_ID}/path`, {
      headers: { Authorization: `Bearer ${TEST_PAT_TOKEN}` },
    }, capturingDO);

    // The DO should receive the resolved tenant_id from D1 (not "_pending").
    expect(capturedTenantId).toBe(TEST_TENANT_ID);
    expect(capturedTenantId).not.toBe("_pending");
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// pip adapter HTTP Basic auth (fix/worker-pip-basic-auth)
//
// pip/uv natively emit ONLY URL-embedded HTTP Basic
// (`https://hugr:<PAT>@host/pip/<tenant>/simple/` → `Authorization: Basic
// base64("hugr:<PAT>")`) and never `Authorization: Bearer`. The documented pip
// recipe (apps/docs/docs/integrations/pip.md) was unusable because the edge
// Worker rejected every non-Bearer scheme with 401 `invalid_scheme` BEFORE the
// adapter ran. The Worker now accepts Basic on the `pip` route ONLY, taking the
// password as the PAT and verifying it through the identical HMAC + D1 gate.
// ──────────────────────────────────────────────────────────────────────────────

describe("pip adapter Basic auth", () => {
  // `hugr` is the documented (ignored) username; the password IS the PAT.
  const basic = (user: string, pass: string): string =>
    `Basic ${btoa(`${user}:${pass}`)}`;

  it("forwards a Basic credential whose password is a valid PAT (pip route → non-401)", async () => {
    // urlTenant must equal the PAT-resolved tenant or the spoof gate 403s;
    // TEST_PAT_TOKEN resolves to TEST_TENANT_ID via the default D1 mock.
    const resp = await workerFetch(
      `http://localhost/pip/${TEST_TENANT_ID}/simple/requests/`,
      { headers: { Authorization: basic("hugr", TEST_PAT_TOKEN) } },
    );
    expect(resp.status).not.toBe(401);
    expect(resp.status).toBe(503); // DO stub — auth passed, forwarded downstream
  });

  it("forwards the PAT (password) to the DO exactly as a Bearer PAT would (same tenant header)", async () => {
    let capturedTenantId: string | null = null;
    let capturedAuth: string | null = null;
    const capturingDO: Partial<Env> = {
      CORELINK_SERVER: {
        idFromName: (_n: string) => ({ toString: () => "stub-id" }),
        get: () => ({
          fetch: async (req: Request): Promise<Response> => {
            capturedTenantId = req.headers.get("x-corelink-tenant-id");
            capturedAuth = req.headers.get("authorization");
            return new Response(JSON.stringify({ ok: true }), {
              status: 200,
              headers: { "Content-Type": "application/json" },
            });
          },
        }),
        idFromString: (_s: string) => ({ toString: () => "stub-id" }),
        newUniqueId: () => ({ toString: () => "stub-unique-id" }),
        jurisdiction: (_j: string) => capturingDO.CORELINK_SERVER,
      } as unknown as DurableObjectNamespace,
    };

    const resp = await workerFetch(
      `http://localhost/pip/${TEST_TENANT_ID}/simple/flask/`,
      { headers: { Authorization: basic("hugr", TEST_PAT_TOKEN) } },
      capturingDO,
    );
    expect(resp.status).toBe(200);
    // Tenant resolved from the PAT (the Basic password), not the URL blindly.
    expect(capturedTenantId).toBe(TEST_TENANT_ID);
    // The container re-derives auth from x-corelink-tenant-id, but the original
    // Basic header is forwarded verbatim (adapter also supports basic auth).
    expect(capturedAuth).toBe(basic("hugr", TEST_PAT_TOKEN));
  });

  it("ignores the username label (any username, valid PAT password → non-401)", async () => {
    const resp = await workerFetch(
      `http://localhost/pip/${TEST_TENANT_ID}/simple/numpy/`,
      { headers: { Authorization: basic("anything", TEST_PAT_TOKEN) } },
    );
    expect(resp.status).not.toBe(401);
    expect(resp.status).toBe(503);
  });

  it("returns 401 for Basic whose password is a valid-format PAT not in D1", async () => {
    const unknownTokenId = "BBBBBBBBBBBBBBBB"; // not in the default D1 mock
    const patNotInD1 =
      "corelink_pat_" + unknownTokenId + "." +
      "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA" + "." +
      "AAAAAAAAAAAAAAAAAAAAAA";
    const resp = await workerFetch(
      `http://localhost/pip/${TEST_TENANT_ID}/simple/requests/`,
      { headers: { Authorization: basic("hugr", patNotInD1) } },
    );
    expect(resp.status).toBe(401);
  });

  it("returns 401 for Basic whose password is not a canonical PAT (junk)", async () => {
    const resp = await workerFetch(
      `http://localhost/pip/${TEST_TENANT_ID}/simple/requests/`,
      { headers: { Authorization: basic("hugr", "x".repeat(64)) } },
    );
    expect(resp.status).toBe(401);
  });

  it("returns 401 for malformed Basic — payload has no `:` separator", async () => {
    // base64("nocolonhere") — decodes but lacks the user:pass separator.
    const resp = await workerFetch(
      `http://localhost/pip/${TEST_TENANT_ID}/simple/requests/`,
      { headers: { Authorization: `Basic ${btoa("nocolonhere")}` } },
    );
    expect(resp.status).toBe(401);
  });

  it("returns 401 for malformed Basic — payload is not valid base64", async () => {
    const resp = await workerFetch(
      `http://localhost/pip/${TEST_TENANT_ID}/simple/requests/`,
      { headers: { Authorization: "Basic !!!not-base64!!!" } },
    );
    expect(resp.status).toBe(401);
  });

  it("returns 401 for Basic with an empty password (`hugr:`)", async () => {
    const resp = await workerFetch(
      `http://localhost/pip/${TEST_TENANT_ID}/simple/requests/`,
      { headers: { Authorization: `Basic ${btoa("hugr:")}` } },
    );
    expect(resp.status).toBe(401);
  });

  it("still accepts Bearer of the same PAT on the pip route (unchanged)", async () => {
    const resp = await workerFetch(
      `http://localhost/pip/${TEST_TENANT_ID}/simple/requests/`,
      { headers: { Authorization: `Bearer ${TEST_PAT_TOKEN}` } },
    );
    expect(resp.status).not.toBe(401);
    expect(resp.status).toBe(503);
  });

  // ── Scope guard: Basic is accepted ONLY on the pip route ────────────────────

  it("SCOPE GUARD: Basic with a VALID PAT password on /v1/cas → still 401 invalid_scheme", async () => {
    // The native CAS/AC API must NEVER accept Basic — even when the password is a
    // cryptographically valid, D1-known PAT. This is the anti-broadening guard.
    const resp = await workerFetch("http://localhost/v1/cas/somehash", {
      headers: { Authorization: basic("hugr", TEST_PAT_TOKEN) },
    });
    expect(resp.status).toBe(401);
  });

  it("SCOPE GUARD: Basic with a valid PAT password on /api/v2 (REAPI) → still 401", async () => {
    const resp = await workerFetch(`http://localhost/api/v2/${TEST_TENANT_ID}/path`, {
      headers: { Authorization: basic("hugr", TEST_PAT_TOKEN) },
    });
    expect(resp.status).toBe(401);
  });

  it("SCOPE GUARD: Basic with a valid PAT password on /npm (npm route) → still 401", async () => {
    const resp = await workerFetch(`http://localhost/npm/${TEST_TENANT_ID}/some-pkg`, {
      headers: { Authorization: basic("hugr", TEST_PAT_TOKEN) },
    });
    expect(resp.status).toBe(401);
  });

  it("SCOPE GUARD: Basic with a valid PAT password on /cargo → still 401", async () => {
    const resp = await workerFetch(`http://localhost/cargo/${TEST_TENANT_ID}/api/v1/crates/serde`, {
      headers: { Authorization: basic("hugr", TEST_PAT_TOKEN) },
    });
    expect(resp.status).toBe(401);
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// CORS
// ──────────────────────────────────────────────────────────────────────────────
// B-126 M3 split population: index_part2.test.ts, index_part3.test.ts, index_part4.test.ts, index_part5.test.ts
