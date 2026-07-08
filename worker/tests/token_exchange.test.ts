/**
 * Unit tests for POST /internal/v1/auth/token-exchange — githugr authz #1
 * (RFC 8693 session→tenant-scoped-PAT exchange).
 *
 * The defining behavior is the CROSS-TENANT REJECTION: a session that resolves
 * to tenant A must get a 403 when it asks for `audience` = tenant B. That 403 is
 * githugr's cross-tenant-WRITE rejection. The endpoint is internal-auth gated
 * (githugr backend) AND session gated (the user) AND audience-checked.
 *
 * @clerk/backend's verifyToken is mocked per-test; the _system DO stub captures
 * the forwarded mint so we can assert the short TTL + that the session token is
 * never forwarded past the edge.
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import type { D1Database, DurableObjectNamespace } from "@cloudflare/workers-types";

vi.mock("@clerk/backend", () => ({ verifyToken: vi.fn() }));
import { verifyToken } from "@clerk/backend";
import workerHandler from "../src/index.js";
import type { Env } from "../src/index.js";

const mockVerifyToken = vi.mocked(verifyToken);

const INTERNAL_KEY = "test-internal-auth-key-0123456789"; // ≥32 chars
const CLERK_SECRET = "sk_test_clerk_secret";

const CANNED_MINT = {
  token_plaintext: "corelink_pat_abcdef0123456789.rndsecret.hmacsig",
  pat_id: "11111111-2222-3333-4444-555555555555",
  token_id: "abcdef0123456789",
  expires_ms: 1893456000000,
  hash: "$argon2id$v=19$m=19456,t=2,p=1$c2FsdA$aGFzaA",
};

function makeCtx(): ExecutionContext {
  return {
    waitUntil: (_p: Promise<unknown>) => {},
    passThroughOnException: () => {},
  } as unknown as ExecutionContext;
}

function makeConfigDb(clerkUserToTenant: Map<string, string>): D1Database {
  return {
    prepare: (sql: string) => ({
      bind: (...args: unknown[]) => ({
        first: async <T>() => {
          // Throttle INSERT…ON CONFLICT…RETURNING count → 1 (under cap).
          if (sql.includes("session_exchange_throttle")) {
            return { count: 1 } as T;
          }
          const tenantId = clerkUserToTenant.get(args[0] as string);
          return (tenantId ? { tenant_id: tenantId } : null) as T | null;
        },
        // mintScopedPat now persists the pat row after a successful mint.
        run: async () => ({ success: true }) as unknown as D1Result,
      }),
    }),
  } as unknown as D1Database;
}

function makeMintNamespace(
  captured: { req?: Request },
  opts: { status?: number; body?: unknown } = {},
): DurableObjectNamespace {
  const stub = {
    fetch: async (req: Request): Promise<Response> => {
      captured.req = req;
      return new Response(JSON.stringify(opts.body ?? CANNED_MINT), {
        status: opts.status ?? 200,
        headers: { "Content-Type": "application/json" },
      });
    },
  };
  const namespace = {
    idFromName: (_n: string) => ({ toString: () => "stub-id" }),
    get: (_id: unknown) => stub,
    idFromString: (_s: string) => ({ toString: () => "stub-id" }),
    newUniqueId: () => ({ toString: () => "stub-unique" }),
    jurisdiction: (_j: string) => namespace,
  } as unknown as DurableObjectNamespace;
  return namespace;
}

function makeEnv(opts: {
  captured: { req?: Request };
  clerkUserToTenant: Map<string, string>;
  withClerkSecret?: boolean;
  withInternalKey?: boolean;
}): Env {
  return {
    CORELINK_SERVER: makeMintNamespace(opts.captured),
    ENVIRONMENT: "test",
    CONFIG_DB: makeConfigDb(opts.clerkUserToTenant),
    CLERK_SECRET_KEY: opts.withClerkSecret === false ? undefined : CLERK_SECRET,
    CORELINK_INTERNAL_AUTH_KEY: opts.withInternalKey === false ? undefined : INTERNAL_KEY,
  } as Env;
}

function exchangeFetch(
  env: Env,
  opts: { auth?: string; bearer?: string; body?: unknown; method?: string } = {},
): Promise<Response> {
  const headers: Record<string, string> = { "Content-Type": "application/json" };
  if (opts.auth !== undefined) headers["x-corelink-internal-auth"] = opts.auth;
  if (opts.bearer !== undefined) headers["authorization"] = `Bearer ${opts.bearer}`;
  const method = opts.method ?? "POST";
  const init: RequestInit = { method, headers };
  if (method === "POST") {
    init.body = opts.body !== undefined ? JSON.stringify(opts.body) : JSON.stringify({ audience: "acme" });
  }
  const req = new Request("http://localhost/internal/v1/auth/token-exchange", init);
  return workerHandler.fetch!(req, env, makeCtx());
}

function validClaims(sub: string) {
  return {
    sub,
    azp: "https://corelink-admin.humangr.com",
    iss: "https://clerk.humangr.com",
  } as never;
}

describe("POST /internal/v1/auth/token-exchange — githugr authz #1", () => {
  beforeEach(() => {
    mockVerifyToken.mockReset();
  });

  it("exchanges (session + matching audience) for a ~300s tenant-scoped PAT", async () => {
    mockVerifyToken.mockResolvedValue(validClaims("user_acme"));
    const captured: { req?: Request } = {};
    const env = makeEnv({ captured, clerkUserToTenant: new Map([["user_acme", "acme"]]) });

    const resp = await exchangeFetch(env, { auth: INTERNAL_KEY, bearer: "clerk.jwt", body: { audience: "acme" } });

    expect(resp.status).toBe(200);
    const body = (await resp.json()) as Record<string, unknown>;
    expect(body["token_plaintext"]).toBe(CANNED_MINT.token_plaintext);
    expect(body["tenant"]).toBe("acme");
    expect(body["hash"]).toBeUndefined(); // never leak the Argon2id hash
    // The mint was asked for a 300s TTL.
    const mintBody = (await captured.req!.json()) as Record<string, unknown>;
    expect(mintBody["ttl_seconds"]).toBe(300);
    expect(mintBody["tenant_id"]).toBe("acme");
    expect(mintBody["scopes"]).toBe("cas:rw");
  });

  it("CROSS-TENANT: 403 when session tenant ≠ audience (the githugr critical)", async () => {
    // Session resolves to tenant A ("acme"); caller asks for audience B ("evilcorp").
    mockVerifyToken.mockResolvedValue(validClaims("user_acme"));
    const captured: { req?: Request } = {};
    const env = makeEnv({ captured, clerkUserToTenant: new Map([["user_acme", "acme"]]) });

    const resp = await exchangeFetch(env, {
      auth: INTERNAL_KEY,
      bearer: "clerk.jwt",
      body: { audience: "evilcorp" },
    });

    expect(resp.status).toBe(403);
    expect(captured.req).toBeUndefined(); // NEVER minted for the wrong tenant
  });

  it("400 when audience is absent (the whole point of the endpoint)", async () => {
    mockVerifyToken.mockResolvedValue(validClaims("user_acme"));
    const captured: { req?: Request } = {};
    const env = makeEnv({ captured, clerkUserToTenant: new Map([["user_acme", "acme"]]) });
    const resp = await exchangeFetch(env, { auth: INTERNAL_KEY, bearer: "clerk.jwt", body: {} });
    expect(resp.status).toBe(400);
    expect(captured.req).toBeUndefined();
  });

  it("400 on an unsupported scope (admin refused — least privilege)", async () => {
    mockVerifyToken.mockResolvedValue(validClaims("user_acme"));
    const captured: { req?: Request } = {};
    const env = makeEnv({ captured, clerkUserToTenant: new Map([["user_acme", "acme"]]) });
    const resp = await exchangeFetch(env, {
      auth: INTERNAL_KEY,
      bearer: "clerk.jwt",
      body: { audience: "acme", scope: "admin" },
    });
    expect(resp.status).toBe(400);
    expect(captured.req).toBeUndefined();
  });

  it("401 when the internal-auth header is missing (before any session work)", async () => {
    const captured: { req?: Request } = {};
    const env = makeEnv({ captured, clerkUserToTenant: new Map([["user_acme", "acme"]]) });
    const resp = await exchangeFetch(env, { bearer: "clerk.jwt", body: { audience: "acme" } });
    expect(resp.status).toBe(401);
    expect(captured.req).toBeUndefined();
  });

  it("401 when no Clerk session Bearer is present", async () => {
    const captured: { req?: Request } = {};
    const env = makeEnv({ captured, clerkUserToTenant: new Map() });
    const resp = await exchangeFetch(env, { auth: INTERNAL_KEY, body: { audience: "acme" } });
    expect(resp.status).toBe(401);
    expect(captured.req).toBeUndefined();
  });

  it("401 on an invalid / expired session (never mints)", async () => {
    mockVerifyToken.mockRejectedValue(new Error("expired"));
    const captured: { req?: Request } = {};
    const env = makeEnv({ captured, clerkUserToTenant: new Map() });
    const resp = await exchangeFetch(env, { auth: INTERNAL_KEY, bearer: "bad.jwt", body: { audience: "acme" } });
    expect(resp.status).toBe(401);
    expect(captured.req).toBeUndefined();
  });

  it("403 when the verified session has no CoreLink tenant", async () => {
    mockVerifyToken.mockResolvedValue(validClaims("user_orphan"));
    const captured: { req?: Request } = {};
    const env = makeEnv({ captured, clerkUserToTenant: new Map() }); // no mapping
    const resp = await exchangeFetch(env, { auth: INTERNAL_KEY, bearer: "clerk.jwt", body: { audience: "acme" } });
    expect(resp.status).toBe(403);
    expect(captured.req).toBeUndefined();
  });

  it("NEVER forwards the Clerk session JWT to the container", async () => {
    mockVerifyToken.mockResolvedValue(validClaims("user_acme"));
    const captured: { req?: Request } = {};
    const env = makeEnv({ captured, clerkUserToTenant: new Map([["user_acme", "acme"]]) });
    await exchangeFetch(env, { auth: INTERNAL_KEY, bearer: "super.secret.jwt", body: { audience: "acme" } });
    expect(captured.req!.headers.get("authorization")).toBeNull();
  });

  it("405 on a non-POST method", async () => {
    const captured: { req?: Request } = {};
    const env = makeEnv({ captured, clerkUserToTenant: new Map() });
    const resp = await exchangeFetch(env, { auth: INTERNAL_KEY, bearer: "clerk.jwt", method: "GET" });
    expect(resp.status).toBe(405);
    expect(captured.req).toBeUndefined();
  });

  // ── Track-B: propagate the Clerk step-up freshness signal `fva[0]` ──────────
  function claimsWithFva(sub: string, fva: unknown) {
    return { sub, azp: "https://corelink-admin.humangr.com", iss: "https://clerk.humangr.com", fva } as never;
  }
  // Each exchange uses a UNIQUE user+tenant so repeated calls don't trip the
  // worker's per-tenant session-exchange throttle (429).
  let fvaSeq = 0;
  async function fvaMinutesInResponse(fvaClaim: unknown): Promise<unknown> {
    const u = `user_fva_${fvaSeq}`;
    const t = `tenant_fva_${fvaSeq}`;
    fvaSeq += 1;
    mockVerifyToken.mockResolvedValue(claimsWithFva(u, fvaClaim));
    const captured: { req?: Request } = {};
    const env = makeEnv({ captured, clerkUserToTenant: new Map([[u, t]]) });
    const resp = await exchangeFetch(env, { auth: INTERNAL_KEY, bearer: "clerk.jwt", body: { audience: t } });
    expect(resp.status).toBe(200);
    return ((await resp.json()) as Record<string, unknown>)["fva_minutes"];
  }

  it("propagates fva_minutes = fva[0] on a fresh session (the step-up unblock)", async () => {
    expect(await fvaMinutesInResponse([0, -1])).toBe(0);
    expect(await fvaMinutesInResponse([3, -1])).toBe(3);
  });

  it("OMITS fva_minutes when the session carries no fva claim (fail-closed = not fresh)", async () => {
    expect(await fvaMinutesInResponse(undefined)).toBeUndefined();
  });

  it("OMITS fva_minutes on a malformed/negative fva (never defaults to 0)", async () => {
    // negative first-factor age, non-array, non-integer, missing element → all omit
    expect(await fvaMinutesInResponse([-1, -1])).toBeUndefined();
    expect(await fvaMinutesInResponse("nope")).toBeUndefined();
    expect(await fvaMinutesInResponse([1.5, -1])).toBeUndefined();
    expect(await fvaMinutesInResponse([])).toBeUndefined();
  });
});
