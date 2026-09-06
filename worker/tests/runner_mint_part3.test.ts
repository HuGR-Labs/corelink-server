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
import { batchViaFirst } from "./d1_batch_mock.js";

const INTERNAL_KEY = "test-internal-auth-key-0123456789"; // ≥32 chars
const RUNNER_MINT_KEY = "test-pat-mint-auth-key-0123456789ab"; // ≥32 chars, distinct

const TENANT = "11111111-1111-1111-1111-111111111111";
const JOB_ID = "job-abc-0001";
const INSTALLATION_ID = "gh-install-42";
const REPO_FULL_NAME = "acme/widgets";
const MAX_CONCURRENCY = 7;

// WP5a: blake3("clw/ref/runner/v1/build-out") — the exact-key narrowing value.
// PINNED (also asserted in worker/tests/blake3.test.ts against the official
// BLAKE3 test vectors); must match the Rust `blake3` crate the container derives
// with (WP5b).
const EXPECTED_AC_KEY_HEX = "bbd4ce224ed20e213837318117052855a885949a210ab760e1e075402e826adb";

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
 * CONFIG_DB stub for the WP2 authz chokepoint + the mint persist/throttle path.
 *
 * The 4 authz reads (all via prepare().bind().first()) are keyed off the SQL:
 *   - `tenant_gh_installation_map` → derive tenant. Returns `{ tenant_id }` iff
 *     the installation_id is in `mapped` (installation_id → tenant_id).
 *   - `tenant_offboarding_state`   → suspend gate. Returns a row iff the derived
 *     tenant is in `suspended` (a PRESENT row = suspended → deny).
 *   - `runner_repo_allowlist`      → allowlist gate. Returns a row iff
 *     (tenant, repo) is in `allowlisted`.
 *   - `runners_entitlement`        → entitlement + ceiling. Returns
 *     `{ max_concurrency }` iff the tenant is in `entitled`.
 * Plus the mint INSERT/throttle bookkeeping (session_exchange_throttle, pat).
 */
function makeConfigDb(opts: {
  mapped?: Map<string, string>;
  suspended?: Set<string>;
  // The allowlist key joins tenant and repo on a NUL, which cannot appear in
  // either half, so no pair of values can collide on the joined string.
  // Write it as the ESCAPE `\u0000`, never as a raw NUL byte: one raw NUL
  // makes the whole file BINARY to git, and a binary file gets no diff --
  // `git diff` prints a byte count, so every later change to this mock
  // would ship unreviewed. The escape is the same character to the
  // runtime and keeps the file text.
  allowlisted?: Set<string>; // `${tenant}\u0000${repo}`
  entitled?: Map<string, number>; // tenant → max_concurrency
  /** tenant → max_vcpu_h (0072 is NULLABLE; absent ⇒ the column reads null). */
  vcpuCeilings?: Map<string, number | null>;
  revokeCapture?: { sql?: string; binds?: unknown[] };
  patInsertCapture?: { sql?: string; binds?: unknown[] };
}): D1Database {
  const mapped = opts.mapped ?? new Map<string, string>();
  const suspended = opts.suspended ?? new Set<string>();
  const allowlisted = opts.allowlisted ?? new Set<string>();
  const entitled = opts.entitled ?? new Map<string, number>();
  const vcpuCeilings = opts.vcpuCeilings ?? new Map<string, number | null>();
  return {
    prepare: (sql: string) => ({
      bind: (...args: unknown[]) => ({
        // All authz reads + throttle INSERT...RETURNING go through first().
        first: async <T>() => {
          if (sql.includes("tenant_gh_installation_map")) {
            const installationId = args[0] as string;
            const tenantId = mapped.get(installationId);
            return (tenantId !== undefined ? { tenant_id: tenantId } : null) as T | null;
          }
          if (sql.includes("tenant_offboarding_state")) {
            const tenantId = args[0] as string;
            return (suspended.has(tenantId) ? { "1": 1 } : null) as T | null;
          }
          if (sql.includes("runner_repo_allowlist")) {
            const tenantId = args[0] as string;
            const repo = args[1] as string;
            return (allowlisted.has(`${tenantId}\u0000${repo}`) ? { "1": 1 } : null) as T | null;
          }
          if (sql.includes("runners_entitlement")) {
            const tenantId = args[0] as string;
            const mc = entitled.get(tenantId);
            // `max_vcpu_h` (0072) is NULLABLE: a tenant absent from
            // `vcpuCeilings` gets null, exactly what D1 returns for a row that
            // never had a metered compute ceiling.
            return (mc !== undefined
              ? {
                  max_concurrency: mc,
                  max_vcpu_h: vcpuCeilings.has(tenantId)
                    ? (vcpuCeilings.get(tenantId) ?? null)
                    : null,
                }
              : null) as T | null;
          }
          // Throttle INSERT ... ON CONFLICT ... RETURNING count → 1 (under cap).
          if (sql.includes("session_exchange_throttle")) {
            return { count: 1 } as T;
          }
          // WP-F1: revoke UPDATE ... RETURNING token_id → the revoked row's
          // token (when opts.revokeTokenId is set); null = zero rows matched
          // (already-revoked / unknown pat_id) → NO KV delete.
          if (sql.includes("UPDATE pat SET revoked_at_ms")) {
            if (opts.revokeCapture) {
              opts.revokeCapture.sql = sql;
              opts.revokeCapture.binds = args;
            }
            return (opts.revokeTokenId !== undefined
              ? { token_id: opts.revokeTokenId }
              : null) as T | null;
          }
          return null as T | null;
        },
        run: async () => {
          if (sql.includes("UPDATE pat SET revoked_at_ms") && opts.revokeCapture) {
            opts.revokeCapture.sql = sql;
            opts.revokeCapture.binds = args;
          }
          // WP5a: capture the pat INSERT (SQL + binds) so tests can assert the
          // narrowed runner_job_ac_key column+value. `meta.changes = 1` so the
          // mint's belt-and-braces "wrote no row" guard passes.
          if (sql.includes("INSERT INTO pat") && opts.patInsertCapture) {
            opts.patInsertCapture.sql = sql;
            opts.patInsertCapture.binds = args;
          }
          return { success: true, meta: { changes: 1 } } as unknown as D1Result;
        },
      }),
    }),
  } as unknown as D1Database;
}

/**
 * Convenience: the fully-authorized world — installation mapped to TENANT, not
 * suspended, repo allowlisted, entitled with MAX_CONCURRENCY. Individual tests
 * override one axis to exercise each 403.
 */
function authorizedConfig(opts: {
  mapped?: Map<string, string>;
  suspended?: Set<string>;
  allowlisted?: Set<string>;
  entitled?: Map<string, number>;
  revokeCapture?: { sql?: string; binds?: unknown[] };
  revokeTokenId?: string | null;
  kvDeleted?: string[];
  kvThrow?: boolean;
  patInsertCapture?: { sql?: string; binds?: unknown[] };
}) {
  return {
    mapped: opts.mapped ?? new Map([[INSTALLATION_ID, TENANT]]),
    suspended: opts.suspended ?? new Set<string>(),
    allowlisted: opts.allowlisted ?? new Set([`${TENANT}\u0000${REPO_FULL_NAME}`]),
    entitled: opts.entitled ?? new Map([[TENANT, MAX_CONCURRENCY]]),
    revokeCapture: opts.revokeCapture,
    revokeTokenId: opts.revokeTokenId,
    kvDeleted: opts.kvDeleted,
    kvThrow: opts.kvThrow,
    patInsertCapture: opts.patInsertCapture,
  };
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
  mapped?: Map<string, string>;
  suspended?: Set<string>;
  allowlisted?: Set<string>;
  entitled?: Map<string, number>;
  vcpuCeilings?: Map<string, number | null>;
  revokeCapture?: { sql?: string; binds?: unknown[] };
  revokeTokenId?: string | null;
  kvDeleted?: string[];
  kvThrow?: boolean;
  patInsertCapture?: { sql?: string; binds?: unknown[] };
  withInternalKey?: boolean;
  withRunnerMintKey?: boolean;
}): Env {
  return {
    METADATA_KV: (opts.kvDeleted || opts.kvThrow
      ? {
          get: async () => null,
          put: async () => {},
          delete: async (key: string) => {
            if (opts.kvThrow) throw new Error("kv down");
            opts.kvDeleted?.push(key);
          },
        }
      : undefined),
    CORELINK_SERVER: makeMintNamespace(opts.captured ?? {}),
    ENVIRONMENT: "test",
    CONFIG_DB: makeConfigDb({
      mapped: opts.mapped,
      suspended: opts.suspended,
      allowlisted: opts.allowlisted,
      entitled: opts.entitled,
      vcpuCeilings: opts.vcpuCeilings,
      revokeCapture: opts.revokeCapture,
      revokeTokenId: (opts as { revokeTokenId?: string | null }).revokeTokenId,
      kvDeleted: (opts as { kvDeleted?: string[] }).kvDeleted,
      kvThrow: (opts as { kvThrow?: boolean }).kvThrow,
      patInsertCapture: opts.patInsertCapture,
    }),
    CORELINK_INTERNAL_AUTH_KEY: opts.withInternalKey === false ? undefined : INTERNAL_KEY,
    CORELINK_RUNNER_MINT_AUTH_KEY: opts.withRunnerMintKey ? RUNNER_MINT_KEY : undefined,
  } as Env;
}

/** Env pre-loaded with the fully-authorized world (all 4 checks pass). */
function makeAuthorizedEnv(opts: {
  captured?: { req?: Request };
  mapped?: Map<string, string>;
  suspended?: Set<string>;
  allowlisted?: Set<string>;
  entitled?: Map<string, number>;
  vcpuCeilings?: Map<string, number | null>;
  revokeCapture?: { sql?: string; binds?: unknown[] };
  revokeTokenId?: string | null;
  kvDeleted?: string[];
  kvThrow?: boolean;
  patInsertCapture?: { sql?: string; binds?: unknown[] };
  withInternalKey?: boolean;
  withRunnerMintKey?: boolean;
}): Env {
  const cfg = authorizedConfig(opts);
  return makeEnv({ ...opts, ...cfg });
}

function mintFetch(
  env: Env,
  opts: { auth?: string; body?: unknown; method?: string; bearer?: string } = {},
): Promise<Response> {
  const headers: Record<string, string> = { "Content-Type": "application/json" };
  if (opts.auth !== undefined) headers["x-corelink-internal-auth"] = opts.auth;
  // The acquiring PAT (fabricd/native path) rides the Authorization bearer slot.
  if (opts.bearer !== undefined) headers["authorization"] = `Bearer ${opts.bearer}`;
  const method = opts.method ?? "POST";
  const init: RequestInit = { method, headers };
  if (method === "POST") {
    init.body =
      opts.body !== undefined
        ? JSON.stringify(opts.body)
        : JSON.stringify({
            job_id: JOB_ID,
            repo_full_name: REPO_FULL_NAME,
            installation_id: INSTALLATION_ID,
          });
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

/** Build the WP2 mint body, overriding fields per test (distinct job_id ⇒ own throttle). */
function mintBody(over: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    job_id: JOB_ID,
    repo_full_name: REPO_FULL_NAME,
    installation_id: INSTALLATION_ID,
    ...over,
  };
}

describe("POST /internal/v1/runner/mint — fabricd/native path (installation_id absent → PAT-introspection tenant)", () => {
  const FABRIC_KEY = "test-fabric-introspect-key-0123456789"; // ≥32 chars

  // _system DO stub that serves BOTH the container introspect (tenant resolution
  // for the fabricd path) AND the mint, routed by path — mirrors the real DO
  // fronting the container for both routes.
  function introspectMintNamespace(
    introspect: { status?: number; body?: unknown },
    mintCaptured: { req?: Request },
  ): DurableObjectNamespace {
    const stub = {
      fetch: async (req: Request): Promise<Response> => {
        const { pathname } = new URL(req.url);
        if (pathname === "/internal/v1/auth/introspect") {
          return new Response(JSON.stringify(introspect.body ?? { valid: false }), {
            status: introspect.status ?? 200,
            headers: { "Content-Type": "application/json" },
          });
        }
        mintCaptured.req = req;
        return new Response(JSON.stringify(CANNED_MINT), {
          status: 200,
          headers: { "Content-Type": "application/json" },
        });
      },
    };
    const ns = {
      idFromName: (_n: string) => ({ toString: () => "stub-id" }),
      get: (_id: unknown) => stub,
      idFromString: (_s: string) => ({ toString: () => "stub-id" }),
      newUniqueId: () => ({ toString: () => "stub-unique" }),
      jurisdiction: (_j: string) => ns,
    } as unknown as DurableObjectNamespace;
    return ns;
  }

  function fabricdEnv(
    introspect: { status?: number; body?: unknown },
    mintCaptured: { req?: Request },
    over: { allowlisted?: Set<string>; entitled?: Map<string, number> } = {},
  ): Env {
    const base = makeAuthorizedEnv({ withRunnerMintKey: true, ...over });
    return {
      ...base,
      CORELINK_SERVER: introspectMintNamespace(introspect, mintCaptured),
      FABRIC_INTROSPECT_AUTH_KEY: FABRIC_KEY,
    } as Env;
  }

  it("mints with the tenant from the acquiring PAT when installation_id is absent", async () => {
    const mintCaptured: { req?: Request } = {};
    const env = fabricdEnv({ body: { valid: true, tenant_id: TENANT } }, mintCaptured);
    const resp = await mintFetch(env, {
      auth: RUNNER_MINT_KEY,
      bearer: "corelink_pat_acquiring.secret.sig",
      body: { job_id: "job-fabricd-1", repo_full_name: REPO_FULL_NAME }, // NO installation_id
    });
    expect(resp.status).toBe(200);
    // Mint scoped to the INTROSPECTED tenant (server-derived), never a body value.
    const mintReqBody = (await mintCaptured.req!.json()) as Record<string, unknown>;
    expect(mintReqBody["tenant_id"]).toBe(TENANT);
  });

  it("401 when installation_id AND the acquiring-PAT bearer are both absent", async () => {
    const env = fabricdEnv({ body: { valid: true, tenant_id: TENANT } }, {});
    const resp = await mintFetch(env, {
      auth: RUNNER_MINT_KEY,
      body: { job_id: "job-fabricd-2", repo_full_name: REPO_FULL_NAME },
    });
    expect(resp.status).toBe(401);
  });

  it("403 when the acquiring PAT is invalid (introspect valid:false — no tenant)", async () => {
    const env = fabricdEnv({ body: { valid: false } }, {});
    const resp = await mintFetch(env, {
      auth: RUNNER_MINT_KEY,
      bearer: "corelink_pat_forged.secret.sig",
      body: { job_id: "job-fabricd-3", repo_full_name: REPO_FULL_NAME },
    });
    expect(resp.status).toBe(403);
  });

  it("403 when introspect is unavailable (non-200 → fail-CLOSED)", async () => {
    const env = fabricdEnv({ status: 503, body: { error: "store down" } }, {});
    const resp = await mintFetch(env, {
      auth: RUNNER_MINT_KEY,
      bearer: "corelink_pat_x.secret.sig",
      body: { job_id: "job-fabricd-4", repo_full_name: REPO_FULL_NAME },
    });
    expect(resp.status).toBe(403);
  });

  it("a body-named tenant is IGNORED — no `tenant`/`owner_tenant` field is ever a source", async () => {
    // With no installation_id and no bearer, the ONLY outcome is 401; a named
    // tenant in the body is never honored (the single-tenant hole stays closed).
    const env = fabricdEnv({ body: { valid: true, tenant_id: TENANT } }, {});
    const resp = await mintFetch(env, {
      auth: RUNNER_MINT_KEY,
      body: {
        job_id: "job-fabricd-5",
        repo_full_name: REPO_FULL_NAME,
        tenant: "evil-tenant",
        owner_tenant: "evil-tenant",
      },
    });
    expect(resp.status).toBe(401);
  });

  it("a valid PAT for tenant A cannot mint for a repo allowlisted only to tenant B", async () => {
    // introspect resolves A; the repo is allowlisted/entitled to B only → the
    // allowlist gate denies A (403). The PAT-derived tenant is still fully gated.
    const A = TENANT;
    const B = "99999999-9999-9999-9999-999999999999";
    const env = fabricdEnv(
      { body: { valid: true, tenant_id: A } },
      {},
      {
        allowlisted: new Set([`${B} ${REPO_FULL_NAME}`]),
        entitled: new Map([[B, MAX_CONCURRENCY]]),
      },
    );
    const resp = await mintFetch(env, {
      auth: RUNNER_MINT_KEY,
      bearer: "corelink_pat_tenantA.secret.sig",
      body: { job_id: "job-fabricd-6", repo_full_name: REPO_FULL_NAME },
    });
    expect(resp.status).toBe(403);
  });

  it("installation_id path is unchanged (regression): tenant from the map, bearer ignored", async () => {
    const mintCaptured: { req?: Request } = {};
    // The introspect (were it consulted) would return a DIFFERENT tenant — prove
    // the map path wins and the bearer is never introspected when installation_id present.
    const env = fabricdEnv({ body: { valid: true, tenant_id: "must-not-be-used" } }, mintCaptured);
    const resp = await mintFetch(env, {
      auth: RUNNER_MINT_KEY,
      bearer: "corelink_pat_ignored.secret.sig",
      body: { job_id: "job-fabricd-7", repo_full_name: REPO_FULL_NAME, installation_id: INSTALLATION_ID },
    });
    expect(resp.status).toBe(200);
    const mintReqBody = (await mintCaptured.req!.json()) as Record<string, unknown>;
    expect(mintReqBody["tenant_id"]).toBe(TENANT); // from the installation map
  });


// ── WP-F1: revoke KV-deletes the edge L2 entry (`patrow:<token_id>`) ─────────

describe("POST /internal/v1/runner/revoke — WP-F1 L2 KV invalidation", () => {
  it("(f1.1) successful revoke deletes patrow:<token_id> from METADATA_KV", async () => {
    const kvDeleted: string[] = [];
    const env = makeEnv({ revokeTokenId: "tok_abc", kvDeleted });
    const resp = await revokeFetch(env, {
      auth: INTERNAL_KEY,
      body: { pat_id: CANNED_MINT.pat_id },
    });
    expect(resp.status).toBe(200);
    expect(kvDeleted).toEqual(["patrow:tok_abc"]);
  });

  it("(f1.2) unknown pat_id → 200 idempotent, NO KV delete", async () => {
    const kvDeleted: string[] = [];
    const env = makeEnv({ revokeTokenId: null, kvDeleted });
    const resp = await revokeFetch(env, {
      auth: INTERNAL_KEY,
      body: { pat_id: "does-not-exist" },
    });
    expect(resp.status).toBe(200);
    expect(kvDeleted).toEqual([]);
  });

  it("(f1.3) re-revoke (already revoked) → 200, empty RETURNING, no KV delete", async () => {
    const kvDeleted: string[] = [];
    const env = makeEnv({ revokeTokenId: null, kvDeleted });
    const resp = await revokeFetch(env, {
      auth: INTERNAL_KEY,
      body: { pat_id: CANNED_MINT.pat_id, owner_tenant: TENANT },
    });
    expect(resp.status).toBe(200);
    expect(kvDeleted).toEqual([]);
  });

  it("(f1.4) both branches (with/without owner_tenant) delete the KV key", async () => {
    for (const body of [
      { pat_id: CANNED_MINT.pat_id, owner_tenant: TENANT },
      { pat_id: CANNED_MINT.pat_id },
    ]) {
      const kvDeleted: string[] = [];
      const env = makeEnv({ revokeTokenId: "tok_both", kvDeleted });
      const resp = await revokeFetch(env, { auth: INTERNAL_KEY, body });
      expect(resp.status).toBe(200);
      expect(kvDeleted).toEqual(["patrow:tok_both"]);
    }
  });

  it("(f1.5) KV unavailable → revoke STILL 200 and D1 still updated", async () => {
    const revokeCapture: { binds?: unknown[] } = {};
    const env = makeEnv({ revokeCapture, revokeTokenId: "tok_x", kvThrow: true });
    const resp = await revokeFetch(env, {
      auth: INTERNAL_KEY,
      body: { pat_id: CANNED_MINT.pat_id },
    });
    expect(resp.status).toBe(200);
    // D1 write still happened (binds captured by the fake's first()).
    expect(revokeCapture.binds).toBeDefined();
    const body = (await resp.json()) as Record<string, unknown>;
    expect(body["revoked"]).toBe(true);
  });

  it("(f1.6) no METADATA_KV binding at all → 200, no crash", async () => {
    const env = makeEnv({ revokeTokenId: "tok_y" }); // kvDeleted/kvThrow unset → binding undefined
    const resp = await revokeFetch(env, {
      auth: INTERNAL_KEY,
      body: { pat_id: CANNED_MINT.pat_id },
    });
    expect(resp.status).toBe(200);
  });
});
});
