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

describe("security (H4): forwarded-request trust-header hygiene", () => {
  /** Capture all headers the DO stub receives on the forwarded request. */
  function makeHeaderCapturingEnv(): {
    env: Partial<Env>;
    captured: { headers?: Headers };
  } {
    const captured: { headers?: Headers } = {};
    const env: Partial<Env> = {
      CORELINK_SERVER: {
        idFromName: (_n: string) => ({ toString: () => "stub-id" }),
        get: () => ({
          fetch: async (req: Request): Promise<Response> => {
            captured.headers = new Headers(req.headers);
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

  const SMUGGLED: Record<string, string> = {
    "x-admin-scope": "admin:*",
    "x-admin-principal": "attacker@evil.example.com",
    "x-admin-tenant": "victim-tenant",
    "x-corelink-internal-auth": "forged-internal-secret",
    "x-corelink-fanout-from": "prod",
  };

  it("MAIN data-plane forward strips ALL client trust headers", async () => {
    const { env, captured } = makeHeaderCapturingEnv();
    const resp = await workerFetch(
      `http://localhost/api/v2/${TEST_TENANT_ID}/path`,
      { headers: { Authorization: `Bearer ${TEST_PAT_TOKEN}`, ...SMUGGLED } },
      env,
    );
    expect(resp.status).toBe(200);
    expect(captured.headers).toBeDefined();
    for (const name of Object.keys(SMUGGLED)) {
      expect(captured.headers?.get(name), `${name} must be stripped`).toBeNull();
    }
  });

  it("MAIN data-plane forward: Worker-set trust headers survive the strip", async () => {
    const { env, captured } = makeHeaderCapturingEnv();
    await workerFetch(
      `http://localhost/api/v2/${TEST_TENANT_ID}/path`,
      {
        headers: {
          Authorization: `Bearer ${TEST_PAT_TOKEN}`,
          // Client also tries to spoof the Worker-established headers — these
          // are unconditionally overwritten by the Worker, not just deleted.
          "x-corelink-tenant-id": "spoofed-tenant",
          "x-corelink-route-kind": "spoofed-kind",
          "x-corelink-token-prefix": "spoofed-prefix",
          ...SMUGGLED,
        },
      },
      env,
    );
    // The Worker's own values win.
    expect(captured.headers?.get("x-corelink-tenant-id")).toBe(TEST_TENANT_ID);
    expect(captured.headers?.get("x-corelink-route-kind")).toBe("reapi_v2");
    expect(captured.headers?.get("x-corelink-token-prefix")).not.toBe("spoofed-prefix");
  });

  it("internal forward strips smuggled internal-auth and re-sets it from the server secret", async () => {
    const { env, captured } = makeHeaderCapturingEnv();
    const INTERNAL_KEY = "the-real-internal-secret-0123456789";
    const resp = await workerFetch(
      "http://localhost/_internal/pat/mint",
      {
        method: "POST",
        headers: {
          // Caller authenticates with the correct shared secret …
          "x-corelink-internal-auth": INTERNAL_KEY,
          // … but also tries to smuggle admin headers.
          "x-admin-scope": "admin:*",
          "x-admin-principal": "attacker",
          "x-admin-tenant": "victim",
          "x-corelink-fanout-from": "prod",
        },
        body: "{}",
      },
      { ...env, CORELINK_INTERNAL_AUTH_KEY: INTERNAL_KEY },
    );
    expect(resp.status).toBe(200);
    // Admin headers stripped …
    expect(captured.headers?.get("x-admin-scope")).toBeNull();
    expect(captured.headers?.get("x-admin-principal")).toBeNull();
    expect(captured.headers?.get("x-admin-tenant")).toBeNull();
    expect(captured.headers?.get("x-corelink-fanout-from")).toBeNull();
    // … internal-auth re-set from the server secret (delete-then-set).
    expect(captured.headers?.get("x-corelink-internal-auth")).toBe(INTERNAL_KEY);
  });

  // ── CAA-360 CRITICAL: DSR erase must sweep EVERY residency jurisdiction ──────
  const ERASE_KEY = "the-real-internal-secret-0123456789";
  const okRegion = () => ({
    fetch: async (): Promise<Response> => new Response("{}", { status: 200 }),
  });

  it("DSR erase ORIGIN fans out to EVERY regional worker + completes (200) when all confirm", async () => {
    const { env } = makeHeaderCapturingEnv();
    const called: string[] = [];
    const mk = (name: string) => ({
      fetch: async (req: Request): Promise<Response> => {
        called.push(name);
        // the fan-out marker MUST be set so the regional worker does not re-fan
        expect(req.headers.get("x-corelink-fanout-from")).toBe("iad");
        return new Response("{}", { status: 200 });
      },
    });
    const resp = await workerFetch(
      "http://localhost/_internal/dsr/erase",
      { method: "POST", headers: { "x-corelink-internal-auth": ERASE_KEY }, body: JSON.stringify({ dsr_id: "d1", tenant_id: "t1" }) },
      { ...env, CORELINK_INTERNAL_AUTH_KEY: ERASE_KEY, PROD_LHR: mk("lhr"), PROD_SAM: mk("sam"), PROD_NRT: mk("nrt"), PROD_SYD: mk("syd") },
    );
    expect(resp.status).toBe(200);
    expect(called.sort()).toEqual(["lhr", "nrt", "sam", "syd"]);
  });

  it("DSR erase FAILS CLOSED (502) when any region errors — no false VerifiedComplete", async () => {
    const { env } = makeHeaderCapturingEnv();
    const euFail = { fetch: async (): Promise<Response> => new Response("boom", { status: 500 }) };
    const resp = await workerFetch(
      "http://localhost/_internal/dsr/erase",
      { method: "POST", headers: { "x-corelink-internal-auth": ERASE_KEY }, body: "{}" },
      { ...env, CORELINK_INTERNAL_AUTH_KEY: ERASE_KEY, PROD_LHR: euFail, PROD_SAM: okRegion(), PROD_NRT: okRegion(), PROD_SYD: okRegion() },
    );
    expect(resp.status).toBe(502);
  });

  it("DSR erase FAILS CLOSED (502) when a regional binding is ABSENT (jurisdiction unprovable)", async () => {
    const { env } = makeHeaderCapturingEnv();
    const resp = await workerFetch(
      "http://localhost/_internal/dsr/erase",
      { method: "POST", headers: { "x-corelink-internal-auth": ERASE_KEY }, body: "{}" },
      // PROD_LHR (the EU jurisdiction) intentionally ABSENT → cannot prove EU erased
      { ...env, CORELINK_INTERNAL_AUTH_KEY: ERASE_KEY, PROD_SAM: okRegion(), PROD_NRT: okRegion(), PROD_SYD: okRegion() },
    );
    expect(resp.status).toBe(502);
  });

  it("a fanned-out DSR erase (x-corelink-fanout-from present) does NOT re-fan (loop guard)", async () => {
    const { env } = makeHeaderCapturingEnv();
    let regionCalled = false;
    const region = { fetch: async (): Promise<Response> => { regionCalled = true; return new Response("{}", { status: 200 }); } };
    const resp = await workerFetch(
      "http://localhost/_internal/dsr/erase",
      { method: "POST", headers: { "x-corelink-internal-auth": ERASE_KEY, "x-corelink-fanout-from": "iad" }, body: "{}" },
      { ...env, CORELINK_INTERNAL_AUTH_KEY: ERASE_KEY, PROD_LHR: region, PROD_SAM: region, PROD_NRT: region, PROD_SYD: region },
    );
    expect(resp.status).toBe(200);
    expect(regionCalled).toBe(false);
  });

  it("region-fanout forward strips smuggled trust headers but keeps Worker-set fanout-from", async () => {
    // Capture the headers the regional Worker (Service Binding) receives.
    let fanoutHeaders: Headers | undefined;
    const regionalBinding = {
      fetch: async (req: Request): Promise<Response> => {
        fanoutHeaders = new Headers(req.headers);
        return new Response("{}", { status: 200, headers: { "Content-Type": "application/json" } });
      },
    };
    // D1 mock: PAT row + tenant.primary_region = "weur" (EU MACRO) → maps to the
    // lhr colo → fans out via PROD_LHR (backlog #29: macro, NOT a colo literal).
    const d1 = {
      prepare: (sql: string) => ({
        bind: (...args: unknown[]) => ({
          first: async <T>() => {
            if (sql.includes("FROM pat")) {
              return { tenant_id: TEST_TENANT_ID, expires_ms: Date.now() + 3_600_000 } as T;
            }
            if (sql.includes("primary_region")) {
              return { primary_region: "weur" } as T;
            }
            // tenant_storage_state / tier lookups → null (no quota record).
            void args;
            return null as T;
          },
        }),
        first: async <T>() => null as T | null,
      }),
    } as unknown as D1Database;

    const resp = await workerFetch(
      `http://localhost/api/v2/${TEST_TENANT_ID}/path`,
      {
        headers: {
          Authorization: `Bearer ${TEST_PAT_TOKEN}`,
          "x-admin-scope": "admin:*",
          "x-corelink-internal-auth": "forged",
          "x-corelink-fanout-from": "spoofed-origin",
          // Client tries to smuggle a forged residency macro — must be stripped.
          "x-corelink-primary-region": "enam",
        },
      },
      { CONFIG_DB: d1, PROD_LHR: regionalBinding, CORELINK_INTERNAL_AUTH_KEY: "the-real-internal-secret-0123456789" },
    );
    expect(resp.status).toBe(200);
    expect(fanoutHeaders).toBeDefined();
    // Smuggled admin/internal-auth stripped.
    expect(fanoutHeaders?.get("x-admin-scope")).toBeNull();
    expect(fanoutHeaders?.get("x-corelink-internal-auth")).toBeNull();
    // fanout-from is Worker-established to the forgery-safe internal-auth secret (#349), NOT a guessable literal; the client's smuggled value is stripped + replaced.
    expect(fanoutHeaders?.get("x-corelink-fanout-from")).toBe("the-real-internal-secret-0123456789");
    // backlog #29: the Worker sets the trusted residency macro from D1
    // (weur), overwriting the client's smuggled "enam".
    expect(fanoutHeaders?.get("x-corelink-primary-region")).toBe("weur");
  });

  // backlog #29 — the core fix: a weur tenant routes to PROD_LHR; a missing
  // non-IAD binding (or a D1 throw) FAILS CLOSED with 503 (never the IAD leak).
  function d1ReturningRegion(region: string | null, throwIt = false): D1Database {
    return {
      prepare: (sql: string) => ({
        bind: () => ({
          first: async <T>() => {
            if (sql.includes("FROM pat")) {
              return { tenant_id: TEST_TENANT_ID, expires_ms: Date.now() + 3_600_000 } as T;
            }
            if (sql.includes("primary_region")) {
              if (throwIt) throw new Error("D1 transient");
              return (region === null ? null : { primary_region: region }) as T;
            }
            return null as T;
          },
        }),
        first: async <T>() => null as T | null,
      }),
    } as unknown as D1Database;
  }

  // NOTE on harness: the PAT-authenticated forward path of this suite requires
  // PAT_SIGNING_KEY + a valid HMAC PAT, which the raw `vitest run` harness does
  // not provide (extractAuth fails closed → 503); the positive fan-out path is
  // covered by the "region-fanout forward …" test above (weur→PROD_LHR), which
  // runs green in CI/wrangler. The two assertions below test the LEAK-PREVENTION
  // property that is harness-INDEPENDENT: a non-IAD-resident tenant whose
  // residency cannot be honoured is NEVER served by the local IAD DO stub —
  // it must 503, never the IAD-DO "CONTAINER_UNAVAILABLE" leak path.

  it("weur tenant with PROD_LHR binding MISSING is never served from IAD (no leak)", async () => {
    let iadDoHit = false;
    const namespace = {
      idFromName: () => ({ toString: () => "iad" }),
      get: () => ({
        fetch: async () => {
          iadDoHit = true;
          return new Response("iad-do", { status: 200 });
        },
      }),
    } as unknown as Env["CORELINK_SERVER"];
    const resp = await workerFetch(
      `http://localhost/api/v2/${TEST_TENANT_ID}/path`,
      { headers: { Authorization: `Bearer ${TEST_PAT_TOKEN}` } },
      // No PROD_LHR binding → must FAIL CLOSED, never fall through to the IAD DO.
      { CONFIG_DB: d1ReturningRegion("weur"), CORELINK_SERVER: namespace },
    );
    // The IAD DO must NOT have served this EU tenant (the cross-border leak).
    expect(iadDoHit).toBe(false);
    expect(resp.status).toBe(503);
  });

});

// ──────────────────────────────────────────────────────────────────────────────
// H1: PAT scope is a server-trusted signal — read from D1, forwarded to the DO
// as x-corelink-scope, and NEVER forgeable by the client.
// ──────────────────────────────────────────────────────────────────────────────
