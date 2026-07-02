/**
 * Unit tests for the D-9 corelink-runners seam:
 *   - POST /internal/v1/runner/mint   — per-job, short-TTL, tenant-scoped PAT.
 *   - POST /internal/v1/runner/revoke — revoke a runner PAT by pat_id.
 *
 * The mint is `token-exchange` MINUS the Clerk session, PLUS a runners-
 * entitlement check: the trusted dispatcher (internal-auth) mints on behalf of a
 * disposable runner. It REUSES the single mint authority (mintScopedPat → the
 * container's /_internal/pat/mint via the _system DO) and derives a stable
 * per-job principal UUID from `job_id`. Revoke reuses the existing
 * `UPDATE pat SET revoked_at_ms` revocation surface.
 *
 * The _system DO stub captures the forwarded mint so we can assert the TTL +
 * scope + that no client trust headers reach the mint route. The CONFIG_DB stub
 * answers the entitlement lookup, the per-principal throttle INSERT, and the
 * revoke UPDATE by inspecting the SQL.
 */

import { describe, it, expect } from "vitest";
import type { D1Database, DurableObjectNamespace } from "@cloudflare/workers-types";
import workerHandler from "../src/index.js";
import type { Env } from "../src/index.js";

const INTERNAL_KEY = "test-internal-auth-key-0123456789"; // ≥32 chars
const RUNNER_MINT_KEY = "test-pat-mint-auth-key-0123456789ab"; // ≥32 chars, distinct

const TENANT = "11111111-1111-1111-1111-111111111111";
const JOB_ID = "job-abc-0001";

const CANNED_MINT = {
  token_plaintext: "corelink_pat_abcdef0123456789.rndsecret.hmacsig",
  pat_id: "22222222-3333-4444-5555-666666666666",
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
 * CONFIG_DB stub. `entitled` is the set of tenant ids that have a
 * runners_entitlement row. `revokeCapture` records the binds of the revoke
 * UPDATE so a test can assert the PAT was marked revoked.
 */
function makeConfigDb(opts: {
  entitled: Set<string>;
  revokeCapture?: { binds?: unknown[] };
}): D1Database {
  return {
    prepare: (sql: string) => ({
      bind: (...args: unknown[]) => ({
        // Entitlement lookup + throttle INSERT...RETURNING both go through first().
        first: async <T>() => {
          if (sql.includes("runners_entitlement")) {
            const tenantId = args[0] as string;
            return (opts.entitled.has(tenantId) ? { tenant_id: tenantId } : null) as T | null;
          }
          // Throttle INSERT ... ON CONFLICT ... RETURNING count → 1 (under cap).
          if (sql.includes("session_exchange_throttle")) {
            return { count: 1 } as T;
          }
          return null as T | null;
        },
        run: async () => {
          if (sql.includes("UPDATE pat SET revoked_at_ms") && opts.revokeCapture) {
            opts.revokeCapture.binds = args;
          }
          return { success: true } as unknown as D1Result;
        },
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
  captured?: { req?: Request };
  entitled?: Set<string>;
  revokeCapture?: { binds?: unknown[] };
  withInternalKey?: boolean;
  withRunnerMintKey?: boolean;
}): Env {
  return {
    CORELINK_SERVER: makeMintNamespace(opts.captured ?? {}),
    ENVIRONMENT: "test",
    CONFIG_DB: makeConfigDb({
      entitled: opts.entitled ?? new Set<string>(),
      revokeCapture: opts.revokeCapture,
    }),
    CORELINK_INTERNAL_AUTH_KEY: opts.withInternalKey === false ? undefined : INTERNAL_KEY,
    CORELINK_RUNNER_MINT_AUTH_KEY: opts.withRunnerMintKey ? RUNNER_MINT_KEY : undefined,
  } as Env;
}

function mintFetch(
  env: Env,
  opts: { auth?: string; body?: unknown; method?: string } = {},
): Promise<Response> {
  const headers: Record<string, string> = { "Content-Type": "application/json" };
  if (opts.auth !== undefined) headers["x-corelink-internal-auth"] = opts.auth;
  const method = opts.method ?? "POST";
  const init: RequestInit = { method, headers };
  if (method === "POST") {
    init.body =
      opts.body !== undefined
        ? JSON.stringify(opts.body)
        : JSON.stringify({ owner_tenant: TENANT, job_id: JOB_ID });
  }
  const req = new Request("http://localhost/internal/v1/runner/mint", init);
  return workerHandler.fetch!(req, env, makeCtx());
}

function revokeFetch(
  env: Env,
  opts: { auth?: string; body?: unknown; method?: string } = {},
): Promise<Response> {
  const headers: Record<string, string> = { "Content-Type": "application/json" };
  if (opts.auth !== undefined) headers["x-corelink-internal-auth"] = opts.auth;
  const method = opts.method ?? "POST";
  const init: RequestInit = { method, headers };
  if (method === "POST") {
    init.body =
      opts.body !== undefined
        ? JSON.stringify(opts.body)
        : JSON.stringify({ pat_id: CANNED_MINT.pat_id, owner_tenant: TENANT });
  }
  const req = new Request("http://localhost/internal/v1/runner/revoke", init);
  return workerHandler.fetch!(req, env, makeCtx());
}

describe("POST /internal/v1/runner/mint — D-9 runner PAT mint", () => {
  it("(a) 401 when the internal-auth header is missing (before any work)", async () => {
    const captured: { req?: Request } = {};
    const env = makeEnv({ captured, entitled: new Set([TENANT]) });
    const resp = await mintFetch(env, {}); // no auth header
    expect(resp.status).toBe(401);
    expect(captured.req).toBeUndefined(); // never minted
  });

  it("(a) 401 when the internal-auth header is blank", async () => {
    const captured: { req?: Request } = {};
    const env = makeEnv({ captured, entitled: new Set([TENANT]) });
    const resp = await mintFetch(env, { auth: "" });
    expect(resp.status).toBe(401);
    expect(captured.req).toBeUndefined();
  });

  it("(a) 403 when NO internal-auth key is bound (fail-CLOSED)", async () => {
    const captured: { req?: Request } = {};
    const env = makeEnv({ captured, entitled: new Set([TENANT]), withInternalKey: false });
    const resp = await mintFetch(env, { auth: INTERNAL_KEY });
    expect(resp.status).toBe(403);
    expect(captured.req).toBeUndefined();
  });

  it("(b) 200 + mint envelope for an entitled tenant with a job_id", async () => {
    const captured: { req?: Request } = {};
    const env = makeEnv({ captured, entitled: new Set([TENANT]) });
    const resp = await mintFetch(env, { auth: INTERNAL_KEY, body: { owner_tenant: TENANT, job_id: JOB_ID } });

    expect(resp.status).toBe(200);
    const body = (await resp.json()) as Record<string, unknown>;
    expect(body["token_plaintext"]).toBe(CANNED_MINT.token_plaintext);
    expect(body["pat_id"]).toBe(CANNED_MINT.pat_id);
    expect(body["token_id"]).toBe(CANNED_MINT.token_id);
    expect(body["expires_ms"]).toBe(CANNED_MINT.expires_ms);
    expect(body["hash"]).toBeUndefined(); // never leak the Argon2id hash

    // The mint was asked for the job-bounded TTL + tenant + cas:rw scope.
    const mintBody = (await captured.req!.json()) as Record<string, unknown>;
    expect(mintBody["ttl_seconds"]).toBe(5400);
    expect(mintBody["tenant_id"]).toBe(TENANT);
    expect(mintBody["scopes"]).toBe("cas:rw");
    // principal_id is the SHA-256-derived UUID of job_id (NOT the raw job_id).
    expect(mintBody["principal_id"]).not.toBe(JOB_ID);
    expect(mintBody["principal_id"]).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-/);
  });

  it("(c) a caller-supplied ttl_seconds below the cap is passed through (lease-bound)", async () => {
    const captured: { req?: Request } = {};
    const env = makeEnv({ captured, entitled: new Set([TENANT]) });
    // Distinct job_id → its own throttle principal (avoids perturbing the
    // shared per-principal in-memory throttle counter other tests rely on).
    const resp = await mintFetch(env, {
      auth: INTERNAL_KEY,
      body: { owner_tenant: TENANT, job_id: `${JOB_ID}-ttl-pass`, ttl_seconds: 600 },
    });
    expect(resp.status).toBe(200);
    const mintBody = (await captured.req!.json()) as Record<string, unknown>;
    expect(mintBody["ttl_seconds"]).toBe(600); // honored, the PAT expires with the lease
  });

  it("(c) a ttl_seconds ABOVE the cap is clamped down to 5400 (never extend)", async () => {
    const captured: { req?: Request } = {};
    const env = makeEnv({ captured, entitled: new Set([TENANT]) });
    const resp = await mintFetch(env, {
      auth: INTERNAL_KEY,
      body: { owner_tenant: TENANT, job_id: `${JOB_ID}-ttl-clamp`, ttl_seconds: 999999 },
    });
    expect(resp.status).toBe(200);
    const mintBody = (await captured.req!.json()) as Record<string, unknown>;
    expect(mintBody["ttl_seconds"]).toBe(5400); // clamped to the 90-min cap
  });

  it("(c) ttl_seconds=0 is REFUSED 400 (the container maps 0 → no-expiry; never mint)", async () => {
    const captured: { req?: Request } = {};
    const env = makeEnv({ captured, entitled: new Set([TENANT]) });
    const resp = await mintFetch(env, {
      auth: INTERNAL_KEY,
      body: { owner_tenant: TENANT, job_id: JOB_ID, ttl_seconds: 0 },
    });
    expect(resp.status).toBe(400);
    expect(captured.req).toBeUndefined(); // never minted a non-expiring PAT
  });

  it("(c) a negative or non-integer ttl_seconds is REFUSED 400", async () => {
    for (const bad of [-1, 3.5, "600", null]) {
      const captured: { req?: Request } = {};
      const env = makeEnv({ captured, entitled: new Set([TENANT]) });
      const resp = await mintFetch(env, {
        auth: INTERNAL_KEY,
        body: { owner_tenant: TENANT, job_id: JOB_ID, ttl_seconds: bad },
      });
      expect(resp.status).toBe(400);
      expect(captured.req).toBeUndefined();
    }
  });

  it("(b) per-job principal UUID is STABLE across mints for the same job_id", async () => {
    const c1: { req?: Request } = {};
    const c2: { req?: Request } = {};
    const env1 = makeEnv({ captured: c1, entitled: new Set([TENANT]) });
    const env2 = makeEnv({ captured: c2, entitled: new Set([TENANT]) });
    await mintFetch(env1, { auth: INTERNAL_KEY, body: { owner_tenant: TENANT, job_id: JOB_ID } });
    await mintFetch(env2, { auth: INTERNAL_KEY, body: { owner_tenant: TENANT, job_id: JOB_ID } });
    const p1 = (await c1.req!.json()) as Record<string, unknown>;
    const p2 = (await c2.req!.json()) as Record<string, unknown>;
    expect(p1["principal_id"]).toBe(p2["principal_id"]);
  });

  it("(b) the per-consumer runner_mint key is accepted (shared key path independent)", async () => {
    const captured: { req?: Request } = {};
    const env = makeEnv({ captured, entitled: new Set([TENANT]), withRunnerMintKey: true });
    // Caller presents the dedicated runner_mint key, NOT the shared key.
    const resp = await mintFetch(env, { auth: RUNNER_MINT_KEY, body: { owner_tenant: TENANT, job_id: JOB_ID } });
    expect(resp.status).toBe(200);
    expect(captured.req).toBeDefined();
  });

  it("(b) when a dedicated runner_mint key is bound, the shared key is rejected", async () => {
    const captured: { req?: Request } = {};
    const env = makeEnv({ captured, entitled: new Set([TENANT]), withRunnerMintKey: true });
    const resp = await mintFetch(env, { auth: INTERNAL_KEY, body: { owner_tenant: TENANT, job_id: JOB_ID } });
    expect(resp.status).toBe(401);
    expect(captured.req).toBeUndefined();
  });

  it("(c) 403 when the tenant has NO runners_entitlement row (not entitled)", async () => {
    const captured: { req?: Request } = {};
    const env = makeEnv({ captured, entitled: new Set() }); // not entitled
    const resp = await mintFetch(env, { auth: INTERNAL_KEY, body: { owner_tenant: TENANT, job_id: JOB_ID } });
    expect(resp.status).toBe(403);
    expect(captured.req).toBeUndefined(); // never minted
  });

  it("(d) 400 when admin scope is requested (least privilege — refused)", async () => {
    const captured: { req?: Request } = {};
    const env = makeEnv({ captured, entitled: new Set([TENANT]) });
    const resp = await mintFetch(env, {
      auth: INTERNAL_KEY,
      body: { owner_tenant: TENANT, job_id: JOB_ID, scope: "admin" },
    });
    expect(resp.status).toBe(400);
    expect(captured.req).toBeUndefined();
  });

  it("(d) 400 when owner scope is requested", async () => {
    const captured: { req?: Request } = {};
    const env = makeEnv({ captured, entitled: new Set([TENANT]) });
    const resp = await mintFetch(env, {
      auth: INTERNAL_KEY,
      body: { owner_tenant: TENANT, job_id: JOB_ID, scope: "owner" },
    });
    expect(resp.status).toBe(400);
    expect(captured.req).toBeUndefined();
  });

  it("400 when owner_tenant is missing", async () => {
    const env = makeEnv({ entitled: new Set([TENANT]) });
    const resp = await mintFetch(env, { auth: INTERNAL_KEY, body: { job_id: JOB_ID } });
    expect(resp.status).toBe(400);
  });

  it("400 when job_id is missing", async () => {
    const env = makeEnv({ entitled: new Set([TENANT]) });
    const resp = await mintFetch(env, { auth: INTERNAL_KEY, body: { owner_tenant: TENANT } });
    expect(resp.status).toBe(400);
  });

  it("405 on a non-POST method", async () => {
    const captured: { req?: Request } = {};
    const env = makeEnv({ captured, entitled: new Set([TENANT]) });
    const resp = await mintFetch(env, { auth: INTERNAL_KEY, method: "GET" });
    expect(resp.status).toBe(405);
    expect(captured.req).toBeUndefined();
  });

  it("NEVER lets a client trust header reach the mint route", async () => {
    const captured: { req?: Request } = {};
    const env = makeEnv({ captured, entitled: new Set([TENANT]) });
    await mintFetch(env, { auth: INTERNAL_KEY, body: { owner_tenant: TENANT, job_id: JOB_ID } });
    // mintScopedPat builds a FRESH request; the only internal-auth header on the
    // forwarded request is the server-trusted key, and route-kind is "internal".
    expect(captured.req!.headers.get("x-corelink-route-kind")).toBe("internal");
    expect(captured.req!.headers.get("authorization")).toBeNull();
  });
});

describe("POST /internal/v1/runner/revoke — D-9 runner PAT revoke", () => {
  it("(e) 200 + marks the pat revoked via tenant-scoped UPDATE pat SET revoked_at_ms", async () => {
    const revokeCapture: { binds?: unknown[] } = {};
    const env = makeEnv({ revokeCapture });
    const resp = await revokeFetch(env, {
      auth: INTERNAL_KEY,
      body: { pat_id: CANNED_MINT.pat_id, owner_tenant: TENANT },
    });
    expect(resp.status).toBe(200);
    const body = (await resp.json()) as Record<string, unknown>;
    expect(body["pat_id"]).toBe(CANNED_MINT.pat_id);
    expect(body["revoked"]).toBe(true);
    // REV-S2: the revoke UPDATE was bound with (now_ms, pat_id, owner_tenant) — the
    // tenant predicate bounds a compromised pat_mint key to the named tenant.
    expect(revokeCapture.binds).toBeDefined();
    expect(revokeCapture.binds![1]).toBe(CANNED_MINT.pat_id);
    expect(revokeCapture.binds![2]).toBe(TENANT);
    expect(typeof revokeCapture.binds![0]).toBe("number");
  });

  it("401 when the internal-auth header is missing", async () => {
    const revokeCapture: { binds?: unknown[] } = {};
    const env = makeEnv({ revokeCapture });
    const resp = await revokeFetch(env, { body: { pat_id: CANNED_MINT.pat_id, owner_tenant: TENANT } });
    expect(resp.status).toBe(401);
    expect(revokeCapture.binds).toBeUndefined(); // never wrote
  });

  it("400 when pat_id is missing", async () => {
    const env = makeEnv({});
    const resp = await revokeFetch(env, { auth: INTERNAL_KEY, body: { owner_tenant: TENANT } });
    expect(resp.status).toBe(400);
  });

  it("(REV-S2) owner_tenant is now MANDATORY: absent → 400, NO revoke runs", async () => {
    // Hardening complete: the off-repo dispatcher's PR-B is deployed + proven to
    // always send owner_tenant (green-lit 2026-06-21, no lockstep), so the
    // backward-compat un-scoped path is CLOSED. Absent owner_tenant → hard 400 and
    // NO UPDATE — a compromised runner_mint key can never revoke another tenant's
    // PAT by guessing a pat_id.
    const revokeCapture: { binds?: unknown[] } = {};
    const env = makeEnv({ revokeCapture });
    const resp = await revokeFetch(env, { auth: INTERNAL_KEY, body: { pat_id: CANNED_MINT.pat_id } });
    expect(resp.status).toBe(400);
    // No UPDATE is prepared when owner_tenant is missing (fail-CLOSED before D1).
    expect(revokeCapture.binds).toBeUndefined();
  });

  it("405 on a non-POST method", async () => {
    const env = makeEnv({});
    const resp = await revokeFetch(env, { auth: INTERNAL_KEY, method: "GET" });
    expect(resp.status).toBe(405);
  });
});
