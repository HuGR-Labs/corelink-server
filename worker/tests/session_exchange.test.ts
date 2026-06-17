/**
 * Unit tests for POST /v1/session/exchange — seam C, session→PAT exchange
 * (hugit-P2 WP-C).
 *
 * The caller presents a Clerk SESSION JWT (not a CoreLink PAT). The worker
 * verifies it at the edge (shared lib/clerk_auth pipeline), resolves the
 * CoreLink tenant from the verified Clerk user id, and mints a short-lived
 * tenant-scoped PAT by REUSING the container's /_internal/pat/mint route via
 * the _system Durable Object — never forwarding the session token past the
 * edge.
 *
 * @clerk/backend's verifyToken is mocked per-test; the DO stub captures the
 * forwarded mint request so we can assert the exact internal-auth contract the
 * edge injects AND returns a canned MintResponse so we can assert the public
 * exchange response shape (and that the Argon2id hash is NEVER leaked).
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import type { D1Database, DurableObjectNamespace } from "@cloudflare/workers-types";

// Mock the Clerk edge verifier (hoisted) — controlled per test.
vi.mock("@clerk/backend", () => ({ verifyToken: vi.fn() }));
import { verifyToken } from "@clerk/backend";
import workerHandler from "../src/index.js";
import type { Env } from "../src/index.js";

const mockVerifyToken = vi.mocked(verifyToken);

const INTERNAL_KEY = "test-internal-auth-key-0123456789"; // ≥16 chars
const CLERK_SECRET = "sk_test_clerk_secret";

// githugr multi-issuer (Option B) test fixtures.
const GITHUGR_ISSUER = "https://clerk.githugr.com";
const GITHUGR_JWT_KEY = "-----BEGIN PUBLIC KEY-----\nMOCKKEY\n-----END PUBLIC KEY-----";
const GITHUGR_TENANT = "ee30f7ba-fc25-4d71-939e-ebe130b4c6a3";
const GITHUGR_AZP = "https://www.githugr.com";

/** A JWT whose UNVERIFIED payload carries `iss`/`sub` — for the peek-routing in
 *  `verifyClerkSessionAndResolveTenant`. The signature is mock (`verifyToken` is
 *  mocked), but the payload must really base64url-decode so routing picks the path. */
function bearerWithIssuer(iss: string, sub = "user_g"): string {
  const b64url = (o: unknown) => Buffer.from(JSON.stringify(o)).toString("base64url");
  return `${b64url({ alg: "RS256", typ: "JWT" })}.${b64url({ iss, sub })}.sig`;
}

/** A representative container /_internal/pat/mint 200 response. */
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

/**
 * CONFIG_DB mock.
 *
 * Resolves the tenant lookup (clerk_user_id → tenant_id), the per-principal mint
 * throttle (`session_exchange_throttle` INSERT…RETURNING count), and the pat
 * persistence INSERT that {@link mintScopedPat} now performs after a successful
 * container mint. `patInsertCapture` records the binds of that INSERT so a test
 * can assert the exact columns; `patInsertThrows` simulates an FK/UNIQUE/CHECK
 * failure (fail-CLOSED path).
 */
function makeConfigDb(
  clerkUserToTenant: Map<string, string>,
  opts: { patInsertCapture?: { binds?: unknown[] }; patInsertThrows?: boolean } = {},
): D1Database {
  return {
    prepare: (sql: string) => ({
      bind: (...args: unknown[]) => ({
        first: async <T>() => {
          // Throttle INSERT…ON CONFLICT…RETURNING count → 1 (under cap).
          if (sql.includes("session_exchange_throttle")) {
            return { count: 1 } as T;
          }
          // Tenant lookup by clerk_user_id.
          const clerkUserId = args[0] as string;
          const tenantId = clerkUserToTenant.get(clerkUserId);
          return (tenantId ? { tenant_id: tenantId } : null) as T | null;
        },
        run: async () => {
          if (sql.includes("INSERT INTO pat")) {
            if (opts.patInsertThrows) {
              throw new Error("D1 pat INSERT constraint failure (FK/UNIQUE/CHECK)");
            }
            if (opts.patInsertCapture) {
              opts.patInsertCapture.binds = args;
            }
          }
          return { success: true } as unknown as D1Result;
        },
      }),
    }),
  } as unknown as D1Database;
}

/**
 * CORELINK_SERVER DO namespace that CAPTURES the forwarded mint request and
 * replies with a configurable response (default: the canned MintResponse 200).
 */
function makeMintNamespace(
  captured: { req?: Request },
  opts: { status?: number; body?: unknown } = {},
): DurableObjectNamespace {
  const stub = {
    fetch: async (req: Request): Promise<Response> => {
      captured.req = req;
      const status = opts.status ?? 200;
      const body = opts.body ?? CANNED_MINT;
      return new Response(typeof body === "string" ? body : JSON.stringify(body), {
        status,
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
  mintStatus?: number;
  mintBody?: unknown;
  patInsertCapture?: { binds?: unknown[] };
  patInsertThrows?: boolean;
  withGithugr?: boolean;
}): Env {
  return {
    ...(opts.withGithugr
      ? {
          GITHUGR_CLERK_ISSUER_URL: GITHUGR_ISSUER,
          GITHUGR_CLERK_JWT_KEY: GITHUGR_JWT_KEY,
          GITHUGR_TENANT_ID: GITHUGR_TENANT,
        }
      : {}),
    CORELINK_SERVER: makeMintNamespace(opts.captured, {
      ...(opts.mintStatus !== undefined ? { status: opts.mintStatus } : {}),
      ...(opts.mintBody !== undefined ? { body: opts.mintBody } : {}),
    }),
    ENVIRONMENT: "test",
    CONFIG_DB: makeConfigDb(opts.clerkUserToTenant, {
      ...(opts.patInsertCapture !== undefined ? { patInsertCapture: opts.patInsertCapture } : {}),
      ...(opts.patInsertThrows !== undefined ? { patInsertThrows: opts.patInsertThrows } : {}),
    }),
    CLERK_SECRET_KEY: opts.withClerkSecret === false ? undefined : CLERK_SECRET,
    CORELINK_INTERNAL_AUTH_KEY: opts.withInternalKey === false ? undefined : INTERNAL_KEY,
  } as Env;
}

function exchangeFetch(
  env: Env,
  headers: Record<string, string>,
  method = "POST",
): Promise<Response> {
  const req = new Request("http://localhost/v1/session/exchange", {
    method,
    headers: { "Content-Type": "application/json", ...headers },
  });
  return workerHandler.fetch!(req, env, makeCtx());
}

function validClaims(sub: string) {
  return {
    sub,
    azp: "https://corelink-admin.humangr.com",
    iss: "https://clerk.humangr.com",
  } as never;
}

describe("POST /v1/session/exchange — seam C session→PAT exchange (WP-C)", () => {
  beforeEach(() => {
    mockVerifyToken.mockReset();
  });

  it("verifies the session, mints via /_internal/pat/mint, returns the public PAT", async () => {
    mockVerifyToken.mockResolvedValue(validClaims("user_abc"));
    const captured: { req?: Request } = {};
    const env = makeEnv({ captured, clerkUserToTenant: new Map([["user_abc", "acme-default"]]) });

    const resp = await exchangeFetch(env, { Authorization: "Bearer clerk.jwt" });

    expect(resp.status).toBe(200);
    const body = (await resp.json()) as Record<string, unknown>;
    expect(body["token_plaintext"]).toBe(CANNED_MINT.token_plaintext);
    expect(body["pat_id"]).toBe(CANNED_MINT.pat_id);
    expect(body["token_id"]).toBe(CANNED_MINT.token_id);
    expect(body["expires_ms"]).toBe(CANNED_MINT.expires_ms);
    // F-01: `principal` is the OPAQUE derived UUID, NEVER the raw Clerk user id
    // (surfacing `user_abc` would leak the upstream IdP subject to the client).
    expect(body["principal"]).toMatch(
      /^[0-9a-f]{8}-[0-9a-f]{4}-8[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/,
    );
    expect(body["principal"]).not.toBe("user_abc");
    expect(body["tenant"]).toBe("acme-default");
  });

  it("NEVER leaks the Argon2id hash to the caller", async () => {
    mockVerifyToken.mockResolvedValue(validClaims("user_abc"));
    const captured: { req?: Request } = {};
    const env = makeEnv({ captured, clerkUserToTenant: new Map([["user_abc", "acme-default"]]) });

    const resp = await exchangeFetch(env, { Authorization: "Bearer clerk.jwt" });
    const body = (await resp.json()) as Record<string, unknown>;

    expect(body["hash"]).toBeUndefined();
  });

  it("calls the container mint with the SERVER internal-auth + a cas:rw short-TTL body", async () => {
    mockVerifyToken.mockResolvedValue(validClaims("user_abc"));
    const captured: { req?: Request } = {};
    const env = makeEnv({ captured, clerkUserToTenant: new Map([["user_abc", "acme-default"]]) });

    await exchangeFetch(env, { Authorization: "Bearer clerk.jwt" });

    expect(captured.req).toBeDefined();
    const h = captured.req!.headers;
    expect(h.get("x-corelink-internal-auth")).toBe(INTERNAL_KEY);
    expect(h.get("x-corelink-route-kind")).toBe("internal");
    const mintBody = (await captured.req!.json()) as Record<string, unknown>;
    expect(mintBody["tenant_id"]).toBe("acme-default");
    expect(mintBody["scopes"]).toBe("cas:rw");
    expect(mintBody["ttl_seconds"]).toBe(3600);
    // principal_id is a deterministic UUID derived from the Clerk user id.
    expect(mintBody["principal_id"]).toMatch(
      /^[0-9a-f]{8}-[0-9a-f]{4}-8[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/,
    );
  });

  it("derives a STABLE principal_id for the same Clerk user across two exchanges", async () => {
    mockVerifyToken.mockResolvedValue(validClaims("user_stable"));
    const cap1: { req?: Request } = {};
    const env1 = makeEnv({ captured: cap1, clerkUserToTenant: new Map([["user_stable", "t1"]]) });
    await exchangeFetch(env1, { Authorization: "Bearer clerk.jwt" });
    const id1 = ((await cap1.req!.json()) as Record<string, unknown>)["principal_id"];

    const cap2: { req?: Request } = {};
    const env2 = makeEnv({ captured: cap2, clerkUserToTenant: new Map([["user_stable", "t1"]]) });
    await exchangeFetch(env2, { Authorization: "Bearer clerk.jwt" });
    const id2 = ((await cap2.req!.json()) as Record<string, unknown>)["principal_id"];

    expect(id1).toBe(id2);
  });

  it("NEVER forwards the Clerk session JWT to the container", async () => {
    mockVerifyToken.mockResolvedValue(validClaims("user_abc"));
    const captured: { req?: Request } = {};
    const env = makeEnv({ captured, clerkUserToTenant: new Map([["user_abc", "acme-default"]]) });

    await exchangeFetch(env, { Authorization: "Bearer super.secret.jwt" });

    expect(captured.req!.headers.get("authorization")).toBeNull();
  });

  it("returns 405 on a non-POST method (GET)", async () => {
    const captured: { req?: Request } = {};
    const env = makeEnv({ captured, clerkUserToTenant: new Map() });

    const resp = await exchangeFetch(env, { Authorization: "Bearer clerk.jwt" }, "GET");

    expect(resp.status).toBe(405);
    expect(captured.req).toBeUndefined();
  });

  it("returns 401 on an invalid / expired session (never mints)", async () => {
    mockVerifyToken.mockRejectedValue(new Error("token expired"));
    const captured: { req?: Request } = {};
    const env = makeEnv({ captured, clerkUserToTenant: new Map() });

    const resp = await exchangeFetch(env, { Authorization: "Bearer bad.jwt" });

    expect(resp.status).toBe(401);
    expect(captured.req).toBeUndefined();
  });

  it("returns 401 when no Bearer session is present", async () => {
    const captured: { req?: Request } = {};
    const env = makeEnv({ captured, clerkUserToTenant: new Map() });

    const resp = await exchangeFetch(env, {});

    expect(resp.status).toBe(401);
    expect(captured.req).toBeUndefined();
  });

  it("returns 403 when the verified session has no CoreLink tenant", async () => {
    mockVerifyToken.mockResolvedValue(validClaims("user_no_tenant"));
    const captured: { req?: Request } = {};
    const env = makeEnv({ captured, clerkUserToTenant: new Map() }); // no mapping

    const resp = await exchangeFetch(env, { Authorization: "Bearer clerk.jwt" });

    expect(resp.status).toBe(403);
    expect(captured.req).toBeUndefined();
  });

  it("fail-CLOSED: 403 when CLERK_SECRET_KEY is unbound (no verification possible)", async () => {
    const captured: { req?: Request } = {};
    const env = makeEnv({ captured, clerkUserToTenant: new Map(), withClerkSecret: false });

    const resp = await exchangeFetch(env, { Authorization: "Bearer clerk.jwt" });

    expect(resp.status).toBe(403);
    expect(captured.req).toBeUndefined();
  });

  it("fail-CLOSED: 403 when CORELINK_INTERNAL_AUTH_KEY is unbound (cannot mint)", async () => {
    mockVerifyToken.mockResolvedValue(validClaims("user_abc"));
    const captured: { req?: Request } = {};
    const env = makeEnv({
      captured,
      clerkUserToTenant: new Map([["user_abc", "acme-default"]]),
      withInternalKey: false,
    });

    const resp = await exchangeFetch(env, { Authorization: "Bearer clerk.jwt" });

    expect(resp.status).toBe(403);
    expect(captured.req).toBeUndefined();
  });

  it("M1: rejects (401) a verified token whose azp is not in the allowlist (never mints)", async () => {
    mockVerifyToken.mockResolvedValue({
      sub: "user_abc",
      azp: "https://attacker-app.example.com",
      iss: "https://clerk.humangr.com",
    } as never);
    const captured: { req?: Request } = {};
    const env = makeEnv({ captured, clerkUserToTenant: new Map([["user_abc", "acme-default"]]) });

    const resp = await exchangeFetch(env, { Authorization: "Bearer wrong-azp.jwt" });

    expect(resp.status).toBe(401);
    expect(captured.req).toBeUndefined();
  });

  it("collapses a container mint failure to a fail-CLOSED 500 (no internal status leak)", async () => {
    // Unique principal: the F20 in-memory mint-throttle backstop is module-scoped
    // and persists across tests in this file, so a per-test user id keeps each
    // test independent of the others' mint counts.
    mockVerifyToken.mockResolvedValue(validClaims("user_mintfail"));
    const captured: { req?: Request } = {};
    const env = makeEnv({
      captured,
      clerkUserToTenant: new Map([["user_mintfail", "acme-default"]]),
      mintStatus: 401,
      mintBody: { error: "unauthorized" },
    });

    const resp = await exchangeFetch(env, { Authorization: "Bearer clerk.jwt" });

    // The mint route's 401 must NOT surface verbatim — collapse to 500.
    expect(resp.status).toBe(500);
    const body = (await resp.json()) as Record<string, unknown>;
    expect(body["error"]).toBe("INTERNAL_ERROR");
  });

  it("returns 500 when the container mint response is malformed (missing fields)", async () => {
    // Unique principal — see the F20 module-scoped throttle note above.
    mockVerifyToken.mockResolvedValue(validClaims("user_malformed"));
    const captured: { req?: Request } = {};
    const env = makeEnv({
      captured,
      clerkUserToTenant: new Map([["user_malformed", "acme-default"]]),
      mintBody: { pat_id: "only-this" }, // missing token_plaintext / token_id / expires_ms
    });

    const resp = await exchangeFetch(env, { Authorization: "Bearer clerk.jwt" });

    expect(resp.status).toBe(500);
  });

  // ── pat-row persistence (the MINTSCOPEDPAT-PERSIST fix) ─────────────────────

  it("PERSIST: on a successful mint, INSERTs the pat row with the right columns", async () => {
    mockVerifyToken.mockResolvedValue(validClaims("user_persist_ok"));
    const captured: { req?: Request } = {};
    const patInsertCapture: { binds?: unknown[] } = {};
    const env = makeEnv({
      captured,
      clerkUserToTenant: new Map([["user_persist_ok", "acme-default"]]),
      patInsertCapture,
    });

    const resp = await exchangeFetch(env, { Authorization: "Bearer clerk.jwt" });

    expect(resp.status).toBe(200);
    expect(patInsertCapture.binds).toBeDefined();
    const binds = patInsertCapture.binds!;
    // (pat_id, tenant_id, pat_hash, scope, expires_ms, token_id,
    //  shown_once_token, [consumed=1 literal], created_ms)
    expect(binds[0]).toBe(CANNED_MINT.pat_id); // pat_id
    expect(binds[1]).toBe("acme-default"); // tenant_id (FK)
    expect(binds[2]).toBe(CANNED_MINT.hash); // pat_hash = the container's Argon2id hash
    expect(binds[3]).toBe("read-write"); // scope canonicalized from cas:rw
    expect(binds[4]).toBe(CANNED_MINT.expires_ms); // expires_ms
    expect(binds[5]).toBe(CANNED_MINT.token_id); // token_id
    expect(binds[6]).toBe(CANNED_MINT.token_id); // shown_once_token = unique token_id
    expect(binds[6]).not.toBe(CANNED_MINT.token_plaintext); // NEVER the raw plaintext
    expect(typeof binds[7]).toBe("number"); // created_ms = Date.now()
  });

  it("PERSIST: cas:rw is canonicalized to the D1 CHECK-legal 'read-write'", async () => {
    // The session-exchange caller always passes cas:rw; assert the stored scope
    // is the CHECK-legal canonical value (a raw 'cas:rw' would fail the CHECK).
    mockVerifyToken.mockResolvedValue(validClaims("user_scope_map"));
    const captured: { req?: Request } = {};
    const patInsertCapture: { binds?: unknown[] } = {};
    const env = makeEnv({
      captured,
      clerkUserToTenant: new Map([["user_scope_map", "acme-default"]]),
      patInsertCapture,
    });

    await exchangeFetch(env, { Authorization: "Bearer clerk.jwt" });

    expect(patInsertCapture.binds![3]).toBe("read-write");
  });

  it("PERSIST: fail-CLOSED 500 + NO token when the pat INSERT throws (FK/UNIQUE/CHECK)", async () => {
    mockVerifyToken.mockResolvedValue(validClaims("user_persist_fk"));
    const captured: { req?: Request } = {};
    const env = makeEnv({
      captured,
      clerkUserToTenant: new Map([["user_persist_fk", "acme-default"]]),
      patInsertThrows: true,
    });

    const resp = await exchangeFetch(env, { Authorization: "Bearer clerk.jwt" });

    expect(resp.status).toBe(500);
    const body = (await resp.json()) as Record<string, unknown>;
    expect(body["error"]).toBe("INTERNAL_ERROR");
    // The token must NEVER be returned when its row could not be persisted.
    expect(body["token_plaintext"]).toBeUndefined();
  });

  it("PERSIST: fail-CLOSED 500 when the mint 200 has no Argon2id hash", async () => {
    mockVerifyToken.mockResolvedValue(validClaims("user_nohash"));
    const captured: { req?: Request } = {};
    // A 200 with all public fields but NO hash → cannot persist an authenticatable
    // row → fail-CLOSED (never write a hash-less row, never return the token).
    const env = makeEnv({
      captured,
      clerkUserToTenant: new Map([["user_nohash", "acme-default"]]),
      mintBody: {
        token_plaintext: "corelink_pat_x.y.z",
        pat_id: "aaaa1111-2222-3333-4444-555555555555",
        token_id: "fedcba9876543210",
        expires_ms: 1893456000000,
        // hash deliberately omitted
      },
    });

    const resp = await exchangeFetch(env, { Authorization: "Bearer clerk.jwt" });

    expect(resp.status).toBe(500);
    const body = (await resp.json()) as Record<string, unknown>;
    expect(body["token_plaintext"]).toBeUndefined();
  });
});

describe("POST /v1/session/exchange — githugr multi-issuer (Option B, single tenant)", () => {
  it("a githugr-issuer session resolves to the FIXED githugr tenant (no per-user D1 lookup)", async () => {
    mockVerifyToken.mockResolvedValue({ sub: "user_g", azp: GITHUGR_AZP, iss: GITHUGR_ISSUER } as never);
    const captured: { req?: Request } = {};
    // EMPTY tenant map: a clerk_user_id→tenant lookup would 403. A 200 proves the
    // fixed-tenant resolution path (Option B) was taken, not the D1 lookup.
    const env = makeEnv({ captured, clerkUserToTenant: new Map(), withGithugr: true });
    const resp = await exchangeFetch(env, {
      Authorization: `Bearer ${bearerWithIssuer(GITHUGR_ISSUER, "user_g")}`,
    });
    expect(resp.status).toBe(200);
    const body = (await resp.json()) as Record<string, unknown>;
    expect(body["tenant"]).toBe(GITHUGR_TENANT);
  });

  it("rejects (401) a githugr token whose azp is not in the githugr allowlist", async () => {
    mockVerifyToken.mockResolvedValue({
      sub: "user_g",
      azp: "https://evil.example",
      iss: GITHUGR_ISSUER,
    } as never);
    const captured: { req?: Request } = {};
    const env = makeEnv({ captured, clerkUserToTenant: new Map(), withGithugr: true });
    const resp = await exchangeFetch(env, {
      Authorization: `Bearer ${bearerWithIssuer(GITHUGR_ISSUER, "user_g")}`,
    });
    expect(resp.status).toBe(401);
  });

  it("with githugr configured, a CoreLink-issuer session STILL uses the CoreLink tenant lookup (no regression)", async () => {
    mockVerifyToken.mockResolvedValue(validClaims("user_abc"));
    const captured: { req?: Request } = {};
    const env = makeEnv({
      captured,
      clerkUserToTenant: new Map([["user_abc", "acme-default"]]),
      withGithugr: true,
    });
    const resp = await exchangeFetch(env, {
      Authorization: `Bearer ${bearerWithIssuer("https://clerk.humangr.com", "user_abc")}`,
    });
    expect(resp.status).toBe(200);
    const body = (await resp.json()) as Record<string, unknown>;
    expect(body["tenant"]).toBe("acme-default");
  });
});
