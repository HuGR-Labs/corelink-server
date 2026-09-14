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

describe("H1: x-corelink-scope server-trust header", () => {
  /** Build a capturing-DO env that records the headers the DO receives. */
  function makeCapturingEnv(record: (h: Headers) => void): Partial<Env> {
    const ns = {
      idFromName: (_n: string) => ({ toString: () => "id" }),
      get: () => ({
        fetch: async (req: Request) => {
          record(new Headers(req.headers));
          return new Response("{}", {
            status: 200,
            headers: { "Content-Type": "application/json" },
          });
        },
      }),
      idFromString: (_s: string) => ({ toString: () => "id" }),
      newUniqueId: () => ({ toString: () => "id" }),
      jurisdiction: (_j: string) => ns,
    } as unknown as DurableObjectNamespace;
    return { CORELINK_SERVER: ns };
  }

  it("(a) forwards the D1-resolved scope verbatim as x-corelink-scope", async () => {
    let seen: Headers | undefined;
    const env: Partial<Env> = {
      ...makeCapturingEnv((h) => { seen = h; }),
      CONFIG_DB: makeD1Mock(new Map([
        [TEST_TOKEN_ID, {
          tenant_id: TEST_TENANT_ID,
          expires_ms: Date.now() + 3_600_000,
          scope: "cas:rw",
        }],
      ])),
    };
    const resp = await workerFetch(
      `http://localhost/api/v2/${TEST_TENANT_ID}/path`,
      { headers: { Authorization: `Bearer ${TEST_PAT_TOKEN}` } },
      env,
    );
    expect(resp.status).toBe(200);
    expect(seen?.get("x-corelink-scope")).toBe("cas:rw");
  });

  it("(b) a client-supplied x-corelink-scope is STRIPPED — never forwarded as the client value", async () => {
    let seen: Headers | undefined;
    const env: Partial<Env> = {
      ...makeCapturingEnv((h) => { seen = h; }),
      CONFIG_DB: makeD1Mock(new Map([
        [TEST_TOKEN_ID, {
          tenant_id: TEST_TENANT_ID,
          expires_ms: Date.now() + 3_600_000,
          scope: "cas:rw",
        }],
      ])),
    };
    const resp = await workerFetch(
      `http://localhost/api/v2/${TEST_TENANT_ID}/path`,
      {
        headers: {
          Authorization: `Bearer ${TEST_PAT_TOKEN}`,
          // Client attempts to smuggle an escalated scope.
          "x-corelink-scope": "admin",
        },
      },
      env,
    );
    expect(resp.status).toBe(200);
    // The forged "admin" must be gone — replaced by the D1-resolved value only.
    expect(seen?.get("x-corelink-scope")).toBe("cas:rw");
    expect(seen?.get("x-corelink-scope")).not.toBe("admin");
  });

  it("(c) a missing/null D1 scope is forwarded as the empty string", async () => {
    let seen: Headers | undefined;
    const env: Partial<Env> = {
      ...makeCapturingEnv((h) => { seen = h; }),
      // Row OMITS scope (older row, NULL in D1) → normalised to "".
      CONFIG_DB: makeD1Mock(new Map([
        [TEST_TOKEN_ID, {
          tenant_id: TEST_TENANT_ID,
          expires_ms: Date.now() + 3_600_000,
          scope: null,
        }],
      ])),
    };
    const resp = await workerFetch(
      `http://localhost/api/v2/${TEST_TENANT_ID}/path`,
      { headers: { Authorization: `Bearer ${TEST_PAT_TOKEN}` } },
      env,
    );
    expect(resp.status).toBe(200);
    // Header is present and explicitly empty (not absent, not the literal "null").
    expect(seen?.get("x-corelink-scope")).toBe("");
  });

  it("(c') a row with scope entirely absent (older fixture) is forwarded as the empty string", async () => {
    let seen: Headers | undefined;
    const env: Partial<Env> = {
      ...makeCapturingEnv((h) => { seen = h; }),
      // Default fixture map: no `scope` key at all.
      CONFIG_DB: makeD1Mock(new Map([
        [TEST_TOKEN_ID, {
          tenant_id: TEST_TENANT_ID,
          expires_ms: Date.now() + 3_600_000,
        }],
      ])),
    };
    const resp = await workerFetch(
      `http://localhost/api/v2/${TEST_TENANT_ID}/path`,
      { headers: { Authorization: `Bearer ${TEST_PAT_TOKEN}` } },
      env,
    );
    expect(resp.status).toBe(200);
    expect(seen?.get("x-corelink-scope")).toBe("");
  });

  it("region-fanout forward also carries the D1-resolved x-corelink-scope (and strips client value)", async () => {
    let fanoutHeaders: Headers | undefined;
    const regionalBinding = {
      fetch: async (req: Request): Promise<Response> => {
        fanoutHeaders = new Headers(req.headers);
        return new Response("{}", { status: 200, headers: { "Content-Type": "application/json" } });
      },
    };
    const d1 = {
      prepare: (sql: string) => ({
        bind: (...args: unknown[]) => ({
          first: async <T>() => {
            if (sql.includes("FROM pat")) {
              return {
                tenant_id: TEST_TENANT_ID,
                expires_ms: Date.now() + 3_600_000,
                scope: "cas:rw",
              } as T;
            }
            if (sql.includes("primary_region")) {
              // D1 holds the MACRO code (wnam/enam/weur/sam/...), NOT a colo
              // string. The worker maps the macro → colo via region-map
              // (coloForMacro): weur → lhr → env.PROD_LHR. A literal "lhr"
              // here would NOT match any macro and fail-close (RESIDENCY_UNAVAILABLE).
              return { primary_region: "weur" } as T;
            }
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
          "x-corelink-scope": "admin", // smuggle attempt on the fanout path
        },
      },
      {
        CONFIG_DB: d1,
        PROD_LHR: regionalBinding,
        // This case verifies authenticated residency fan-out/header trust. Keep
        // the orthogonal monthly quota gate out of the fixture so a missing or
        // partial quota mock cannot turn the forwarding assertion into a 429.
        // Production keeps the default fail-closed quota behavior unchanged.
        REQUEST_QUOTA_DISABLED: "true",
      },
    );
    expect(resp.status).toBe(200);
    expect(fanoutHeaders?.get("x-corelink-scope")).toBe("cas:rw");
    expect(fanoutHeaders?.get("x-corelink-scope")).not.toBe("admin");
  });
});
