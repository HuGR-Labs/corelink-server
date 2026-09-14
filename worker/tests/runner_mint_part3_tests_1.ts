import { describe, it, expect } from "vitest";
import type { D1Database, DurableObjectNamespace } from "@cloudflare/workers-types";
import type { Env } from "../src/index.js";
import { workerHandler, batchViaFirst, INTERNAL_KEY, RUNNER_MINT_KEY, TENANT, JOB_ID, INSTALLATION_ID, REPO_FULL_NAME, MAX_CONCURRENCY, EXPECTED_AC_KEY_HEX, CANNED_MINT, makeCtx, makeConfigDb, authorizedConfig, makeMintNamespace, makeEnv, makeAuthorizedEnv, mintFetch, revokeFetch, mintBody } from "./runner_mint_test_helpers.js";

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
        if (pathname === "/_do/runner-cleanup/prepare") {
          return new Response(null, { status: 204 });
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
