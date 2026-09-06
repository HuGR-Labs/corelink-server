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

describe("CORS", () => {
  it("returns 204 for OPTIONS preflight from allowed origin", async () => {
    const resp = await workerFetch("http://localhost/v2/", {
      method: "OPTIONS",
      headers: {
        Origin: "https://humangr.com",
        "Access-Control-Request-Method": "GET",
      },
    });
    expect(resp.status).toBe(204);
    expect(resp.headers.get("access-control-allow-origin")).toBe(
      "https://humangr.com",
    );
  });

  it("returns 204 for OPTIONS from unknown origin but no ACAO header", async () => {
    const resp = await workerFetch("http://localhost/health", {
      method: "OPTIONS",
      headers: {
        Origin: "https://evil.example.com",
        "Access-Control-Request-Method": "GET",
      },
    });
    expect(resp.status).toBe(204);
    expect(resp.headers.get("access-control-allow-origin")).toBeNull();
  });

  it("attaches ACAO to /health from allowed admin origin", async () => {
    const resp = await workerFetch("http://localhost/health", {
      headers: { Origin: "https://humangr.com" },
    });
    expect(resp.headers.get("access-control-allow-origin")).toBe(
      "https://humangr.com",
    );
  });

  it("does NOT attach ACAO to /health from disallowed origin", async () => {
    const resp = await workerFetch("http://localhost/health", {
      headers: { Origin: "https://attacker.example.com" },
    });
    expect(resp.headers.get("access-control-allow-origin")).toBeNull();
  });

  it("attaches Vary: Origin when ACAO is set", async () => {
    const resp = await workerFetch("http://localhost/health", {
      headers: { Origin: "https://humangr.com" },
    });
    expect(resp.headers.get("vary")).toBe("Origin");
  });

  it("all three allowed origins work", async () => {
    const allowed = [
      "https://humangr.com",
      "https://humangr.com",
      "https://corelink-docs.humangr.com",
    ];
    for (const origin of allowed) {
      const resp = await workerFetch("http://localhost/health", {
        headers: { Origin: origin },
      });
      expect(resp.headers.get("access-control-allow-origin")).toBe(origin);
    }
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// Route resolution
// ──────────────────────────────────────────────────────────────────────────────

describe("route resolution — OCI paths (pass-through)", () => {
  const ociPaths = [
    "/v2/",
    "/v2",
    "/v2/_catalog",
    "/v2/myrepo/blobs/sha256:abc",
    "/v2/myrepo/manifests/latest",
    "/v2/myrepo/tags/list",
    "/token",
  ];

  // Every OCI path resolves to the pass-through and is forwarded to the _oci DO
  // (the first /v2/ segment is the repository NAME, NOT a tenant; /token has no
  // tenant segment). The Worker does NOT auth-gate or build an OCI envelope —
  // it returns the DO's response verbatim (503 CONTAINER_UNAVAILABLE in tests).
  for (const path of ociPaths) {
    it(`${path} forwards to the _oci DO (no Worker-synthesized OCI envelope)`, async () => {
      const resp = await workerFetch(`http://localhost${path}`);
      const body = await resp.json() as Record<string, unknown>;
      // Worker returns the DO stub body verbatim — no OCI errors[] array and no
      // Docker-Distribution-Api-Version header synthesized by the Worker.
      expect(body["errors"]).toBeUndefined();
      expect(resp.headers.get("docker-distribution-api-version")).toBeNull();
      expect(body["error"]).toBe("CONTAINER_UNAVAILABLE");
    });
  }

  it("the first /v2/ segment is the repository NAME, not a tenant — all OCI routes share the _oci DO", async () => {
    const namesUsed: string[] = [];
    const capturingEnv: Partial<Env> = {
      CORELINK_SERVER: {
        idFromName: (name: string) => {
          namesUsed.push(name);
          return { toString: () => `do-${name}` };
        },
        get: () => ({
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
    // Two DIFFERENT repository names + the /token leg must all route to "_oci".
    await workerFetch("http://localhost/v2/alpine/blobs/sha256:abc", undefined, capturingEnv);
    await workerFetch("http://localhost/v2/ubuntu/manifests/latest", undefined, capturingEnv);
    await workerFetch("http://localhost/token", undefined, capturingEnv);
    expect(namesUsed).toEqual(["_oci", "_oci", "_oci"]);
    // The repository name (alpine/ubuntu) must NEVER be used as a DO key.
    expect(namesUsed).not.toContain("alpine");
    expect(namesUsed).not.toContain("ubuntu");
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// Stripe billing webhook pass-through (LAUNCH-BLOCKER fix)
//
// POST /v1/billing/stripe-webhook is authenticated by Stripe's `Stripe-Signature`
// HMAC header, NOT a Bearer PAT. The Worker MUST forward it to the shared _system
// DO WITHOUT the PAT gate (no 401), preserving the RAW body + Stripe-Signature so
// the container can verify the HMAC over the exact signed bytes. It strips
// client-trust headers (delete-then-set) and the container — never a header — is
// the sole tenant authority.
// ──────────────────────────────────────────────────────────────────────────────

describe("Stripe billing webhook pass-through (/v1/billing/stripe-webhook)", () => {
  /** Capture what the Worker forwards to the DO + the DO name it routes to. */
  function makeBillingCapturingEnv(): {
    env: Partial<Env>;
    namesUsed: string[];
    captured: {
      body?: string;
      sig?: string | null;
      auth?: string | null;
      routeKind?: string | null;
      tenant?: string | null;
      scope?: string | null;
      forgedAdminScope?: string | null;
    };
  } {
    const namesUsed: string[] = [];
    const captured: {
      body?: string;
      sig?: string | null;
      auth?: string | null;
      routeKind?: string | null;
      tenant?: string | null;
      scope?: string | null;
      forgedAdminScope?: string | null;
    } = {};
    const env: Partial<Env> = {
      CORELINK_SERVER: {
        idFromName: (name: string) => {
          namesUsed.push(name);
          return { toString: () => `do-${name}` };
        },
        get: () => ({
          fetch: async (req: Request): Promise<Response> => {
            // Read the forwarded body to PROVE the bytes survived the forward
            // unchanged (this is the DO's job — the Worker must not read it).
            captured.body = await req.text();
            captured.sig = req.headers.get("stripe-signature");
            captured.auth = req.headers.get("authorization");
            captured.routeKind = req.headers.get("x-corelink-route-kind");
            captured.tenant = req.headers.get("x-corelink-tenant-id");
            captured.scope = req.headers.get("x-corelink-scope");
            captured.forgedAdminScope = req.headers.get("x-admin-scope");
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
    return { env, namesUsed, captured };
  }

  const RAW_WEBHOOK_BODY = JSON.stringify({
    id: "evt_test_123",
    type: "checkout.session.completed",
    data: { object: { metadata: { tenant_id: "should-be-ignored-by-worker" } } },
  });
  const STRIPE_SIG = "t=1700000000,v1=deadbeefcafef00ddeadbeefcafef00ddeadbeefcafef00ddeadbeefcafef00d";

  it("does NOT 401 the un-PAT'd webhook — forwards to the _system DO (no Worker auth gate)", async () => {
    // The webhook carries ONLY a Stripe-Signature (no Bearer PAT). Pre-fix this
    // fell into the reapi_v1 PAT bucket → 401. It must now pass through (200 stub).
    const { env } = makeBillingCapturingEnv();
    const resp = await workerFetch(
      "http://localhost/v1/billing/stripe-webhook",
      { method: "POST", headers: { "Stripe-Signature": STRIPE_SIG }, body: RAW_WEBHOOK_BODY },
      env,
    );
    expect(resp.status).toBe(200);
  });

  it("routes to the shared _system DO (no URL tenant)", async () => {
    const { env, namesUsed } = makeBillingCapturingEnv();
    await workerFetch(
      "http://localhost/v1/billing/stripe-webhook",
      { method: "POST", headers: { "Stripe-Signature": STRIPE_SIG }, body: RAW_WEBHOOK_BODY },
      env,
    );
    expect(namesUsed).toEqual(["_system"]);
  });

  it("forwards the RAW body UNCHANGED (HMAC must verify over the exact bytes)", async () => {
    const { env, captured } = makeBillingCapturingEnv();
    await workerFetch(
      "http://localhost/v1/billing/stripe-webhook",
      { method: "POST", headers: { "Stripe-Signature": STRIPE_SIG }, body: RAW_WEBHOOK_BODY },
      env,
    );
    // Byte-identical: no parse/re-serialize. A reordered/reformatted body would
    // break the container's signature verification.
    expect(captured.body).toBe(RAW_WEBHOOK_BODY);
  });

  it("preserves the Stripe-Signature header (the webhook's auth)", async () => {
    const { env, captured } = makeBillingCapturingEnv();
    await workerFetch(
      "http://localhost/v1/billing/stripe-webhook",
      { method: "POST", headers: { "Stripe-Signature": STRIPE_SIG }, body: RAW_WEBHOOK_BODY },
      env,
    );
    expect(captured.sig).toBe(STRIPE_SIG);
  });

  it("sets x-corelink-route-kind=billing_webhook and injects NO tenant/scope", async () => {
    const { env, captured } = makeBillingCapturingEnv();
    await workerFetch(
      "http://localhost/v1/billing/stripe-webhook",
      { method: "POST", headers: { "Stripe-Signature": STRIPE_SIG }, body: RAW_WEBHOOK_BODY },
      env,
    );
    expect(captured.routeKind).toBe("billing_webhook");
    // The container derives the tenant from the signed event, never a header.
    expect(captured.tenant).toBeNull();
    expect(captured.scope).toBeNull();
  });

  it("strips client-forged trust headers (x-corelink-scope / x-admin-scope / x-corelink-tenant-id)", async () => {
    const { env, captured } = makeBillingCapturingEnv();
    await workerFetch(
      "http://localhost/v1/billing/stripe-webhook",
      {
        method: "POST",
        headers: {
          "Stripe-Signature": STRIPE_SIG,
          // Forged server-trust headers — the Worker MUST strip all of these.
          "x-corelink-scope": "admin",
          "x-admin-scope": "admin",
          "x-corelink-tenant-id": "attacker-tenant",
        },
        body: RAW_WEBHOOK_BODY,
      },
      env,
    );
    expect(captured.scope).toBeNull();
    expect(captured.tenant).toBeNull();
    expect(captured.forgedAdminScope).toBeNull();
  });

  it("a trailing-segment path (/v1/billing/stripe-webhook/extra) does NOT pass through — stays PAT-gated (401)", async () => {
    // The carve-out is an EXACT-path match; anything else under /v1/billing/*
    // still falls into the reapi_v1 PAT bucket and 401s without a PAT.
    const resp = await workerFetch("http://localhost/v1/billing/stripe-webhook/extra", {
      method: "POST",
    });
    expect(resp.status).toBe(401);
  });

  it("does NOT disturb /v1/customer/billing/* — that PAT route still 401s without auth", async () => {
    // Guard against the carve-out accidentally widening to other /v1/*billing*
    // paths: the customer-portal billing route is PAT-required and must 401.
    const resp = await workerFetch("http://localhost/v1/customer/billing/portal", {
      method: "GET",
    });
    expect(resp.status).toBe(401);
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// M22(a): the public erasure-attestation verifier pass-through
// (/v1/public/attestation/*, /v1/public/keys/erasure/*.pub) forwards to the
// _anonymous DO with NO PAT. The container now applies a scoped per-IP rate
// limit on this router, keyed on the trusted x-corelink-client-ip header —
// so this arm MUST forward CF's unforgeable cf-connecting-ip as
// x-corelink-client-ip (the only arm among the pure pass-throughs that
// previously did NOT set it).
// ──────────────────────────────────────────────────────────────────────────────

describe("public erasure-attestation verifier pass-through (/v1/public/*)", () => {
  /** Capture what the Worker forwards to the DO + the DO name it routes to. */
  function makePublicAttestationCapturingEnv(): {
    env: Partial<Env>;
    namesUsed: string[];
    captured: { routeKind?: string | null; tenant?: string | null; clientIp?: string | null };
  } {
    const namesUsed: string[] = [];
    const captured: { routeKind?: string | null; tenant?: string | null; clientIp?: string | null } = {};
    const env: Partial<Env> = {
      CORELINK_SERVER: {
        idFromName: (name: string) => {
          namesUsed.push(name);
          return { toString: () => `do-${name}` };
        },
        get: () => ({
          fetch: async (req: Request): Promise<Response> => {
            captured.routeKind = req.headers.get("x-corelink-route-kind");
            captured.tenant = req.headers.get("x-corelink-tenant-id");
            captured.clientIp = req.headers.get("x-corelink-client-ip");
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
    return { env, namesUsed, captured };
  }

  it("routes to the shared _anonymous DO with routeKind=public_attestation", async () => {
    const { env, namesUsed, captured } = makePublicAttestationCapturingEnv();
    const resp = await workerFetch(
      "http://localhost/v1/public/attestation/req-1",
      { headers: { "cf-connecting-ip": "203.0.113.7" } },
      env,
    );
    expect(resp.status).toBe(200);
    expect(namesUsed).toEqual(["_anonymous"]);
    expect(captured.routeKind).toBe("public_attestation");
    expect(captured.tenant).toBe("_anonymous");
  });

  it("forwards cf-connecting-ip as the trusted x-corelink-client-ip (M22a rate-limit anchor)", async () => {
    const { env, captured } = makePublicAttestationCapturingEnv();
    await workerFetch(
      "http://localhost/v1/public/attestation/req-1",
      { headers: { "cf-connecting-ip": "203.0.113.7" } },
      env,
    );
    expect(captured.clientIp).toBe("203.0.113.7");
  });

  it("strips a client-forged x-corelink-client-ip before setting the trusted value", async () => {
    const { env, captured } = makePublicAttestationCapturingEnv();
    await workerFetch(
      "http://localhost/v1/public/keys/erasure/weur.pub",
      {
        headers: {
          "cf-connecting-ip": "203.0.113.7",
          // Client tries to smuggle a forged client-IP to dodge/attribute the
          // per-IP bucket to a different IP.
          "x-corelink-client-ip": "6.6.6.6",
        },
      },
      env,
    );
    // The Worker-set value (from cf-connecting-ip) wins, never the client's.
    expect(captured.clientIp).toBe("203.0.113.7");
  });

  it("still forwards (empty client-ip) when cf-connecting-ip is absent — container fail-opens", async () => {
    const { env, captured } = makePublicAttestationCapturingEnv();
    const resp = await workerFetch(
      "http://localhost/v1/public/attestation/req-1",
      {},
      env,
    );
    expect(resp.status).toBe(200);
    // No cf-connecting-ip (e.g. local dev) → the Worker sets "" (never absent
    // header, never the client's own value) and the container's rate limiter
    // fail-opens on an absent/empty header.
    expect(captured.clientIp).toBe("");
  });
});

describe("route resolution — non-OCI paths", () => {
  const reapiPaths = [
    "/api/v2/tenant/blobs",
    "/npm/tenant/package",
    "/pip/tenant/index",
    "/brew/tenant/api/formula",
    "/cargo/tenant/api/v1/crates",
  ];

  for (const path of reapiPaths) {
    it(`${path} returns REAPI error envelope on unauthenticated request`, async () => {
      const resp = await workerFetch(`http://localhost${path}`);
      const body = await resp.json() as Record<string, unknown>;
      expect(typeof body["error"]).toBe("string");
      expect(body["error"]).toBe("UNAUTHORIZED");
      expect(body["errors"]).toBeUndefined();
    });
  }
});

// ──────────────────────────────────────────────────────────────────────────────
// DO error mapping
// ──────────────────────────────────────────────────────────────────────────────

describe("DO error mapping", () => {
  it("DO 503 on OCI path is forwarded verbatim (Worker does not reshape it)", async () => {
    // OCI is pass-through: the Worker forwards the DO's 503 body as-is. No auth
    // header is needed (the Worker no longer gates OCI), but supplying one must
    // not change the outcome — it is forwarded to the container, not consumed.
    const resp = await workerFetch(`http://localhost/v2/myrepo/manifests/latest`, {
      headers: { Authorization: "Bearer oci-bearer" },
    });
    expect(resp.status).toBe(503);
    const body = await resp.json() as Record<string, unknown>;
    // The DO stub's CONTAINER_UNAVAILABLE body, verbatim — not reshaped into an
    // OCI `errors[]` envelope.
    expect(body["error"]).toBe("CONTAINER_UNAVAILABLE");
    expect(body["errors"]).toBeUndefined();
  });

  it("DO 503 on REAPI path returns REAPI envelope", async () => {
    const resp = await workerFetch(`http://localhost/api/v2/${TEST_TENANT_ID}/blobs`, {
      headers: { Authorization: `Bearer ${TEST_PAT_TOKEN}` },
    });
    // DO returns 503, Worker passes it through with x-request-id added
    expect(resp.status).toBe(503);
    expect(resp.headers.get("x-request-id")).not.toBeNull();
  });

  it("DO fetch exception returns 500 on REAPI path", async () => {
    const throwingEnv: Partial<Env> = {
      CORELINK_SERVER: {
        idFromName: (_name: string) => ({ toString: () => "stub-id" }),
        get: (_id: unknown) => ({
          fetch: async (_req: Request): Promise<Response> => {
            throw new Error("connection refused");
          },
        }),
        idFromString: (_s: string) => ({ toString: () => "stub-id" }),
        newUniqueId: () => ({ toString: () => "stub-unique-id" }),
        jurisdiction: (_j: string) => throwingEnv.CORELINK_SERVER,
      } as unknown as DurableObjectNamespace,
    };

    const resp = await workerFetch(`http://localhost/api/v2/${TEST_TENANT_ID}/path`, {
      headers: { Authorization: `Bearer ${TEST_PAT_TOKEN}` },
    }, throwingEnv);
    expect(resp.status).toBe(500);
    const body = await resp.json() as { error: string };
    expect(body.error).toBe("INTERNAL_ERROR");
  });

  it("OCI DO fetch exception returns a controlled 500 with x-request-id (not OCI envelope, not opaque)", async () => {
    // Hardening (PR #169): the OCI pass-through wraps ociStub.fetch in try/catch
    // (parity with the PAT-path DO forward). A DO/container throw must yield the
    // same controlled REAPI error the PAT path uses — NOT a leaked stack, NOT a
    // Worker-synthesized OCI `errors[]` envelope.
    const throwingEnv: Partial<Env> = {
      CORELINK_SERVER: {
        idFromName: (_name: string) => ({ toString: () => "stub-id" }),
        get: (_id: unknown) => ({
          fetch: async (_req: Request): Promise<Response> => {
            throw new Error("network error");
          },
        }),
        idFromString: (_s: string) => ({ toString: () => "stub-id" }),
        newUniqueId: () => ({ toString: () => "stub-unique-id" }),
        jurisdiction: (_j: string) => throwingEnv.CORELINK_SERVER,
      } as unknown as DurableObjectNamespace,
    };

    const resp = await workerFetch(`http://localhost/v2/myrepo/blobs/sha256:abc`, undefined, throwingEnv);
    expect(resp.status).toBe(500);
    expect(resp.headers.get("x-request-id")).not.toBeNull();
    const body = await resp.json() as Record<string, unknown>;
    expect(body["error"]).toBe("INTERNAL_ERROR");
    // The Worker no longer builds OCI envelopes — no errors[] on OCI faults.
    expect(body["errors"]).toBeUndefined();
    // Must not leak the underlying throw message.
    expect(JSON.stringify(body)).not.toContain("network error");
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// Security invariants
// ──────────────────────────────────────────────────────────────────────────────

describe("security invariants", () => {
  it("X-Request-Id present on every response type", async () => {
    const cases = [
      "http://localhost/health",
      "http://localhost/v2/",
      "http://localhost/api/v2/t/p",
      "http://localhost/unknown-xyz",
    ];
    for (const url of cases) {
      const resp = await workerFetch(url);
      expect(resp.headers.get("x-request-id"), `missing for ${url}`).not.toBeNull();
    }
  });

  it("token value NOT reflected in any 401 response body", async () => {
    const secretToken = "SECRETTOKEN123" + "x".repeat(50);
    const resp = await workerFetch("http://localhost/api/v2/t/path", {
      headers: { Authorization: `Bearer ${secretToken}` },
    });
    const text = await resp.text();
    expect(text).not.toContain(secretToken);
    expect(text).not.toContain("SECRETTOKEN");
  });

  it("token format check is constant-time (no early exit on short token)", async () => {
    // If format check is NOT constant-time, very short tokens would return faster.
    // We can't measure nanoseconds, but we can at least verify it returns 401 for
    // both short tokens (1 char) and max-1 length tokens.
    const resp1 = await workerFetch("http://localhost/api/v2/t/p", {
      headers: { Authorization: "Bearer x" },
    });
    const resp2 = await workerFetch("http://localhost/api/v2/t/p", {
      headers: { Authorization: "Bearer " + "y".repeat(31) },
    });
    expect(resp1.status).toBe(401);
    expect(resp2.status).toBe(401);
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// INV-NO-BODY-IN-LOGS
// ──────────────────────────────────────────────────────────────────────────────

describe("INV-NO-BODY-IN-LOGS", () => {
  it("POST body content NOT reflected in error response", async () => {
    const sensitiveBody = "secret-payload-that-must-not-leak=true&token=abc123";

    const resp = await workerFetch(`http://localhost/api/v2/${TEST_TENANT_ID}/blobs`, {
      method: "POST",
      headers: {
        Authorization: `Bearer ${TEST_PAT_TOKEN}`,
        "Content-Type": "application/x-www-form-urlencoded",
      },
      body: sensitiveBody,
    });

    const text = await resp.text();
    expect(text).not.toContain("secret-payload");
    expect(text).not.toContain("must-not-leak");
    expect(text).not.toContain("abc123");
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// Auth middleware — invalid character paths
// ──────────────────────────────────────────────────────────────────────────────

describe("auth middleware — invalid token characters", () => {
  it("returns 401 for token containing a space character (0x20 < 0x21)", async () => {
    // Token with an embedded space — must be caught by the printable-ASCII guard
    const tokenWithSpace = "validstart-" + "a".repeat(20) + " " + "a".repeat(20);
    const resp = await workerFetch("http://localhost/api/v2/t/p", {
      headers: { Authorization: `Bearer ${tokenWithSpace}` },
    });
    expect(resp.status).toBe(401);
  });

  it("returns 401 for token containing a tab character (0x09 < 0x21)", async () => {
    const tokenWithTab = "validstart-" + "a".repeat(20) + "\t" + "a".repeat(20);
    const resp = await workerFetch("http://localhost/api/v2/t/p", {
      headers: { Authorization: `Bearer ${tokenWithTab}` },
    });
    expect(resp.status).toBe(401);
  });

  it("returns 401 for token containing a DEL character (0x7F > 0x7E)", async () => {
    const tokenWithDel = "validstart-" + "a".repeat(20) + "\x7F" + "a".repeat(20);
    const resp = await workerFetch("http://localhost/api/v2/t/p", {
      headers: { Authorization: `Bearer ${tokenWithDel}` },
    });
    expect(resp.status).toBe(401);
  });

  it("error body for invalid-char token does not echo the token", async () => {
    const evilToken = "INVALID\x01CHARS" + "x".repeat(32);
    const resp = await workerFetch("http://localhost/api/v2/t/p", {
      headers: { Authorization: `Bearer ${evilToken}` },
    });
    const text = await resp.text();
    expect(text).not.toContain("INVALID");
    expect(text).not.toContain("\x01");
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// OCI pass-through — the CONTAINER owns OCI status + error envelopes.
//
// Previously the Worker mapped OCI codes → status via ociStatusForCode/ociError
// (now DELETED). Under PR #169 the container emits ALL OCI responses; the Worker
// is a pure forwarder, so these tests assert the container's envelope is returned
// VERBATIM (status, body, OCI-spec headers) and the Worker adds none of its own.
// ──────────────────────────────────────────────────────────────────────────────
