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

import { describe, it, expect, vi } from "vitest";
import type { D1Database } from "@cloudflare/workers-types";
import workerHandler from "../src/index.js";
import type { Env } from "../src/index.js";
import {
  TEST_PAT_SIGNING_KEY,
  TEST_PAT_TOKEN_ID,
  mintTestPat,
} from "./setup.js";

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
    batch: async () => [],
    exec: async () => ({ count: 0, duration: 0 }),
    withSession: () => null as never,
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
      headers: { Origin: "https://corelink-admin.humangr.com" },
    });
    expect(resp.headers.get("access-control-allow-origin")).toBe(
      "https://corelink-admin.humangr.com",
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
      "https://corelink-admin.humangr.com",
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

  it("D1 throw while resolving residency never serves from IAD (fails closed)", async () => {
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
      { CONFIG_DB: d1ReturningRegion(null, true), CORELINK_SERVER: namespace },
    );
    expect(iadDoHit).toBe(false);
    expect(resp.status).toBe(503);
  });

  // F1: client-supplied trust headers (internal-auth, admin-scope, the forgeable
  // x-forwarded-for) MUST be stripped on the forwarded request, while the
  // Worker-set x-corelink-client-ip (sourced from the unforgeable cf-connecting-ip)
  // survives. This proves the denylist + strip actually fire AND that the Worker
  // overwrites the client IP from CF's trusted header — deleting either the strip
  // or the x-corelink-client-ip set must fail this test.
  it("MAIN forward strips client trust headers (incl. x-forwarded-for) and sets x-corelink-client-ip from cf-connecting-ip", async () => {
    const { env, captured } = makeHeaderCapturingEnv();
    const resp = await workerFetch(
      `http://localhost/api/v2/${TEST_TENANT_ID}/path`,
      {
        headers: {
          Authorization: `Bearer ${TEST_PAT_TOKEN}`,
          // Client tries to smuggle internal-auth + admin-scope …
          "x-corelink-internal-auth": "forged-internal-secret",
          "x-admin-scope": "admin:*",
          // … and a forged client IP via the (now denylisted) XFF …
          "x-forwarded-for": "6.6.6.6",
          // … and tries to pre-set the server-only x-corelink-client-ip header.
          "x-corelink-client-ip": "6.6.6.6",
          // CF edge sets the unforgeable real client IP here.
          "cf-connecting-ip": "203.0.113.7",
        },
      },
      env,
    );
    expect(resp.status).toBe(200);
    expect(captured.headers).toBeDefined();
    // Client-supplied trust headers are stripped.
    expect(captured.headers?.get("x-corelink-internal-auth")).toBeNull();
    expect(captured.headers?.get("x-admin-scope")).toBeNull();
    // The forgeable x-forwarded-for is stripped (no longer trusted for rate-limit).
    expect(captured.headers?.get("x-forwarded-for")).toBeNull();
    // The Worker-set client IP wins: sourced from cf-connecting-ip, NOT the
    // client's forged 6.6.6.6 value.
    expect(captured.headers?.get("x-corelink-client-ip")).toBe("203.0.113.7");
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// H1: PAT scope is a server-trusted signal — read from D1, forwarded to the DO
// as x-corelink-scope, and NEVER forgeable by the client.
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
      { CONFIG_DB: d1, PROD_LHR: regionalBinding },
    );
    expect(resp.status).toBe(200);
    expect(fanoutHeaders?.get("x-corelink-scope")).toBe("cas:rw");
    expect(fanoutHeaders?.get("x-corelink-scope")).not.toBe("admin");
  });
});
