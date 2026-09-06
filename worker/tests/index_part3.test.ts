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

describe("OCI pass-through — container-owned status + envelope", () => {
  it("DO/container OCI envelope (404 BLOB_UNKNOWN + Docker-Distribution header) is forwarded verbatim", async () => {
    // The container — NOT the Worker — produces the OCI envelope AND the OCI-spec
    // headers. The Worker must hand them back unchanged (verbatim pass-through).
    const blobUnknownEnv: Partial<Env> = {
      CORELINK_SERVER: {
        idFromName: (_n: string) => ({ toString: () => "id" }),
        get: () => ({
          fetch: async (req: Request) =>
            new Response(
              JSON.stringify({ errors: [{ code: "BLOB_UNKNOWN", message: "blob not found", detail: null }] }),
              {
                status: 404,
                headers: {
                  "Content-Type": "application/json",
                  "Docker-Distribution-Api-Version": "registry/2.0",
                  "X-Request-Id": req.headers.get("x-request-id") ?? "stub",
                },
              },
            ),
        }),
        idFromString: (_s: string) => ({ toString: () => "id" }),
        newUniqueId: () => ({ toString: () => "id" }),
        jurisdiction: (_j: string) => blobUnknownEnv.CORELINK_SERVER,
      } as unknown as DurableObjectNamespace,
    };
    const resp = await workerFetch(
      `http://localhost/v2/myrepo/blobs/sha256:deadbeef`,
      undefined,
      blobUnknownEnv,
    );
    // Status, body, and the container-supplied OCI header all pass through.
    expect(resp.status).toBe(404);
    expect(resp.headers.get("x-request-id")).not.toBeNull();
    expect(resp.headers.get("docker-distribution-api-version")).toBe("registry/2.0");
    const body = await resp.json() as { errors: Array<{ code: string }> };
    expect(body.errors[0]?.code).toBe("BLOB_UNKNOWN");
  });

  it("container 401 + Www-Authenticate (the OCI auth challenge) is forwarded verbatim", async () => {
    // The OCI 401/Www-Authenticate challenge is now the CONTAINER's job (the
    // first leg of OCI two-leg auth). The Worker forwards it unchanged and never
    // builds its own 401 for OCI routes.
    const challengeEnv: Partial<Env> = {
      CORELINK_SERVER: {
        idFromName: (_n: string) => ({ toString: () => "id" }),
        get: () => ({
          fetch: async (req: Request) =>
            new Response(
              JSON.stringify({ errors: [{ code: "UNAUTHORIZED", message: "authentication required", detail: null }] }),
              {
                status: 401,
                headers: {
                  "Content-Type": "application/json",
                  "WWW-Authenticate": 'Bearer realm="https://corelink.test/token",service="corelink-oci"',
                  "Docker-Distribution-Api-Version": "registry/2.0",
                  "X-Request-Id": req.headers.get("x-request-id") ?? "stub",
                },
              },
            ),
        }),
        idFromString: (_s: string) => ({ toString: () => "id" }),
        newUniqueId: () => ({ toString: () => "id" }),
        jurisdiction: (_j: string) => challengeEnv.CORELINK_SERVER,
      } as unknown as DurableObjectNamespace,
    };
    const resp = await workerFetch("http://localhost/v2/repo/blobs/sha256:abc", undefined, challengeEnv);
    expect(resp.status).toBe(401);
    // The Www-Authenticate challenge must survive the forward (it drives the
    // OCI client to the /token second leg).
    expect(resp.headers.get("www-authenticate")).toContain("Bearer realm=");
    const body = await resp.json() as { errors: Array<{ code: string }> };
    expect(body.errors[0]?.code).toBe("UNAUTHORIZED");
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// /api/health alias (e2e Journey 1)
// ──────────────────────────────────────────────────────────────────────────────

describe("GET /api/health (e2e Journey 1 alias)", () => {
  it("returns 200 with status:SERVING (matches Journey 1 exact-string assertion)", async () => {
    // Reference: tests/e2e-user-journeys/src/main.rs:151
    //   if health_body["status"].as_str() != Some("SERVING") { ... }
    const resp = await workerFetch("http://localhost/api/health");
    expect(resp.status).toBe(200);
    const body = await resp.json() as { status: string };
    expect(body.status).toBe("SERVING");
  });

  it("does not require Authorization header", async () => {
    const resp = await workerFetch("http://localhost/api/health");
    expect(resp.status).toBe(200);
  });

  it("legacy /health remains unchanged (status:ok)", async () => {
    // Backward-compat guard: /health MUST keep "ok" — otherwise existing
    // monitors/probes will break.
    const resp = await workerFetch("http://localhost/health");
    const body = await resp.json() as { status: string };
    expect(body.status).toBe("ok");
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// /v1/* route family (e2e user-journeys suite)
// ──────────────────────────────────────────────────────────────────────────────

describe("route resolution — /v1/* family", () => {
  // Capture the route-kind and tenant the worker forwards to the DO so we can
  // assert that /v1/* paths resolve to reapi_v1 / signup with tenantId=_anonymous
  // (preserving any future path-spoof gate that only fires when urlTenant is a
  // real tenant id).
  function makeCapturingEnv(): { env: Partial<Env>; captured: { routeKind?: string; auth?: string | null } } {
    const captured: { routeKind?: string; auth?: string | null } = {};
    const env: Partial<Env> = {
      CORELINK_SERVER: {
        idFromName: (_n: string) => ({ toString: () => "stub-id" }),
        get: () => ({
          fetch: async (req: Request): Promise<Response> => {
            captured.routeKind = req.headers.get("x-corelink-route-kind") ?? undefined;
            captured.auth = req.headers.get("authorization");
            return new Response(JSON.stringify({ ok: true }), {
              status: 200,
              headers: { "Content-Type": "application/json" },
            });
          },
        }),
        idFromString: (_s: string) => ({ toString: () => "stub-id" }),
        newUniqueId: () => ({ toString: () => "stub-id" }),
        jurisdiction: (_j: string) => env.CORELINK_SERVER,
      } as unknown as DurableObjectNamespace,
    };
    return { env, captured };
  }

  it("GET /v1/users/me forwards to DO as routeKind=reapi_v1 (PAT required)", async () => {
    const { env, captured } = makeCapturingEnv();
    const token = TEST_PAT_TOKEN;
    const resp = await workerFetch(
      "http://localhost/v1/users/me",
      { headers: { Authorization: `Bearer ${token}` } },
      env,
    );
    expect(resp.status).toBe(200);
    expect(captured.routeKind).toBe("reapi_v1");
    // Authorization MUST be forwarded so the DO can validate the PAT
    expect(captured.auth).toBe(`Bearer ${token}`);
  });

  it("GET /v1/users/me without Authorization returns 401 (REAPI envelope)", async () => {
    // Journey 2 negative-path assertion (tests/e2e-user-journeys/src/main.rs:288)
    //   if resp_no_auth.status() != 401 { ... }
    const resp = await workerFetch("http://localhost/v1/users/me");
    expect(resp.status).toBe(401);
    const body = await resp.json() as { error: string };
    expect(body.error).toBe("UNAUTHORIZED");
  });

  it("PUT /v1/cas/blobs/<digest>/<size> forwards as reapi_v1 (no urlTenant)", async () => {
    const { env, captured } = makeCapturingEnv();
    const token = TEST_PAT_TOKEN;
    const resp = await workerFetch(
      "http://localhost/v1/cas/blobs/sha256:deadbeef/1024",
      { method: "PUT", headers: { Authorization: `Bearer ${token}` }, body: "x" },
      env,
    );
    expect(resp.status).toBe(200);
    expect(captured.routeKind).toBe("reapi_v1");
  });

  it("GET /v1/admin/audit/events forwards as reapi_v1 (admin scope enforced downstream)", async () => {
    const { env, captured } = makeCapturingEnv();
    const token = TEST_PAT_TOKEN;
    const resp = await workerFetch(
      "http://localhost/v1/admin/audit/events",
      { headers: { Authorization: `Bearer ${token}` } },
      env,
    );
    expect(resp.status).toBe(200);
    expect(captured.routeKind).toBe("reapi_v1");
  });

  it("POST /v1/signup/pilot/:token forwards as signup WITHOUT requiring a Bearer PAT", async () => {
    // Signup is pre-tenant — the :token in the path IS the auth artifact.
    // The Worker MUST NOT require Authorization here; the DO/container
    // validates the path token against the signup-tokens store.
    const { env, captured } = makeCapturingEnv();
    const resp = await workerFetch(
      "http://localhost/v1/signup/pilot/abc123-signup-token",
      { method: "POST", body: JSON.stringify({ email: "x@example.com" }) },
      env,
    );
    expect(resp.status).toBe(200);
    expect(captured.routeKind).toBe("signup");
    // No Authorization header should have been added or required
    expect(captured.auth).toBeNull();
  });

  it("legacy /api/v2/<tenant>/* still resolves to reapi_v2 (no regression)", async () => {
    const { env, captured } = makeCapturingEnv();
    const token = TEST_PAT_TOKEN;
    const resp = await workerFetch(
      `http://localhost/api/v2/${TEST_TENANT_ID}/blobs/sha256:abc`,
      { headers: { Authorization: `Bearer ${token}` } },
      env,
    );
    expect(resp.status).toBe(200);
    expect(captured.routeKind).toBe("reapi_v2");
  });

  // ── /v1/customer/* — customer_v1 route family ────────────────────────────────

  it("GET /v1/customer/overview forwards to DO as routeKind=customer_v1", async () => {
    const { env, captured } = makeCapturingEnv();
    const token = TEST_PAT_TOKEN;
    const resp = await workerFetch(
      "http://localhost/v1/customer/overview",
      { headers: { Authorization: `Bearer ${token}` } },
      env,
    );
    expect(resp.status).toBe(200);
    expect(captured.routeKind).toBe("customer_v1");
    // Authorization MUST be forwarded so the DO can validate the PAT
    expect(captured.auth).toBe(`Bearer ${token}`);
  });

  it("POST /v1/customer/keys forwards to DO as routeKind=customer_v1", async () => {
    const { env, captured } = makeCapturingEnv();
    const token = TEST_PAT_TOKEN;
    const resp = await workerFetch(
      "http://localhost/v1/customer/keys",
      { method: "POST", headers: { Authorization: `Bearer ${token}` }, body: JSON.stringify({ name: "ci-key" }) },
      env,
    );
    expect(resp.status).toBe(200);
    expect(captured.routeKind).toBe("customer_v1");
  });

  it("POST /v1/customer/keys/:id/revoke forwards to DO as routeKind=customer_v1", async () => {
    const { env, captured } = makeCapturingEnv();
    const token = TEST_PAT_TOKEN;
    const resp = await workerFetch(
      "http://localhost/v1/customer/keys/key-abc-123/revoke",
      { method: "POST", headers: { Authorization: `Bearer ${token}` } },
      env,
    );
    expect(resp.status).toBe(200);
    expect(captured.routeKind).toBe("customer_v1");
  });

  it("GET /v1/customer/overview without Authorization returns 401 (auth required)", async () => {
    // customer_v1 is NOT pre-tenant — PAT is required (unlike /v1/signup/*)
    const resp = await workerFetch("http://localhost/v1/customer/overview");
    expect(resp.status).toBe(401);
    const body = await resp.json() as { error: string };
    expect(body.error).toBe("UNAUTHORIZED");
  });

  it("GET /v1/customer/billing/portal without Authorization returns 401", async () => {
    // Deep sub-path also requires PAT
    const resp = await workerFetch("http://localhost/v1/customer/billing/portal");
    expect(resp.status).toBe(401);
    const body = await resp.json() as { error: string };
    expect(body.error).toBe("UNAUTHORIZED");
  });

  it("GET /v1/customer/overview with valid PAT forwards to DO (tenantId=_anonymous preserved)", async () => {
    // Tenant is NOT in the URL — the DO resolves it from the PAT.
    // Worker sets tenantId="_anonymous" in the route match; the PAT-resolved
    // tenant is forwarded via x-corelink-tenant-id header to the DO.
    const { env, captured } = makeCapturingEnv();
    const token = TEST_PAT_TOKEN;
    const resp = await workerFetch(
      "http://localhost/v1/customer/overview",
      { headers: { Authorization: `Bearer ${token}` } },
      env,
    );
    expect(resp.status).toBe(200);
    expect(captured.routeKind).toBe("customer_v1");
    // Auth is forwarded — DO performs Argon2id + scope check
    expect(captured.auth).toBe(`Bearer ${token}`);
  });

  it("/v1/customer/* does NOT fall through to reapi_v1 (specificity gate)", async () => {
    // Ensure customer paths get customer_v1, not the generic reapi_v1 bucket
    const { env, captured } = makeCapturingEnv();
    const token = TEST_PAT_TOKEN;
    const resp = await workerFetch(
      "http://localhost/v1/customer/audit",
      { headers: { Authorization: `Bearer ${token}` } },
      env,
    );
    expect(resp.status).toBe(200);
    expect(captured.routeKind).toBe("customer_v1");
    expect(captured.routeKind).not.toBe("reapi_v1");
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// Request-Id forwarding to DO
// ──────────────────────────────────────────────────────────────────────────────

describe("X-Request-Id forwarding to DO", () => {
  let capturedRequestId: string | null = null;

  it("forwards x-request-id to DO stub", async () => {
    const requestId = "forward-test-" + Math.random().toString(36).slice(2);
    const capturingEnv: Partial<Env> = {
      CORELINK_SERVER: {
        idFromName: (_name: string) => ({ toString: () => "stub-id" }),
        get: (_id: unknown) => ({
          fetch: async (req: Request): Promise<Response> => {
            capturedRequestId = req.headers.get("x-request-id");
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

    await workerFetch(`http://localhost/api/v2/${TEST_TENANT_ID}/path`, {
      headers: {
        Authorization: `Bearer ${TEST_PAT_TOKEN}`,
        "x-request-id": requestId,
      },
    }, capturingEnv);

    expect(capturedRequestId).toBe(requestId);
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// PAT format parser — coverage for parsePat edge cases (WP-A1)
// ──────────────────────────────────────────────────────────────────────────────

describe("PAT format validation — parsePat edge cases", () => {
  // Helper: build a PAT-shaped token with a PLACEHOLDER (all-A) hmac_sig and a
  // specific env / token_id. Such a token PARSES but FAILS the HMAC fast-fail
  // (the all-A sig isn't a real MAC) — used by the negative tests below that
  // assert the parser/charset rejections (401), all reached at/before the HMAC
  // gate, so a real signature is irrelevant to what they prove.
  function makePat(env: "pat" | "ci" | "ro", tokenId = TEST_TOKEN_ID): string {
    return (
      "corelink_" + env + "_" +
      tokenId +
      "." +
      "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA" +
      "." +
      "AAAAAAAAAAAAAAAAAAAAAA"
    );
  }

  // Helper: mint a CRYPTOGRAPHICALLY VALID ci/ro-env PAT for TEST_TOKEN_ID.
  // The HMAC preimage is `<token_id>.<random_secret>` — the env segment is NOT
  // part of it (see parsePat in src/index.ts) — so we mint the canonical
  // pat-env token (whose sig verifies under TEST_PAT_SIGNING_KEY for the default
  // token_id/secret the D1 mock recognises) and rewrite ONLY the env prefix to
  // get the 95-char ci/ro variant carrying the SAME valid signature.
  async function mintEnvPat(env: "ci" | "ro"): Promise<string> {
    const patToken = await mintTestPat(); // corelink_pat_<id>.<secret>.<sig>
    return patToken.replace(/^corelink_pat_/, `corelink_${env}_`);
  }

  it("accepts env=ci (valid sig, 95 chars, token in D1) → 503 (reaches DO)", async () => {
    // Validly-signed ci-env PAT with TEST_TOKEN_ID — the D1 mock recognises it,
    // so it clears parse + HMAC + D1 lookup and reaches the DO stub (503).
    const ciPat = await mintEnvPat("ci");
    expect(ciPat.length).toBe(95);
    const resp = await workerFetch(`http://localhost/api/v2/${TEST_TENANT_ID}/p`, {
      headers: { Authorization: `Bearer ${ciPat}` },
    });
    // Reaches the DO (503 from stub), not 401 — proves the ci env is accepted.
    expect(resp.status).toBe(503);
  });

  it("accepts env=ro (valid sig, 95 chars, token in D1) → 503 (reaches DO)", async () => {
    const roPat = await mintEnvPat("ro");
    expect(roPat.length).toBe(95);
    const resp = await workerFetch(`http://localhost/api/v2/${TEST_TENANT_ID}/p`, {
      headers: { Authorization: `Bearer ${roPat}` },
    });
    expect(resp.status).toBe(503);
  });

  it("rejects wrong prefix (not corelink_)", async () => {
    // Replace 'corelink_' with 'wrongpfx_'
    const bad = "wrongpfx_pat_" + TEST_TOKEN_ID + "." + "A".repeat(43) + "." + "A".repeat(22);
    const resp = await workerFetch("http://localhost/api/v2/t/p", {
      headers: { Authorization: `Bearer ${bad}` },
    });
    expect(resp.status).toBe(401);
  });

  it("rejects unknown env segment (e.g. 'xx')", async () => {
    // 95 chars with env='xx' (invalid)
    const bad = "corelink_xx_" + TEST_TOKEN_ID + "." + "A".repeat(43) + "." + "A".repeat(22);
    expect(bad.length).toBe(95);
    const resp = await workerFetch("http://localhost/api/v2/t/p", {
      headers: { Authorization: `Bearer ${bad}` },
    });
    expect(resp.status).toBe(401);
  });

  it("rejects wrong separator after env (dot instead of underscore)", async () => {
    const bad = "corelink_pat." + TEST_TOKEN_ID + "." + "A".repeat(43) + "." + "A".repeat(22);
    expect(bad.length).toBe(96);
    const resp = await workerFetch("http://localhost/api/v2/t/p", {
      headers: { Authorization: `Bearer ${bad}` },
    });
    expect(resp.status).toBe(401);
  });

  it("rejects invalid Crockford b32 in token_id (contains 'I' which is excluded)", async () => {
    // Replace one char in token_id with 'I' (excluded from Crockford b32)
    const badTokenId = "IIIIIIIIIIIIIIII"; // 16 'I' chars — invalid
    const bad = makePat("pat", badTokenId);
    const resp = await workerFetch("http://localhost/api/v2/t/p", {
      headers: { Authorization: `Bearer ${bad}` },
    });
    expect(resp.status).toBe(401);
  });

  it("rejects invalid base64url in random_secret (contains '=')", async () => {
    // '=' is NOT valid in base64url-no-pad
    const badSecret = "=".repeat(43);
    const bad = "corelink_pat_" + TEST_TOKEN_ID + "." + badSecret + "." + "A".repeat(22);
    expect(bad.length).toBe(96);
    const resp = await workerFetch("http://localhost/api/v2/t/p", {
      headers: { Authorization: `Bearer ${bad}` },
    });
    expect(resp.status).toBe(401);
  });

  it("rejects wrong separator between random_secret and hmac_sig", async () => {
    // Replace the '.' between random_secret and hmac_sig with '_'
    const bad = "corelink_pat_" + TEST_TOKEN_ID + "." + "A".repeat(43) + "_" + "A".repeat(22);
    expect(bad.length).toBe(96);
    const resp = await workerFetch("http://localhost/api/v2/t/p", {
      headers: { Authorization: `Bearer ${bad}` },
    });
    expect(resp.status).toBe(401);
  });

  it("fail-closed: no PAT_SIGNING_KEY in env => 503 (intentional, NOT a masked 401)", async () => {
    // F18 — explicit coverage of the fail-CLOSED native-plane gate. When the
    // signing key is ABSENT, extractAuth must short-circuit to 503 (operator
    // misconfig / alert) BEFORE any HMAC or D1 work — never silently skip the
    // possession check and never collapse into a 401. This is the path that, as
    // the SILENT default, made "passing" PAT tests pass on misconfig (test
    // theater); asserting it on PURPOSE here is what lets every OTHER PAT test
    // bind a real key and exercise the real auth logic.
    //
    // A VALID-format, validly-signed PAT is presented so the ONLY reason this
    // can return 503 is the absent key (not a malformed token): with a key bound
    // the same request flows to the DO. `PAT_SIGNING_KEY: undefined` overrides
    // makeEnv()'s default bound key.
    const resp = await workerFetch("http://localhost/api/v2/t/p", {
      headers: { Authorization: `Bearer ${TEST_PAT_TOKEN}` },
    }, { PAT_SIGNING_KEY: undefined });
    expect(resp.status).toBe(503);
    const body = await resp.json() as { error: string };
    // The 503 envelope is the worker's misconfig signal, NOT an UNAUTHORIZED 401.
    expect(body.error).not.toBe("UNAUTHORIZED");
  });

  it("fail-closed: too-short PAT_SIGNING_KEY (<64 hex) => 503", async () => {
    // A present-but-too-short key (< 32 decoded bytes) is also a LOUD misconfig:
    // extractAuth must fail CLOSED (503), never weaken to accept it.
    const resp = await workerFetch("http://localhost/api/v2/t/p", {
      headers: { Authorization: `Bearer ${TEST_PAT_TOKEN}` },
    }, { PAT_SIGNING_KEY: "abcd" });
    expect(resp.status).toBe(503);
  });

  it("HMAC fast-fail: rejects if PAT_SIGNING_KEY is set but sig is wrong", async () => {
    // Provide a signing key (32 hex bytes = 64 hex chars) but the token has all-A hmac_sig
    // which won't match the HMAC-SHA256 of the preimage with this key.
    const signingKey = "00".repeat(32); // 64 hex chars = 32 bytes (all-zero key)
    const resp = await workerFetch("http://localhost/api/v2/t/p", {
      headers: { Authorization: `Bearer ${TEST_PAT_TOKEN}` },
    }, { PAT_SIGNING_KEY: signingKey });
    // The HMAC will not match (test token's hmac_sig is all-A, not a real MAC)
    expect(resp.status).toBe(401);
  });

  it("D1 lookup error (throws) → 503 (transient fault, fail-closed)", async () => {
    // D1 mock that throws on prepare/bind/first
    const throwingD1 = {
      prepare: () => ({
        bind: () => ({
          first: async () => { throw new Error("D1 connection refused"); },
        }),
      }),
    } as unknown as D1Database;
    const resp = await workerFetch("http://localhost/api/v2/t/p", {
      headers: { Authorization: `Bearer ${TEST_PAT_TOKEN}` },
    }, { CONFIG_DB: throwingD1 });
    // D1 throw is a TRANSIENT infra fault → 503 (retryable), still fail-closed (access denied); NOT 401 "bad credentials" (H1). See the reason→status map in index.ts.
    expect(resp.status).toBe(503);
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// Resolved tenant header forwarded to DO (DoD 3: no "_pending" reaching DO)
// ──────────────────────────────────────────────────────────────────────────────

describe("resolved tenant_id forwarding (WP-A1 DoD 3)", () => {
  it("x-corelink-tenant-id header is set to the D1 tenant_id on authenticated request", async () => {
    let capturedHeader: string | null = null;
    const capturingDO: Partial<Env> = {
      CORELINK_SERVER: {
        idFromName: (_n: string) => ({ toString: () => "id" }),
        get: () => ({
          fetch: async (req: Request) => {
            capturedHeader = req.headers.get("x-corelink-tenant-id");
            return new Response("{}", { status: 200, headers: { "Content-Type": "application/json" } });
          },
        }),
        idFromString: (_s: string) => ({ toString: () => "id" }),
        newUniqueId: () => ({ toString: () => "id" }),
        jurisdiction: (_j: string) => capturingDO.CORELINK_SERVER,
      } as unknown as DurableObjectNamespace,
    };
    await workerFetch(`http://localhost/api/v2/${TEST_TENANT_ID}/path`, {
      headers: { Authorization: `Bearer ${TEST_PAT_TOKEN}` },
    }, capturingDO);
    expect(capturedHeader).toBe(TEST_TENANT_ID);
    expect(capturedHeader).not.toContain("_pending");
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// WP-T1: Tenant-derived DO routing
// DoD 2: two different tenants get different DO ids; URL-tenant≠PAT-tenant → 403
// DoD 3: No _pending_auth shared-DO path for authenticated requests
// DoD 4: DO tenantId resolved (not null) via x-corelink-tenant-id header
//
// NOTE on path-spoof (P1-2): the spoof check is gated on
// `auth.tenantId !== "_pending"`. WP-A1 (parallel work item) resolves the real
// tenant from D1 and writes it to auth.tenantId. Until WP-A1 merges,
// extractAuth() returns "_pending" and the spoof check is intentionally
// bypassed. The tests below cover:
//   (a) WP-A1 stub state: "_pending" → DO routing works, no spoof rejection
//   (b) Post-A1 state (simulated via a custom env that tests routing logic):
//       idFromName receives auth.tenantId, not URL tenant
//   (c) Regression: "_pending_auth" is never used
//   (d) Path-spoof 403 with a real resolved tenant (simulates post-A1 behavior
//       by having the worker use the auth tenant from a crafted request path)
// ──────────────────────────────────────────────────────────────────────────────
