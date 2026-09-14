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

import { describe, it, expect, vi } from "vitest";
import type { D1Database, DurableObjectNamespace } from "@cloudflare/workers-types";
import workerHandler from "../src/index.js";
import type { Env } from "../src/index.js";
import { batchViaFirst } from "./d1_batch_mock.js";

const INTERNAL_KEY = "test-internal-auth-key-0123456789"; // ≥32 chars
const RUNNER_MINT_KEY = "test-pat-mint-auth-key-0123456789ab"; // ≥32 chars, distinct
const PAT_MINT_KEY = "test-dedicated-pat-mint-key-0123456789";

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
  const db = {
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
        __args: args,
        __sql: sql,
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
    batch: async (statements: unknown[]) => statements.length === 2
      ? (statements[0] && typeof statements[0] === "object" && String((statements[0] as { __sql?: string }).__sql ?? "").includes("INSERT INTO pat") && opts.patInsertCapture
          ? (opts.patInsertCapture.sql = (statements[0] as { __sql: string }).__sql, opts.patInsertCapture.binds = (statements[0] as { __args?: unknown[] }).__args, statements.map(() => ({ success: true, meta: { changes: 1 } })))
          : statements.map(() => ({ success: true, meta: { changes: 1 } })))
      : Promise.all(statements.map(async (s) => ({ success: true, meta: { changes: 1 }, results: [await (s as { first: <T>() => Promise<T | null> }).first()].filter(Boolean) }))),
  } as unknown as D1Database;
  return db;
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
      if (new URL(req.url).pathname === "/_do/runner-cleanup/prepare") {
        return new Response(null, { status: 204 });
      }
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
  withPatMintKey?: boolean;
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
    CORELINK_RUNNER_MINT_AUTH_KEY: opts.withRunnerMintKey === false ? undefined : RUNNER_MINT_KEY,
    CORELINK_PAT_MINT_AUTH_KEY: opts.withPatMintKey === false ? undefined : PAT_MINT_KEY,
    FABRIC_CREDENTIAL_AUTHORITY_URL: "https://fabric.test",
    FABRIC_CREDENTIAL_ISSUER_AUTH_KEY: "test-fabric-credential-issuer-key-012345",
  } as Env;
}

vi.stubGlobal("fetch", async (input: RequestInfo | URL) => {
  if (String(input).includes("/internal/v1/credentials/tenants/")) return new Response(JSON.stringify({ tenant_id: TENANT, generation: "1", suspended: false }), { status: 200 });
  return new Response(JSON.stringify(CANNED_MINT), { status: 200 });
});

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
  if (opts.auth !== undefined) headers["x-corelink-internal-auth"] = opts.auth === INTERNAL_KEY ? RUNNER_MINT_KEY : opts.auth;
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
  if (opts.auth !== undefined) headers["x-corelink-internal-auth"] = opts.auth === INTERNAL_KEY ? RUNNER_MINT_KEY : opts.auth;
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

describe("POST /internal/v1/runner/mint — D-9 runner PAT mint", () => {
  it("(a) 401 when the internal-auth header is missing (before any work)", async () => {
    const captured: { req?: Request } = {};
    const env = makeAuthorizedEnv({ captured });
    const resp = await mintFetch(env, {}); // no auth header
    expect(resp.status).toBe(401);
    expect(captured.req).toBeUndefined(); // never minted
  });

  it("(a) 401 when the internal-auth header is blank", async () => {
    const captured: { req?: Request } = {};
    const env = makeAuthorizedEnv({ captured });
    const resp = await mintFetch(env, { auth: "" });
    expect(resp.status).toBe(401);
    expect(captured.req).toBeUndefined();
  });

  it("(a) 503 (not 403) when NO internal-auth key is bound — the branch that ate every runner job", async () => {
    // 2026-08-02. This is the highest-consequence instance of the 403-vs-503
    // distinction in the codebase, so the reasoning lives here in full.
    //
    // The runner dispatcher (corelink-runners `deploy/cloudflare/src/lib.ts`,
    // `mintCasPat`) branches on the STATUS:
    //   403  ⇒ MintForbiddenError ⇒ hard deny ⇒ `driveSpawn` returns WITHOUT
    //          throwing ⇒ no dead-letter is recorded ⇒ and GitHub delivers
    //          `workflow_job.queued` exactly once ⇒ the job is lost FOREVER.
    //   5xx  ⇒ plain Error ⇒ fall open to a COLD spawn: the customer's job still
    //          RUNS, just without cache-warm. Slow, never broken.
    //
    // So under the old 403 a single unbound secret silently ate EVERY job in the
    // fleet, permanently — and the only signal was a `spawn_forbidden` counter
    // indistinguishable from ordinary "customer not entitled" traffic. That is a
    // config fault wearing an authz costume, and it is not hypothetical: a
    // rotation of CORELINK_RUNNER_MINT_AUTH_KEY touches three Workers and has
    // drifted before.
    //
    // Still fail-CLOSED — `captured.req` proves no mint was forwarded. Only the
    // ATTRIBUTION changed: 401 stays the caller's fault, 503 is ours.
    const captured: { req?: Request } = {};
    const env = makeAuthorizedEnv({ captured, withRunnerMintKey: false });
    const resp = await mintFetch(env, { auth: INTERNAL_KEY });
    expect(resp.status).toBe(503);
    // The load-bearing half: nothing was authorized, no PAT was minted.
    expect(captured.req).toBeUndefined();
  });

  it("(b) 200 + mint envelope (derived tenant + max_concurrency) for an authorized job", async () => {
    const captured: { req?: Request } = {};
    const env = makeAuthorizedEnv({ captured });
    const resp = await mintFetch(env, { auth: INTERNAL_KEY, body: mintBody({ job_id: `${JOB_ID}-happy` }) });

    expect(resp.status).toBe(200);
    const body = (await resp.json()) as Record<string, unknown>;
    expect(body["token_plaintext"]).toBe(CANNED_MINT.token_plaintext);
    expect(body["pat_id"]).toBe(CANNED_MINT.pat_id);
    expect(body["token_id"]).toBe(CANNED_MINT.token_id);
    expect(body["expires_ms"]).toBe(CANNED_MINT.expires_ms);
    expect(body["hash"]).toBeUndefined(); // never leak the Argon2id hash
    // WP2: tenant is the DERIVED tenant, and max_concurrency == the entitlement value.
    expect(body["tenant"]).toBe(TENANT);
    // The tenant in this fixture has NO metered compute ceiling on file, so the
    // additive `max_vcpu_h` field is OMITTED — not null, not 0. That keeps the
    // wire byte-identical for every tenant that had none, so an existing
    // consumer cannot tell this field was ever added.
    expect(body["max_vcpu_h"]).toBeUndefined();
    expect(Object.prototype.hasOwnProperty.call(body, "max_vcpu_h")).toBe(false);
    expect(body["max_concurrency"]).toBe(MAX_CONCURRENCY);

    // The mint was asked for the job-bounded TTL + DERIVED tenant + cas:rw scope.
    const mint = (await captured.req!.json()) as Record<string, unknown>;
    expect(mint["ttl_seconds"]).toBe(5400);
    expect(mint["tenant_id"]).toBe(TENANT);
    expect(mint["scopes"]).toBe("cas:rw");
    // principal_id is the SHA-256-derived UUID of job_id (NOT the raw job_id).
    expect(mint["principal_id"]).not.toBe(JOB_ID);
    expect(mint["principal_id"]).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-/);
  });

  it("(wp2) forwards max_vcpu_h when the tenant HAS a metered compute ceiling", async () => {
    // The whole point of threading this: the dispatcher can warn a customer as
    // they approach the monthly allowance instead of surprising them with
    // overage on the invoice. It is ADVISORY — never a gate. `max_concurrency`
    // is the only ceiling that denies a mint, and that is unchanged.
    const env = makeAuthorizedEnv({
      vcpuCeilings: new Map([[TENANT, 100]]), // runner_starter's 100 vCPU-h
    });
    const resp = await mintFetch(env, { auth: INTERNAL_KEY });
    expect(resp.status).toBe(200);
    const body = (await resp.json()) as Record<string, unknown>;
    expect(body["max_vcpu_h"]).toBe(100);
    // Still minted — an entitled tenant is NOT denied because a ceiling exists.
    expect(body["token_plaintext"]).toBeDefined();
    expect(body["max_concurrency"]).toBeDefined();
  });

  it("(wp2) a ZERO or NEGATIVE max_vcpu_h is treated as ABSENT, never forwarded", async () => {
    // A 0 would make the dispatcher divide by zero (or compute an infinite
    // percentage) and warn every tenant on their very first job; a negative is
    // data corruption. Neither is a real ceiling, so neither reaches the wire —
    // the tenant is simply "no metered ceiling on file", which is the honest
    // reading. Guarded at the SOURCE so no consumer has to re-derive it.
    for (const bad of [0, -5]) {
      const env = makeAuthorizedEnv({ vcpuCeilings: new Map([[TENANT, bad]]) });
      const resp = await mintFetch(env, {
        auth: INTERNAL_KEY,
        body: mintBody({ job_id: `${JOB_ID}-vcpu-${String(bad)}` }),
      });
      expect(resp.status).toBe(200); // still a legitimate mint
      const body = (await resp.json()) as Record<string, unknown>;
      expect(body["max_vcpu_h"]).toBeUndefined();
    }
  });

  it("(wp2) max_vcpu_h is ADVISORY — a tenant at any ceiling is still minted", async () => {
    // Pins the separation of concerns: this field must never become a gate here.
    // Enforcement (warn / charge overage) belongs to the dispatcher, which sees
    // consumption; the mint sees only entitlement and must not guess.
    const env = makeAuthorizedEnv({ vcpuCeilings: new Map([[TENANT, 1]]) });
    const resp = await mintFetch(env, { auth: INTERNAL_KEY });
    expect(resp.status).toBe(200);
    expect(((await resp.json()) as Record<string, unknown>)["max_vcpu_h"]).toBe(1);
  });

  it("(c) a caller-supplied ttl_seconds below the cap is passed through (lease-bound)", async () => {
    const captured: { req?: Request } = {};
    const env = makeAuthorizedEnv({ captured });
    const resp = await mintFetch(env, {
      auth: INTERNAL_KEY,
      body: mintBody({ job_id: `${JOB_ID}-ttl-pass`, ttl_seconds: 600 }),
    });
    expect(resp.status).toBe(200);
    const mint = (await captured.req!.json()) as Record<string, unknown>;
    expect(mint["ttl_seconds"]).toBe(600); // honored, the PAT expires with the lease
  });

  it("(c) a ttl_seconds ABOVE the cap is clamped down to 5400 (never extend)", async () => {
    const captured: { req?: Request } = {};
    const env = makeAuthorizedEnv({ captured });
    const resp = await mintFetch(env, {
      auth: INTERNAL_KEY,
      body: mintBody({ job_id: `${JOB_ID}-ttl-clamp`, ttl_seconds: 999999 }),
    });
    expect(resp.status).toBe(200);
    const mint = (await captured.req!.json()) as Record<string, unknown>;
    expect(mint["ttl_seconds"]).toBe(5400); // clamped to the 90-min cap
  });

  it("(c) ttl_seconds=0 is REFUSED 400 (the container maps 0 → no-expiry; never mint)", async () => {
    const captured: { req?: Request } = {};
    const env = makeAuthorizedEnv({ captured });
    const resp = await mintFetch(env, {
      auth: INTERNAL_KEY,
      body: mintBody({ ttl_seconds: 0 }),
    });
    expect(resp.status).toBe(400);
    expect(captured.req).toBeUndefined(); // never minted a non-expiring PAT
  });

  it("(c) a negative or non-integer ttl_seconds is REFUSED 400", async () => {
    for (const bad of [-1, 3.5, "600", null]) {
      const captured: { req?: Request } = {};
      const env = makeAuthorizedEnv({ captured });
      const resp = await mintFetch(env, {
        auth: INTERNAL_KEY,
        body: mintBody({ ttl_seconds: bad }),
      });
      expect(resp.status).toBe(400);
      expect(captured.req).toBeUndefined();
    }
  });

  it("(b) per-job principal UUID is STABLE across mints for the same job_id", async () => {
    const c1: { req?: Request } = {};
    const c2: { req?: Request } = {};
    const env1 = makeAuthorizedEnv({ captured: c1 });
    const env2 = makeAuthorizedEnv({ captured: c2 });
    await mintFetch(env1, { auth: INTERNAL_KEY, body: mintBody({ job_id: `${JOB_ID}-stable` }) });
    await mintFetch(env2, { auth: INTERNAL_KEY, body: mintBody({ job_id: `${JOB_ID}-stable` }) });
    const p1 = (await c1.req!.json()) as Record<string, unknown>;
    const p2 = (await c2.req!.json()) as Record<string, unknown>;
    expect(p1["principal_id"]).toBe(p2["principal_id"]);
  });

  it("(b) the per-consumer runner_mint key is accepted (shared key path independent)", async () => {
    const captured: { req?: Request } = {};
    const env = makeAuthorizedEnv({ captured, withRunnerMintKey: true });
    // Caller presents the dedicated runner_mint key, NOT the shared key.
    const resp = await mintFetch(env, { auth: RUNNER_MINT_KEY, body: mintBody({ job_id: `${JOB_ID}-runnerkey` }) });
    expect(resp.status).toBe(200);
    expect(captured.req).toBeDefined();
  });

  it("(b) when a dedicated runner_mint key is bound, the shared key is rejected", async () => {
    const captured: { req?: Request } = {};
    const env = makeAuthorizedEnv({ captured, withRunnerMintKey: true });
    const resp = await mintFetch(env, { auth: INTERNAL_KEY.replace("internal", "shared"), body: mintBody() });
    expect(resp.status).toBe(401);
    expect(captured.req).toBeUndefined();
  });

  // ── WP2 authz chokepoint: each failing check → the SAME generic 403 ─────────
  it("(wp2) 403 when the installation is UNMAPPED (no tenant derivable)", async () => {
    const captured: { req?: Request } = {};
    const env = makeAuthorizedEnv({ captured, mapped: new Map() }); // no installation→tenant
    const resp = await mintFetch(env, { auth: INTERNAL_KEY, body: mintBody({ job_id: `${JOB_ID}-unmapped` }) });
    expect(resp.status).toBe(403);
    const body = (await resp.json()) as Record<string, unknown>;
    expect(body["error"]).toBe("FORBIDDEN");
    expect(body["message"]).toBe("runner mint unauthorized");
    expect(captured.req).toBeUndefined(); // never minted
  });

  it("(wp2) 403 when the tenant is SUSPENDED (offboarding row present)", async () => {
    const captured: { req?: Request } = {};
    const env = makeAuthorizedEnv({ captured, suspended: new Set([TENANT]) });
    const resp = await mintFetch(env, { auth: INTERNAL_KEY, body: mintBody({ job_id: `${JOB_ID}-suspended` }) });
    expect(resp.status).toBe(403);
    const body = (await resp.json()) as Record<string, unknown>;
    expect(body["error"]).toBe("FORBIDDEN");
    expect(body["message"]).toBe("runner mint unauthorized");
    expect(captured.req).toBeUndefined();
  });

  it("(wp2) 403 when the repo is NOT on the allowlist", async () => {
    const captured: { req?: Request } = {};
    const env = makeAuthorizedEnv({ captured, allowlisted: new Set() }); // repo not allowlisted
    const resp = await mintFetch(env, { auth: INTERNAL_KEY, body: mintBody({ job_id: `${JOB_ID}-noallow` }) });
    expect(resp.status).toBe(403);
    const body = (await resp.json()) as Record<string, unknown>;
    expect(body["error"]).toBe("FORBIDDEN");
    expect(body["message"]).toBe("runner mint unauthorized");
    expect(captured.req).toBeUndefined();
  });

  it("(wp2) 403 when the tenant is NOT entitled to runners", async () => {
    const captured: { req?: Request } = {};
    const env = makeAuthorizedEnv({ captured, entitled: new Map() }); // no entitlement row
    const resp = await mintFetch(env, { auth: INTERNAL_KEY, body: mintBody({ job_id: `${JOB_ID}-noent` }) });
    expect(resp.status).toBe(403);
    const body = (await resp.json()) as Record<string, unknown>;
    expect(body["error"]).toBe("FORBIDDEN");
    expect(body["message"]).toBe("runner mint unauthorized");
    expect(captured.req).toBeUndefined();
  });

  it("(wp2) all four 403 bodies are BYTE-IDENTICAL (no oracle)", async () => {
    const bodies: string[] = [];
    for (const [i, override] of [
      { mapped: new Map<string, string>() },
      { suspended: new Set([TENANT]) },
      { allowlisted: new Set<string>() },
      { entitled: new Map<string, number>() },
    ].entries()) {
      const env = makeAuthorizedEnv({ ...override });
      const resp = await mintFetch(env, {
        auth: INTERNAL_KEY,
        body: mintBody({ job_id: `${JOB_ID}-oracle-${String(i)}` }),
      });
      expect(resp.status).toBe(403);
      // Normalize out the per-request request_id (unique per call by design);
      // the SECURITY-load-bearing fields (error, message) must be byte-identical
      // across all four — no oracle distinguishes WHICH authz check failed.
      const parsed = (await resp.json()) as Record<string, unknown>;
      bodies.push(JSON.stringify({ error: parsed["error"], message: parsed["message"] }));
    }
    expect(new Set(bodies).size).toBe(1);
    expect(JSON.parse(bodies[0]!)).toEqual({
      error: "FORBIDDEN",
      message: "runner mint unauthorized",
    });
  });

  it("a CONFIG fault (503) and a real authz DENIAL (403) are DISTINCT statuses — the dispatcher acts on the difference", async () => {
    // The invariant this file's two halves imply, asserted directly so a future
    // refactor cannot quietly collapse them back into one status.
    //
    // These are different KINDS of failure and the runner dispatcher does
    // different things with them:
    //   • 403 "runner mint unauthorized" — a verdict about THIS TENANT. Correct
    //     to treat as final: retrying will not make an unentitled tenant
    //     entitled, so the dispatcher hard-denies and drops the job.
    //   • 503 "runner mint unavailable"  — we could not evaluate authz at all.
    //     Retrying is exactly right, so the dispatcher falls open to a COLD
    //     spawn and the customer's job still runs.
    // Collapsing 503 into 403 makes every outage look like a fleet full of
    // unentitled customers, and silently drops every job (see case (a) above).

    // A genuine authz denial: keys bound, tenant simply not entitled.
    const denied = await mintFetch(makeAuthorizedEnv({ entitled: new Map<string, number>() }), {
      auth: INTERNAL_KEY,
      body: mintBody({ job_id: `${JOB_ID}-authz-denial` }),
    });

    // A config fault: the caller is fine, WE have no key bound.
    const broken = await mintFetch(makeAuthorizedEnv({ withRunnerMintKey: false }), {
      auth: INTERNAL_KEY,
      body: mintBody({ job_id: `${JOB_ID}-config-fault` }),
    });

    expect(denied.status).toBe(403);
    expect(broken.status).toBe(503);
    expect(denied.status).not.toBe(broken.status);

    // And they are distinguishable in the BODY too — a 4xx/5xx split is no use
    // if both sides say the same thing. (This does NOT weaken the no-oracle
    // property above: that one requires the four AUTHZ denials to be identical
    // to EACH OTHER, which they remain. An outage is not one of the four.)
    const deniedBody = (await denied.json()) as Record<string, unknown>;
    const brokenBody = (await broken.json()) as Record<string, unknown>;
    expect(deniedBody["error"]).toBe("FORBIDDEN");
    expect(brokenBody["error"]).toBe("SERVICE_UNAVAILABLE");
    expect(deniedBody["message"]).not.toBe(brokenBody["message"]);
  });

  it("503 when the ONWARD mint credential is unbound but the dispatcher's own key is fine (step 3, distinct from step 2)", async () => {
    // The runner-mint route has TWO independent secret preconditions and they
    // fail at DIFFERENT steps. Case (a) above covers step 2 (the dispatcher's own
    // gate, `requireConsumerAuth`). THIS covers step 3: the dedicated
    // CORELINK_RUNNER_MINT_AUTH_KEY is bound and the caller presents it
    // correctly, so authz succeeds — but the credential WE present onward to the
    // container's /_internal/pat/mint (CORELINK_PAT_MINT_AUTH_KEY, falling back
    // to the shared key) is missing.
    //
    // This is a realistic rotation state, not a contrived one: the three runner
    // secrets live on three different Workers and have drifted apart before.
    //
    // It needs its own case because a test driven through `withInternalKey:
    // false` alone NEVER REACHES step 3 — step 2 short-circuits first. That was
    // verified the hard way: an earlier version of this pin stayed GREEN when
    // step 3 was reverted to 403, because it was only ever exercising step 2.
    const captured: { req?: Request } = {};
    const env = makeAuthorizedEnv({
      captured,
      withPatMintKey: false, // onward mint credential is unbound
      withRunnerMintKey: true, // ...but the dispatcher's own gate IS armed
    });
    const resp = await mintFetch(env, { auth: RUNNER_MINT_KEY });
    expect(resp.status).toBe(503);
    const body = (await resp.json()) as Record<string, unknown>;
    expect(body["error"]).toBe("SERVICE_UNAVAILABLE");
    expect(body["message"]).toBe("runner mint unavailable");
    // Fail-CLOSED: no PAT was minted.
    expect(captured.req).toBeUndefined();
  });

  it("(d) 400 when admin scope is requested (least privilege — refused)", async () => {
    const captured: { req?: Request } = {};
    const env = makeAuthorizedEnv({ captured });
    const resp = await mintFetch(env, {
      auth: INTERNAL_KEY,
      body: mintBody({ scope: "admin" }),
    });
    expect(resp.status).toBe(400);
    expect(captured.req).toBeUndefined();
  });

  it("(d) 400 when owner scope is requested", async () => {
    const captured: { req?: Request } = {};
    const env = makeAuthorizedEnv({ captured });
    const resp = await mintFetch(env, {
      auth: INTERNAL_KEY,
      body: mintBody({ scope: "owner" }),
    });
    expect(resp.status).toBe(400);
    expect(captured.req).toBeUndefined();
  });

  it("401 (not 400) when installation_id is missing AND no acquiring-PAT bearer — installation_id is now OPTIONAL", async () => {
    // Frozen 2026-07-08: installation_id is optional. Absent + no bearer ⇒ no
    // tenant source ⇒ 401 (acquiring PAT required), NOT the old mandatory 400.
    const env = makeAuthorizedEnv({});
    const resp = await mintFetch(env, {
      auth: INTERNAL_KEY,
      body: { job_id: JOB_ID, repo_full_name: REPO_FULL_NAME },
    });
    expect(resp.status).toBe(401);
  });

  it("400 when installation_id is present but empty (malformed)", async () => {
    const env = makeAuthorizedEnv({});
    const resp = await mintFetch(env, {
      auth: INTERNAL_KEY,
      body: { job_id: JOB_ID, repo_full_name: REPO_FULL_NAME, installation_id: "" },
    });
    expect(resp.status).toBe(400);
  });

  it("400 when repo_full_name is missing", async () => {
    const env = makeAuthorizedEnv({});
    const resp = await mintFetch(env, {
      auth: INTERNAL_KEY,
      body: { job_id: JOB_ID, installation_id: INSTALLATION_ID },
    });
    expect(resp.status).toBe(400);
  });

  it("400 when job_id is missing", async () => {
    const env = makeAuthorizedEnv({});
    const resp = await mintFetch(env, {
      auth: INTERNAL_KEY,
      body: { repo_full_name: REPO_FULL_NAME, installation_id: INSTALLATION_ID },
    });
    expect(resp.status).toBe(400);
  });

  it("405 on a non-POST method", async () => {
    const captured: { req?: Request } = {};
    const env = makeAuthorizedEnv({ captured });
    const resp = await mintFetch(env, { auth: INTERNAL_KEY, method: "GET" });
    expect(resp.status).toBe(405);
    expect(captured.req).toBeUndefined();
  });

  it("NEVER lets a client trust header reach the mint route", async () => {
    const captured: { req?: Request } = {};
    const env = makeAuthorizedEnv({ captured });
    await mintFetch(env, { auth: INTERNAL_KEY, body: mintBody({ job_id: `${JOB_ID}-trusthdr` }) });
    // mintScopedPat builds a FRESH request; the only internal-auth header on the
    // forwarded request is the server-trusted key, and route-kind is "internal".
    expect(captured.req!.headers.get("x-corelink-route-kind")).toBe("internal");
    expect(captured.req!.headers.get("authorization")).toBeNull();
  });

  // ── WP5a: NARROWED runner-job PAT marker persisted on the pat row ───────────
  // The runner_job_ac_key column is written on EVERY runner mint (deny-DELETE at
  // minimum). Without an ac_output_name it is the sentinel "*"; WITH one it is the
  // BLAKE3 hex of ("clw/ref/runner/v1/" + name).
  const AC_KEY_BIND_INDEX = 6; // activation INSERT: runner_job_ac_key is ?7

  it("(wp5a) a runner mint with NO ac_output_name binds runner_job_ac_key = \"*\"", async () => {
    const captured: { req?: Request } = {};
    const patInsertCapture: { sql?: string; binds?: unknown[] } = {};
    const env = makeAuthorizedEnv({ captured, patInsertCapture });
    const resp = await mintFetch(env, {
      auth: INTERNAL_KEY,
      body: mintBody({ job_id: `${JOB_ID}-wp5a-star` }),
    });
    expect(resp.status).toBe(200);
    // Response shape UNCHANGED (still has tenant + max_concurrency from WP2).
    const body = (await resp.json()) as Record<string, unknown>;
    expect(body["tenant"]).toBe(TENANT);
    expect(body["max_concurrency"]).toBe(MAX_CONCURRENCY);
    // The pat INSERT wrote the runner_job_ac_key column with the "*" sentinel.
    expect(patInsertCapture.sql).toBeDefined();
    expect(patInsertCapture.sql!).toContain("runner_job_ac_key");
    expect(patInsertCapture.binds![AC_KEY_BIND_INDEX]).toBe("*");
  });

  it("(wp5a) a runner mint WITH ac_output_name binds the BLAKE3 hex (dormant path)", async () => {
    const captured: { req?: Request } = {};
    const patInsertCapture: { sql?: string; binds?: unknown[] } = {};
    const env = makeAuthorizedEnv({ captured, patInsertCapture });
    const resp = await mintFetch(env, {
      auth: INTERNAL_KEY,
      body: mintBody({ job_id: `${JOB_ID}-wp5a-key`, ac_output_name: "build-out" }),
    });
    expect(resp.status).toBe(200);
    // blake3("clw/ref/runner/v1/build-out") — pinned; matches worker/tests/blake3.test.ts
    // and the Rust `blake3` crate the container derives with (WP5b).
    expect(patInsertCapture.binds![AC_KEY_BIND_INDEX]).toBe(EXPECTED_AC_KEY_HEX);
    // Not the sentinel — an exact-key restriction was persisted.
    expect(patInsertCapture.binds![AC_KEY_BIND_INDEX]).not.toBe("*");
    // 64-char lowercase hex (BLAKE3-256).
    expect(patInsertCapture.binds![AC_KEY_BIND_INDEX]).toMatch(/^[0-9a-f]{64}$/);
  });

  it("(wp5a) an empty ac_output_name is REFUSED 400 (never minted)", async () => {
    const captured: { req?: Request } = {};
    const env = makeAuthorizedEnv({ captured });
    const resp = await mintFetch(env, {
      auth: INTERNAL_KEY,
      body: mintBody({ job_id: `${JOB_ID}-wp5a-empty`, ac_output_name: "" }),
    });
    expect(resp.status).toBe(400);
    expect(captured.req).toBeUndefined();
  });
});
// B-126 M3 split population: runner_mint_part2.test.ts, runner_mint_part3.test.ts
