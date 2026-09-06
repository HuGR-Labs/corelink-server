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

  it("(2026-07-08 contract) owner_tenant is OPTIONAL: absent → 200, revoke by pat_id alone", async () => {
    // The 2026-07-08 owner-ratified runners contract SUPERSEDES the 2026-06-21
    // REV-S2 mandatory requirement (see runner_mint.ts handleRunnerRevoke): the
    // dispatcher's frozen revoke body is `{pat_id}` only. Absent owner_tenant →
    // revoke by pat_id alone (the pat_id IS the capability; possessing it already
    // permits the revoke, and the surface is internal-auth gated). The UPDATE is
    // bound with (now_ms, pat_id) and carries NO tenant predicate.
    const revokeCapture: { binds?: unknown[] } = {};
    const env = makeEnv({ revokeCapture });
    const resp = await revokeFetch(env, { auth: INTERNAL_KEY, body: { pat_id: CANNED_MINT.pat_id } });
    expect(resp.status).toBe(200);
    expect(revokeCapture.binds).toBeDefined();
    expect(revokeCapture.binds![1]).toBe(CANNED_MINT.pat_id);
    // No tenant predicate is bound when owner_tenant is absent (2 binds, not 3).
    expect(revokeCapture.binds!.length).toBe(2);
  });

  it("405 on a non-POST method", async () => {
    const env = makeEnv({});
    const resp = await revokeFetch(env, { auth: INTERNAL_KEY, method: "GET" });
    expect(resp.status).toBe(405);
  });

  // ── #67 fast-follow (PR #718): owner_tenant-OPTIONAL emits TWO distinct UPDATE
  // shapes. The bind-count tests above pin the ARITY; these pin the exact WHERE
  // clause each code path executes, so the tenant-scoped vs by-pat_id-alone SQL
  // can never silently converge. Distinct pat_ids/owner_tenant from the tests
  // above (no throttle/state pollution per repo convention).
  const REVOKE_SCOPED_PAT_ID = "77777777-8888-9999-aaaa-bbbbbbbbb701";
  const REVOKE_SCOPED_TENANT = "cccccccc-dddd-eeee-ffff-000000000067";
  const REVOKE_BARE_PAT_ID = "77777777-8888-9999-aaaa-bbbbbbbbb702";
  const SQL_TENANT_SCOPED =
    "UPDATE pat SET revoked_at_ms = ?1 WHERE pat_id = ?2 AND tenant_id = ?3 AND revoked_at_ms IS NULL RETURNING token_id";
  const SQL_BY_PAT_ID_ALONE =
    "UPDATE pat SET revoked_at_ms = ?1 WHERE pat_id = ?2 AND revoked_at_ms IS NULL RETURNING token_id";

  it("(#67) owner_tenant PRESENT → executes the TENANT-SCOPED UPDATE (exact SQL + binds)", async () => {
    const revokeCapture: { sql?: string; binds?: unknown[] } = {};
    const env = makeEnv({ revokeCapture });
    const resp = await revokeFetch(env, {
      auth: INTERNAL_KEY,
      body: { pat_id: REVOKE_SCOPED_PAT_ID, owner_tenant: REVOKE_SCOPED_TENANT },
    });
    expect(resp.status).toBe(200);
    const body = (await resp.json()) as Record<string, unknown>;
    expect(body["pat_id"]).toBe(REVOKE_SCOPED_PAT_ID);
    expect(body["revoked"]).toBe(true);
    // EXACT SQL — carries the `AND tenant_id = ?3` predicate (defense-in-depth).
    expect(revokeCapture.sql).toBe(SQL_TENANT_SCOPED);
    // Binds: (now_ms, pat_id, owner_tenant) — 3 params, tenant last.
    expect(revokeCapture.binds).toBeDefined();
    expect(revokeCapture.binds!.length).toBe(3);
    expect(typeof revokeCapture.binds![0]).toBe("number");
    expect(revokeCapture.binds![1]).toBe(REVOKE_SCOPED_PAT_ID);
    expect(revokeCapture.binds![2]).toBe(REVOKE_SCOPED_TENANT);
  });

  it("(#67) owner_tenant ABSENT → executes the BY-PAT_ID-ALONE UPDATE (exact SQL + binds, no tenant predicate)", async () => {
    const revokeCapture: { sql?: string; binds?: unknown[] } = {};
    const env = makeEnv({ revokeCapture });
    const resp = await revokeFetch(env, {
      auth: INTERNAL_KEY,
      body: { pat_id: REVOKE_BARE_PAT_ID }, // 2026-07-08 frozen dispatcher body: {pat_id} only
    });
    expect(resp.status).toBe(200);
    const body = (await resp.json()) as Record<string, unknown>;
    expect(body["pat_id"]).toBe(REVOKE_BARE_PAT_ID);
    expect(body["revoked"]).toBe(true);
    // EXACT SQL — NO `tenant_id` predicate; the pat_id alone identifies the row.
    expect(revokeCapture.sql).toBe(SQL_BY_PAT_ID_ALONE);
    expect(revokeCapture.sql!).not.toContain("tenant_id");
    // Binds: (now_ms, pat_id) — exactly 2 params, no tenant.
    expect(revokeCapture.binds).toBeDefined();
    expect(revokeCapture.binds!.length).toBe(2);
    expect(typeof revokeCapture.binds![0]).toBe("number");
    expect(revokeCapture.binds![1]).toBe(REVOKE_BARE_PAT_ID);
  });

  it("(#67) the two owner_tenant paths emit DIFFERENT WHERE clauses (no silent convergence)", async () => {
    const scopedCap: { sql?: string; binds?: unknown[] } = {};
    const bareCap: { sql?: string; binds?: unknown[] } = {};
    await revokeFetch(makeEnv({ revokeCapture: scopedCap }), {
      auth: INTERNAL_KEY,
      body: { pat_id: REVOKE_SCOPED_PAT_ID, owner_tenant: REVOKE_SCOPED_TENANT },
    });
    await revokeFetch(makeEnv({ revokeCapture: bareCap }), {
      auth: INTERNAL_KEY,
      body: { pat_id: REVOKE_BARE_PAT_ID },
    });
    expect(scopedCap.sql).not.toBe(bareCap.sql);
    expect(scopedCap.sql).toContain("tenant_id");
    expect(bareCap.sql).not.toContain("tenant_id");
  });
});
describe("M22(b) — per-tenant runner mint ceiling composes with the per-job throttle", () => {
  // Unique tenant + installation so the per-tenant throttle key is FRESH: the
  // in-memory backstop map is module-scoped, and we drive the DURABLE per-tenant
  // count deterministically through the mock below.
  const M22_TENANT = "22222222-aaaa-bbbb-cccc-dddddddddd22";
  const M22_INSTALL = "gh-install-m22";
  const M22_REPO = "acme/m22";

  /**
   * CONFIG_DB that (a) authorizes the world for M22_TENANT and (b) returns a
   * per-KEY INCREMENTING throttle count. The per-TENANT ceiling key (same every
   * call) climbs 1,2,3,…; each DISTINCT per-job key is seen once (count 1) so the
   * per-job 10/window throttle NEVER fires — proving the tenant ceiling is what
   * catches a cross-job_id storm.
   */
  function countingDb(maxConcurrency: number, throttleCounts: Map<string, number>): D1Database {
    return {
      prepare: (sql: string) => ({
        bind: (...args: unknown[]) => ({
          first: async <T>() => {
            if (sql.includes("tenant_gh_installation_map")) {
              return (args[0] === M22_INSTALL ? { tenant_id: M22_TENANT } : null) as T | null;
            }
            if (sql.includes("tenant_offboarding_state")) return null as T | null;
            if (sql.includes("runner_repo_allowlist")) {
              return (args[0] === M22_TENANT && args[1] === M22_REPO ? { "1": 1 } : null) as T | null;
            }
            if (sql.includes("runners_entitlement")) {
              // This fixture's tenant has NO metered compute ceiling, so the
              // NULLABLE 0072 column reads null — exactly what D1 returns for a
              // row that never had one.
              return (args[0] === M22_TENANT
                ? { max_concurrency: maxConcurrency, max_vcpu_h: null }
                : null) as T | null;
            }
            if (sql.includes("session_exchange_throttle")) {
              const key = args[0] as string;
              const n = (throttleCounts.get(key) ?? 0) + 1;
              throttleCounts.set(key, n);
              return { count: n } as T;
            }
            return null as T | null;
          },
          run: async () => ({ success: true, meta: { changes: 1 } }) as unknown as D1Result,
        }),
      }),
    } as unknown as D1Database;
  }

  function m22Env(maxConcurrency: number, throttleCounts: Map<string, number>): Env {
    return {
      CORELINK_SERVER: makeMintNamespace({}),
      ENVIRONMENT: "test",
      CONFIG_DB: countingDb(maxConcurrency, throttleCounts),
      CORELINK_INTERNAL_AUTH_KEY: INTERNAL_KEY,
      CORELINK_RUNNER_MINT_AUTH_KEY: undefined,
    } as Env;
  }

  // ceiling = max(max_concurrency * K(=2), FLOOR(=8)). max_concurrency=6 → 12,
  // which is > the FLOOR, so this exercises the SCALING arm (grows with entitlement).
  const MAX_CONC = 6;
  const CEILING = 12;

  it("(5) the (ceiling+1)th mint 429s though every job_id stayed under its OWN 10/window", async () => {
    const throttleCounts = new Map<string, number>();
    // CEILING mints, each a DISTINCT job_id ⇒ per-job throttle count is 1 each
    // (never fires) — yet the shared per-tenant key climbs to CEILING.
    for (let i = 1; i <= CEILING; i++) {
      const env = m22Env(MAX_CONC, throttleCounts);
      const resp = await mintFetch(env, {
        auth: INTERNAL_KEY,
        body: { job_id: `m22-job-${i}`, repo_full_name: M22_REPO, installation_id: M22_INSTALL },
      });
      expect(resp.status).toBe(200); // under the per-tenant ceiling → mints
    }
    // The next mint (still a fresh job_id, still under its own per-job cap) trips the
    // per-tenant ceiling → 429. Composition: per-job OK, per-tenant NOT OK.
    const env = m22Env(MAX_CONC, throttleCounts);
    const resp = await mintFetch(env, {
      auth: INTERNAL_KEY,
      body: { job_id: `m22-job-${CEILING + 1}`, repo_full_name: M22_REPO, installation_id: M22_INSTALL },
    });
    expect(resp.status).toBe(429);
    const body = (await resp.json()) as Record<string, unknown>;
    expect(body["error"]).toBe("TOO_MANY_REQUESTS");
  });

  it("(5-control) fewer mints than the ceiling all succeed — legit fan-out is NEVER false-throttled", async () => {
    const throttleCounts = new Map<string, number>(); // fresh tenant counter
    for (let i = 1; i <= CEILING - 4; i++) {
      const env = m22Env(MAX_CONC, throttleCounts);
      const resp = await mintFetch(env, {
        auth: INTERNAL_KEY,
        body: { job_id: `m22-ctrl-${i}`, repo_full_name: M22_REPO, installation_id: M22_INSTALL },
      });
      expect(resp.status).toBe(200);
    }
  });

  it("(5-scaling) the ceiling GROWS with max_concurrency (a bigger plan tolerates more mints)", async () => {
    // max_concurrency=20 → ceiling=40. The 13th mint (which 429'd at MAX_CONC=6)
    // now succeeds, proving the ceiling scales off entitlement, not a fixed number.
    const throttleCounts = new Map<string, number>();
    for (let i = 1; i <= CEILING + 1; i++) {
      const env = m22Env(20, throttleCounts);
      const resp = await mintFetch(env, {
        auth: INTERNAL_KEY,
        body: { job_id: `m22-big-${i}`, repo_full_name: M22_REPO, installation_id: M22_INSTALL },
      });
      expect(resp.status).toBe(200); // 13 mints all fit under the ceiling of 40
    }
  });
});

// ── 5b/5c/5d authz reads batch into ONE round trip (latency win, behaviour
// pinned). The existing makeConfigDb doubles in this file do NOT implement
// `db.batch`, so every earlier test in this file exercises the
// `typeof ... batch !== "function"` fallback path automatically — the
// sequential awaits are unchanged for those doubles. The cases below
// construct a SEPARATE double that DOES implement `batch` and pin:
//   (1) the round-trip COUNT: one `db.batch` call, not three `prepare().first()`s;
//   (2) PRECEDENCE: a tenant that is BOTH offboarding AND not allowlisted still
//       denies identically (5b is evaluated before 5c before 5d);
//   (3) the fallback path is taken when the double omits `batch` (the existing
//       fully-authorized case, asserted explicitly to lock the guard);
//   (4) the existing entitlement cases (numeric max_concurrency; NULL max_vcpu_h;
//       positive max_vcpu_h) survive the batched rewire unchanged.
describe("POST /internal/v1/runner/mint — 5b/5c/5d authz reads in ONE batch", () => {
  // Routing-equivalent of makeConfigDb PLUS a `batch` implementation that
  // records every submitted SQL and resolves each statement through the same
  // per-SQL `first()` switch the serial path uses — so a batched read and a
  // serial read of the same statement cannot disagree. `__sql` is attached to
  // the bound object so the spy can name the statements it sees.
  function makeBatchConfigDb(opts: {
    mapped?: Map<string, string>;
    suspended?: Set<string>;
    allowlisted?: Set<string>;
    entitled?: Map<string, number>;
    vcpuCeilings?: Map<string, number | null>;
  }): { db: D1Database; batches: string[][]; serialFirsts: string[] } {
    const mapped = opts.mapped ?? new Map([[INSTALLATION_ID, TENANT]]);
    const suspended = opts.suspended ?? new Set<string>();
    const allowlisted = opts.allowlisted ?? new Set([`${TENANT}${REPO_FULL_NAME}`]);
    const entitled = opts.entitled ?? new Map([[TENANT, MAX_CONCURRENCY]]);
    const vcpuCeilings = opts.vcpuCeilings ?? new Map<string, number | null>();
    const batches: string[][] = [];
    const serialFirsts: string[] = [];

    const rowFor = (sql: string, args: unknown[]) => {
      if (sql.includes("tenant_gh_installation_map")) {
        const installationId = args[0] as string;
        const tenantId = mapped.get(installationId);
        return tenantId !== undefined ? { tenant_id: tenantId } : null;
      }
      if (sql.includes("tenant_offboarding_state")) {
        const tenantId = args[0] as string;
        return suspended.has(tenantId) ? { "1": 1 } : null;
      }
      if (sql.includes("runner_repo_allowlist")) {
        const tenantId = args[0] as string;
        const repo = args[1] as string;
        return allowlisted.has(`${tenantId}${repo}`) ? { "1": 1 } : null;
      }
      if (sql.includes("runners_entitlement")) {
        const tenantId = args[0] as string;
        const mc = entitled.get(tenantId);
        return mc !== undefined
          ? {
              max_concurrency: mc,
              max_vcpu_h: vcpuCeilings.has(tenantId)
                ? (vcpuCeilings.get(tenantId) ?? null)
                : null,
            }
          : null;
      }
      return null;
    };

    // `batchViaFirst` resolves a batch THROUGH each statement's own `first()`
    // (deliberately — see `d1_batch_mock.ts`: one routing table, so the serial
    // and batched paths can never answer differently). That makes a naive
    // "`first()` was called" recorder unable to tell a batched read from a
    // leaked serial one. So record into `serialFirsts` only while NOT inside a
    // batch; `batch` flips this for the duration of its own resolution.
    let inBatch = false;
    const db = {
      prepare: (sql: string) => ({
        bind: (...args: unknown[]) => ({
          __sql: sql,
          first: async <T>() => {
            if (!inBatch) serialFirsts.push(sql);
            return rowFor(sql, args) as T | null;
          },
          run: async () => ({ success: true, meta: { changes: 1 } }) as unknown as D1Result,
        }),
      }),
      batch: async (statements: Array<{ __sql?: string; first: <T>() => Promise<T | null> }>) => {
        batches.push(statements.map((s) => s.__sql ?? ""));
        // Resolve through the SAME per-SQL routing as the serial path — a mock
        // that diverged between the two would let this test lie about precedence.
        inBatch = true;
        try {
          const results = await batchViaFirst()(statements);
          return results as Array<{ results?: unknown[] }>;
        } finally {
          inBatch = false;
        }
      },
    } as unknown as D1Database;
    return { db, batches, serialFirsts };
  }

  function makeBatchEnv(
    opts: Parameters<typeof makeBatchConfigDb>[0],
    captured: { req?: Request } = {},
  ): { env: Env; batches: string[][]; serialFirsts: string[] } {
    const { db, batches, serialFirsts } = makeBatchConfigDb(opts);
    return {
      env: {
        CORELINK_SERVER: makeMintNamespace(captured),
        ENVIRONMENT: "test",
        CONFIG_DB: db,
        CORELINK_INTERNAL_AUTH_KEY: INTERNAL_KEY,
        CORELINK_RUNNER_MINT_AUTH_KEY: undefined,
      } as Env,
      batches,
      serialFirsts,
    };
  }

  it("(batch) the three authz reads travel as ONE db.batch, not three serial .first()s", async () => {
    // The load-bearing win: with a batch-capable double, the authz block
    // collapses 5b + 5c + 5d into a single round trip. Anything other than
    // exactly one batch call is a regression (timing is not asserted — flaky
    // on this hardware, per the change brief).
    const captured: { req?: Request } = {};
    const { env, batches, serialFirsts } = makeBatchEnv({}, captured);
    const resp = await mintFetch(env, {
      auth: INTERNAL_KEY,
      body: mintBody({ job_id: `${JOB_ID}-batch-once` }),
    });
    expect(resp.status).toBe(200);
    expect(
      batches.length,
      `expected exactly ONE db.batch call carrying the three authz reads, saw ${batches.length}: ${JSON.stringify(batches)}`,
    ).toBe(1);
    expect(batches[0]).toHaveLength(3);
    expect(batches[0]![0]).toContain("tenant_offboarding_state");
    expect(batches[0]![1]).toContain("runner_repo_allowlist");
    expect(batches[0]![2]).toContain("runners_entitlement");
    // And NO statement reached the database via the serial path (the .first()
    // on the bound statement, when used by db.batch, is not a "serial round
    // trip" — the mock only records the SQL on the .first() path so the test
    // can still tell a leaked authz read from a batched one).
    const authzSerial = serialFirsts.filter(
      (s) =>
        s.includes("tenant_offboarding_state") ||
        s.includes("runner_repo_allowlist") ||
        s.includes("runners_entitlement"),
    );
    expect(
      authzSerial,
      `an authz read escaped the batch onto a serial round trip: ${JSON.stringify(authzSerial)}`,
    ).toHaveLength(0);
    expect(captured.req).toBeDefined(); // the mint still happened
  });

  it("(batch) PRECEDENCE — offboarding AND not-allowlisted still denies (5b wins, behaves identically)", async () => {
    // A tenant that is BOTH suspended AND not on the repo allowlist: the FIRST
    // failing gate in the original 5b -> 5c -> 5d order decides, so this
    // remains a 403. The test pins BOTH that the outcome is unchanged AND
    // that the batch still carries all three statements (no short-circuit
    // dropping 5c/5d — the driver fetches every row positionally, the
    // precedence is enforced in the consumer, not in the SQL).
    const captured: { req?: Request } = {};
    const { env, batches } = makeBatchEnv(
      {
        suspended: new Set([TENANT]), // 5b FAILS
        allowlisted: new Set(), // 5c ALSO fails — 5b must still win
      },
      captured,
    );
    const resp = await mintFetch(env, {
      auth: INTERNAL_KEY,
      body: mintBody({ job_id: `${JOB_ID}-prec` }),
    });
    expect(resp.status).toBe(403);
    const body = (await resp.json()) as Record<string, unknown>;
    expect(body["error"]).toBe("FORBIDDEN");
    expect(body["message"]).toBe("runner mint unauthorized");
    expect(captured.req).toBeUndefined(); // never minted
    // The batch was still issued with all three statements (precedence is a
    // consumer-side decision; the row data is what changes the verdict).
    expect(batches).toHaveLength(1);
    expect(batches[0]).toHaveLength(3);
  });

  it("(batch) PRECEDENCE — not-allowlisted but NOT offboarding denies on 5c (5b passed, 5c fails)", async () => {
    // The mirror image: 5b passes, 5c fails. The batched path must produce
    // the same 403 the serial path always did — so 5c is evaluated AFTER 5b
    // in the consumer, and 5c is the gate that decides.
    const captured: { req?: Request } = {};
    const { env } = makeBatchEnv({ allowlisted: new Set() }, captured);
    const resp = await mintFetch(env, {
      auth: INTERNAL_KEY,
      body: mintBody({ job_id: `${JOB_ID}-prec-5c` }),
    });
    expect(resp.status).toBe(403);
    expect(captured.req).toBeUndefined();
  });

  it("(batch) entitlement cases survive the rewire — numeric max_concurrency, NULL max_vcpu_h", async () => {
    // The fully-authorized world through the batched path: same shape as the
    // serial-path test in the mint describe (200, max_concurrency forwarded,
    // max_vcpu_h omitted because the column read null). If the row-pulling
    // logic in the batched path drifted, this would fail.
    const captured: { req?: Request } = {};
    const { env } = makeBatchEnv({}, captured);
    const resp = await mintFetch(env, {
      auth: INTERNAL_KEY,
      body: mintBody({ job_id: `${JOB_ID}-entitlement` }),
    });
    expect(resp.status).toBe(200);
    const body = (await resp.json()) as Record<string, unknown>;
    expect(body["max_concurrency"]).toBe(MAX_CONCURRENCY);
    expect(Object.prototype.hasOwnProperty.call(body, "max_vcpu_h")).toBe(false);
  });

  it("(batch) a POSITIVE max_vcpu_h is forwarded through the batched path (no regression on 0072)", async () => {
    const captured: { req?: Request } = {};
    const { env } = makeBatchEnv({ vcpuCeilings: new Map([[TENANT, 100]]) }, captured);
    const resp = await mintFetch(env, {
      auth: INTERNAL_KEY,
      body: mintBody({ job_id: `${JOB_ID}-vcpu-fwd` }),
    });
    expect(resp.status).toBe(200);
    const body = (await resp.json()) as Record<string, unknown>;
    expect(body["max_vcpu_h"]).toBe(100);
  });

  it("(fallback) a double WITHOUT `db.batch` still authorizes via three sequential .first()s", async () => {
    // The existing makeConfigDb in this file does NOT implement `batch`, so
    // `typeof env.CONFIG_DB.batch !== "function"` is true and the code falls
    // back to the three sequential reads. Every earlier test in this file
    // already exercises that path implicitly; this case pins it explicitly
    // so a future refactor cannot quietly remove the guard.
    const captured: { req?: Request } = {};
    const env = makeAuthorizedEnv({ captured }); // no `batch` on this double
    // The double has no `batch` method — the guard MUST take the fallback.
    expect(typeof (env.CONFIG_DB as { batch?: unknown }).batch).toBe("undefined");
    // And the sequential path still produces a successful mint.
    const resp = await mintFetch(env, {
      auth: INTERNAL_KEY,
      body: mintBody({ job_id: `${JOB_ID}-fallback` }),
    });
    expect(resp.status).toBe(200);
    expect(((await resp.json()) as Record<string, unknown>)["tenant"]).toBe(TENANT);
    expect(captured.req).toBeDefined();
  });
});
