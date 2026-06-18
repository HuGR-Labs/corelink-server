/**
 * Unit tests for the /v1/customer/* Clerk session bridge (dashboard revival
 * WP-1) — the dual-auth dispatch in worker/src/index.ts plus the shared
 * verification helper worker/src/lib/clerk_auth.ts.
 *
 * Dispatch contract under test:
 *   - A bearer that parses as a canonical CoreLink PAT takes the EXISTING
 *     generic PAT gate, byte-identical (regression: valid PAT forwards;
 *     revoked PAT — migration 0063 — still 401s and NEVER enters the Clerk
 *     pipeline).
 *   - Anything else (Clerk JWT, garbage, missing header) takes the Clerk
 *     pipeline: verifyToken + M1 azp re-assert + M2 issuer check + tenant
 *     lookup by clerk_user_id; then a least-privilege forward to the
 *     per-tenant DO (NO authorization header, NO x-corelink-internal-auth).
 *
 * Mirrors onboarding.test.ts patterns (@clerk/backend mocked per test; the DO
 * stub captures the forwarded request) + index.test.ts PAT fixtures (canonical
 * 96-char PAT shape; D1 mock honoring the `revoked_at_ms IS NULL` predicate).
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import type { D1Database, DurableObjectNamespace } from "@cloudflare/workers-types";

// Mock the Clerk edge verifier (hoisted) — controlled per test.
vi.mock("@clerk/backend", () => ({ verifyToken: vi.fn() }));
import { verifyToken } from "@clerk/backend";
import workerHandler from "../src/index.js";
import type { Env } from "../src/index.js";
import { TEST_PAT_SIGNING_KEY, mintTestPat } from "./setup.js";

const mockVerifyToken = vi.mocked(verifyToken);

const CLERK_SECRET = "sk_test_clerk_secret";
const INTERNAL_KEY = "test-internal-auth-key-0123456789";

// Canonical test PAT — CRYPTOGRAPHICALLY VALID under TEST_PAT_SIGNING_KEY
// (honest auth harness, #345). The native plane (extractAuth) fails CLOSED
// with a 503 unless PAT_SIGNING_KEY is bound AND the PAT's HMAC verifies, so
// PAT-surface regression tests must mint a real signed PAT and bind the key
// (makeBridgeEnv does the latter). The earlier inline all-"A" fixture relied
// on the now-removed "PAT_SIGNING_KEY absent ⇒ skip HMAC" theater and 503'd.
const TEST_TOKEN_ID = "AAAAAAAAAAAAAAAA"; // 16 Crockford b32 chars
const TEST_PAT_TOKEN = await mintTestPat({ tokenId: TEST_TOKEN_ID });
if (TEST_PAT_TOKEN.length !== 96) {
  throw new Error(`TEST_PAT_TOKEN length ${TEST_PAT_TOKEN.length} !== 96`);
}
const REVOKED_TOKEN_ID = "DDDDDDDDDDDDDDDD";
const REVOKED_PAT_TOKEN = await mintTestPat({ tokenId: REVOKED_TOKEN_ID });

const TEST_TENANT_ID = "00000000-0000-0000-0000-000000000001";
const CLERK_TENANT_ID = "00000000-0000-0000-0000-00000000c1e7";

function makeCtx(): ExecutionContext {
  return {
    waitUntil: (_p: Promise<unknown>) => {},
    passThroughOnException: () => {},
  } as unknown as ExecutionContext;
}

/**
 * Combined D1 mock — dispatches on the SQL string:
 *   - `FROM pat` lookups resolve by token_id, honoring the soft-revocation
 *     predicate `revoked_at_ms IS NULL` (migration 0063);
 *   - `FROM tenant WHERE clerk_user_id` lookups resolve the Clerk bridge's
 *     tenant mapping;
 *   - everything else (tier/quota/region probes on the PAT path) → null.
 */
function makeDualD1(opts: {
  pats?: Map<string, { tenant_id: string; expires_ms: number; revoked_at_ms?: number | null }>;
  clerkUserToTenant?: Map<string, string>;
}): D1Database {
  return {
    prepare: (sql: string) => ({
      bind: (...args: unknown[]) => ({
        first: async <T>() => {
          if (sql.includes("FROM pat")) {
            const row = opts.pats?.get(args[0] as string);
            if (sql.includes("revoked_at_ms IS NULL") && row?.revoked_at_ms != null) {
              return null as T | null;
            }
            return (row ?? null) as T | null;
          }
          if (sql.includes("FROM tenant WHERE clerk_user_id")) {
            const tenantId = opts.clerkUserToTenant?.get(args[0] as string);
            return (tenantId ? { tenant_id: tenantId } : null) as T | null;
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

/** CORELINK_SERVER DO namespace that CAPTURES the forwarded request + DO name. */
function makeCaptureNamespace(captured: { req?: Request; doName?: string }): DurableObjectNamespace {
  const stub = {
    fetch: async (req: Request): Promise<Response> => {
      captured.req = req;
      return new Response(JSON.stringify({ ok: true }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      });
    },
  };
  const namespace = {
    idFromName: (n: string) => {
      captured.doName = n;
      return { toString: () => `stub-${n}` };
    },
    get: (_id: unknown) => stub,
    idFromString: (_s: string) => ({ toString: () => "stub-id" }),
    newUniqueId: () => ({ toString: () => "stub-unique" }),
    jurisdiction: (_j: string) => namespace,
  } as unknown as DurableObjectNamespace;
  return namespace;
}

function makeBridgeEnv(opts: {
  captured: { req?: Request; doName?: string };
  pats?: Map<string, { tenant_id: string; expires_ms: number; revoked_at_ms?: number | null }>;
  clerkUserToTenant?: Map<string, string>;
  withClerkSecret?: boolean;
}): Env {
  return {
    CORELINK_SERVER: makeCaptureNamespace(opts.captured),
    ENVIRONMENT: "test",
    CONFIG_DB: makeDualD1({ pats: opts.pats, clerkUserToTenant: opts.clerkUserToTenant }),
    // Honest auth harness (#345): bind the valid test signing key so a minted
    // PAT's HMAC verifies and the PAT surface reaches the real auth/route logic
    // (instead of fail-closing to 503 on an unset key).
    PAT_SIGNING_KEY: TEST_PAT_SIGNING_KEY,
    CLERK_SECRET_KEY: opts.withClerkSecret === false ? undefined : CLERK_SECRET,
    // Bound ON PURPOSE: the least-privilege assertion below must prove the
    // customer_v1 Clerk forward does NOT inject internal-auth even when the
    // key IS available to the Worker (unlike onboarding, which injects it).
    CORELINK_INTERNAL_AUTH_KEY: INTERNAL_KEY,
  } as Env;
}

async function customerFetch(env: Env, headers: Record<string, string>): Promise<Response> {
  const req = new Request("http://localhost/v1/customer/overview", {
    method: "GET",
    headers,
  });
  return workerHandler.fetch!(req, env, makeCtx());
}

describe("/v1/customer/* — Clerk session bridge (dashboard revival WP-1)", () => {
  beforeEach(() => {
    mockVerifyToken.mockReset();
  });

  // ── PAT surface regression (byte-identical dispatch) ───────────────────────

  it("PAT path regression: a valid canonical PAT still forwards via the PAT gate", async () => {
    const captured: { req?: Request; doName?: string } = {};
    const env = makeBridgeEnv({
      captured,
      pats: new Map([[TEST_TOKEN_ID, { tenant_id: TEST_TENANT_ID, expires_ms: Date.now() + 3_600_000 }]]),
    });

    const resp = await customerFetch(env, { Authorization: `Bearer ${TEST_PAT_TOKEN}` });

    expect(resp.status).toBe(200);
    const h = captured.req!.headers;
    expect(h.get("x-corelink-route-kind")).toBe("customer_v1");
    expect(h.get("x-corelink-tenant-id")).toBe(TEST_TENANT_ID);
    // The PAT path FORWARDS Authorization (DO performs the Argon2id verify) —
    // proof the request took the PAT gate, not the Clerk arm (which drops it).
    expect(h.get("authorization")).toBe(`Bearer ${TEST_PAT_TOKEN}`);
    expect(h.get("x-corelink-token-prefix")).not.toBe("clerk");
    // The Clerk verifier must never run on the PAT surface.
    expect(mockVerifyToken).not.toHaveBeenCalled();
  });

  it("revoked-PAT regression (0063): a soft-revoked PAT still 401s on the PAT surface", async () => {
    // A revoked PAT still PARSES as a canonical PAT, so the dispatch guard
    // must route it to the PAT gate (where `revoked_at_ms IS NULL` filters it
    // out → 401) — NEVER into the Clerk pipeline.
    const captured: { req?: Request; doName?: string } = {};
    const env = makeBridgeEnv({
      captured,
      pats: new Map([
        [REVOKED_TOKEN_ID, { tenant_id: TEST_TENANT_ID, expires_ms: Date.now() + 3_600_000, revoked_at_ms: Date.now() - 1000 }],
      ]),
    });

    const resp = await customerFetch(env, { Authorization: `Bearer ${REVOKED_PAT_TOKEN}` });

    expect(resp.status).toBe(401);
    const body = await resp.json() as { error: string };
    expect(body.error).toBe("UNAUTHORIZED");
    expect(captured.req).toBeUndefined();
    expect(mockVerifyToken).not.toHaveBeenCalled();
  });

  // ── Clerk pipeline: rejection paths ─────────────────────────────────────────

  it("returns 401 on a bad-signature / expired Clerk JWT (never forwards)", async () => {
    mockVerifyToken.mockRejectedValue(new Error("signature verification failed"));
    const captured: { req?: Request; doName?: string } = {};
    const env = makeBridgeEnv({ captured });

    const resp = await customerFetch(env, { Authorization: "Bearer eyJ.bad.jwt" });

    expect(resp.status).toBe(401);
    const body = await resp.json() as { error: string };
    expect(body.error).toBe("UNAUTHORIZED");
    expect(captured.req).toBeUndefined();
  });

  it("returns 401 when no Authorization header is present (Clerk arm, UNAUTHORIZED envelope)", async () => {
    const captured: { req?: Request; doName?: string } = {};
    const env = makeBridgeEnv({ captured });

    const resp = await customerFetch(env, {});

    expect(resp.status).toBe(401);
    const body = await resp.json() as { error: string };
    expect(body.error).toBe("UNAUTHORIZED");
    expect(captured.req).toBeUndefined();
    expect(mockVerifyToken).not.toHaveBeenCalled();
  });

  it("returns 403 when the verified session has no CoreLink tenant", async () => {
    mockVerifyToken.mockResolvedValue({
      sub: "user_no_tenant",
      azp: "https://corelink-app.humangr.com",
      iss: "https://clerk.humangr.com",
    } as never);
    const captured: { req?: Request; doName?: string } = {};
    const env = makeBridgeEnv({ captured, clerkUserToTenant: new Map() }); // no mapping

    const resp = await customerFetch(env, { Authorization: "Bearer clerk.jwt" });

    expect(resp.status).toBe(403);
    expect(captured.req).toBeUndefined();
  });

  it("M1: returns 401 when azp is not in the allowlist (never forwards)", async () => {
    mockVerifyToken.mockResolvedValue({
      sub: "user_abc",
      azp: "https://attacker-app.example.com",
      iss: "https://clerk.humangr.com",
    } as never);
    const captured: { req?: Request; doName?: string } = {};
    const env = makeBridgeEnv({
      captured,
      clerkUserToTenant: new Map([["user_abc", CLERK_TENANT_ID]]),
    });

    const resp = await customerFetch(env, { Authorization: "Bearer wrong-azp.jwt" });

    expect(resp.status).toBe(401);
    expect(captured.req).toBeUndefined();
  });

  it("M1: returns 401 when azp is absent (library skips the check; our re-assert catches it)", async () => {
    mockVerifyToken.mockResolvedValue({
      sub: "user_abc",
      // azp intentionally absent
      iss: "https://clerk.humangr.com",
    } as never);
    const captured: { req?: Request; doName?: string } = {};
    const env = makeBridgeEnv({
      captured,
      clerkUserToTenant: new Map([["user_abc", CLERK_TENANT_ID]]),
    });

    const resp = await customerFetch(env, { Authorization: "Bearer no-azp.jwt" });

    expect(resp.status).toBe(401);
    expect(captured.req).toBeUndefined();
  });

  it("accepts azp from corelink-app.humangr.com (the dashboard host)", async () => {
    mockVerifyToken.mockResolvedValue({
      sub: "user_app",
      azp: "https://corelink-app.humangr.com",
      iss: "https://clerk.humangr.com",
    } as never);
    const captured: { req?: Request; doName?: string } = {};
    const env = makeBridgeEnv({
      captured,
      clerkUserToTenant: new Map([["user_app", CLERK_TENANT_ID]]),
    });

    const resp = await customerFetch(env, { Authorization: "Bearer clerk.jwt" });

    expect(resp.status).toBe(200);
    expect(captured.req!.headers.get("x-corelink-tenant-id")).toBe(CLERK_TENANT_ID);
  });

  it("fail-CLOSED: 403 when CLERK_SECRET_KEY is unbound (Clerk arm unavailable)", async () => {
    const captured: { req?: Request; doName?: string } = {};
    const env = makeBridgeEnv({ captured, withClerkSecret: false });

    const resp = await customerFetch(env, { Authorization: "Bearer clerk.jwt" });

    expect(resp.status).toBe(403);
    expect(captured.req).toBeUndefined();
    expect(mockVerifyToken).not.toHaveBeenCalled();
  });

  // ── Clerk pipeline: least-privilege forward ─────────────────────────────────

  it("valid Clerk session: forwards to the PER-TENANT DO with the exact server-trust headers", async () => {
    mockVerifyToken.mockResolvedValue({
      sub: "user_abc",
      azp: "https://corelink-admin.humangr.com",
      iss: "https://clerk.humangr.com",
    } as never);
    const captured: { req?: Request; doName?: string } = {};
    const env = makeBridgeEnv({
      captured,
      clerkUserToTenant: new Map([["user_abc", CLERK_TENANT_ID]]),
    });

    const resp = await customerFetch(env, { Authorization: "Bearer valid.clerk.jwt" });

    expect(resp.status).toBe(200);
    expect(captured.req).toBeDefined();
    // Per-tenant DO — idFromName(resolved tenant), never a shared sentinel.
    expect(captured.doName).toBe(CLERK_TENANT_ID);
    const h = captured.req!.headers;
    expect(h.get("x-corelink-tenant-id")).toBe(CLERK_TENANT_ID);
    expect(h.get("x-corelink-route-kind")).toBe("customer_v1");
    expect(h.get("x-corelink-token-prefix")).toBe("clerk");
    expect(h.get("x-corelink-scope")).toBe("read-write");
    // Least privilege: the Clerk JWT is dropped at the edge…
    expect(h.get("authorization")).toBeNull();
    // …and the internal-auth key is NOT injected (it IS bound in this env —
    // proof the forward deliberately withholds it, unlike onboarding).
    expect(h.get("x-corelink-internal-auth")).toBeNull();
  });

  it("STRIPS client-supplied trust headers before forwarding (spoof defence)", async () => {
    mockVerifyToken.mockResolvedValue({
      sub: "user_abc",
      azp: "https://corelink-admin.humangr.com",
      iss: "https://clerk.humangr.com",
    } as never);
    const captured: { req?: Request; doName?: string } = {};
    const env = makeBridgeEnv({
      captured,
      clerkUserToTenant: new Map([["user_abc", CLERK_TENANT_ID]]),
    });

    await customerFetch(env, {
      Authorization: "Bearer valid.clerk.jwt",
      "x-corelink-internal-auth": "SPOOFED-KEY",
      "x-corelink-tenant-id": "victim-tenant",
      "x-corelink-scope": "admin",
    });

    const h = captured.req!.headers;
    expect(h.get("x-corelink-internal-auth")).toBeNull();
    expect(h.get("x-corelink-tenant-id")).toBe(CLERK_TENANT_ID);
    expect(h.get("x-corelink-scope")).toBe("read-write");
  });

  it("M2: rejects (401) when CLERK_ISSUER_URL is pinned and iss does not match", async () => {
    mockVerifyToken.mockResolvedValue({
      sub: "user_abc",
      azp: "https://corelink-admin.humangr.com",
      iss: "https://attacker.clerk.accounts.dev",
    } as never);
    const captured: { req?: Request; doName?: string } = {};
    const env: Env = {
      ...makeBridgeEnv({ captured, clerkUserToTenant: new Map([["user_abc", CLERK_TENANT_ID]]) }),
      CLERK_ISSUER_URL: "https://clerk.humangr.com",
    };

    const resp = await customerFetch(env, { Authorization: "Bearer wrong-issuer.jwt" });

    expect(resp.status).toBe(401);
    expect(captured.req).toBeUndefined();
  });
});
