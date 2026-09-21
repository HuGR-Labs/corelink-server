import { describe, expect, it } from "vitest";
import type { D1Database } from "@cloudflare/workers-types";
import type { Env } from "../src/index.js";
import { handleRunnerAuthorize } from "../src/lib/runner_authorization.js";
import { matchRoute } from "../src/route_match.js";

const KEY = "runner-authorize-test-key-0123456789abcdef";
const TENANT = "11111111-1111-1111-1111-111111111111";

function env(opts: {
  suspended?: boolean;
  maxConcurrency?: unknown;
  maxVcpuH?: number | null;
  mintCalls?: number[];
  tenant?: string;
  dbReads?: { value: number };
  allowlist?: { tenant: string; repo: string };
  dbSql?: string[];
}): Env {
  const db = {
    prepare: (sql: string) => {
      if (opts.dbReads) opts.dbReads.value++;
      opts.dbSql?.push(sql);
      return {
        bind: (...args: unknown[]) => ({
          first: async <T>() => {
            if (sql.includes("tenant_gh_installation_map")) {
              return { tenant_id: opts.tenant ?? TENANT } as T;
            }
            if (sql.includes("tenant_offboarding_state")) {
              return (opts.suspended ? { 1: 1 } : null) as T;
            }
            if (sql.includes("runner_repo_allowlist")) {
              // Mirror the query contract: tenant IDs compare exactly, while
              // GitHub repository names compare case-insensitively.
              const [tenant, repo] = args;
              const allowlist = opts.allowlist ?? {
                tenant: opts.tenant ?? TENANT,
                repo: "acme/widgets",
              };
              return tenant === allowlist.tenant &&
                typeof repo === "string" &&
                repo.toLowerCase() === allowlist.repo.toLowerCase()
                ? { 1: 1 } as T
                : null as T;
            }
            if (sql.includes("runners_entitlement")) {
              return (opts.maxConcurrency === undefined
                ? { max_concurrency: 4, max_vcpu_h: opts.maxVcpuH ?? null }
                : { max_concurrency: opts.maxConcurrency, max_vcpu_h: opts.maxVcpuH ?? null }) as T;
            }
            return null as T;
          },
          run: async () => ({ success: true, meta: { changes: 0 } }),
        }),
      };
    },
  } as unknown as D1Database;
  return {
    CORELINK_INTERNAL_AUTH_KEY: KEY,
    CORELINK_RUNNER_MINT_AUTH_KEY: KEY,
    CONFIG_DB: db,
    CORELINK_SERVER: {
      idFromName: () => ({}),
      get: () => ({
        fetch: async () => {
          opts.mintCalls?.push(1);
          return new Response("unexpected", { status: 500 });
        },
      }),
    },
    ENVIRONMENT: "test",
  } as unknown as Env;
}

function request(body: unknown, auth = KEY, method = "POST"): Request {
  const init: RequestInit = {
    method,
    headers: { "content-type": "application/json", "x-corelink-internal-auth": auth },
  };
  if (method !== "GET" && method !== "HEAD") init.body = JSON.stringify(body);
  return new Request("https://worker.test/internal/v1/runner/authorize", {
    ...init,
  });
}

describe("POST /internal/v1/runner/authorize", () => {
  it("routes only the exact authorize path to the read-only handler", () => {
    expect(matchRoute(new URL("https://worker.test/internal/v1/runner/authorize"))).toEqual({
      tenantId: "_system",
      pathSuffix: "/internal/v1/runner/authorize",
      routeKind: "runner_authorize",
    });
  });

  it("requires POST and the same job identity as mint", async () => {
    const get = await handleRunnerAuthorize(request({ repo_full_name: "acme/widgets", installation_id: "gh-1" }, KEY, "GET"), env({}), "req-method");
    expect(get.status).toBe(405);
    const missing = await handleRunnerAuthorize(request({ repo_full_name: "acme/widgets", installation_id: "gh-1" }), env({}), "req-job");
    expect(missing.status).toBe(400);
  });

  it.each([
    { job_id: " job-1", repo_full_name: "acme/widgets", installation_id: "gh-1" },
    { job_id: "job-1", repo_full_name: " acme/widgets", installation_id: "gh-1" },
    { job_id: "job-1", repo_full_name: "acme/widgets", installation_id: " gh-1" },
  ])("rejects whitespace identity before any DB reads: %j", async (body) => {
    const dbReads = { value: 0 };
    const response = await handleRunnerAuthorize(request(body), env({ dbReads }), "req-whitespace");
    expect(response.status).toBe(400);
    expect(dbReads.value).toBe(0);
  });

  it.each([null, "not-json"]) ("rejects malformed body %j before any effects", async (body) => {
    const requestBody = body === "not-json" ? body : JSON.stringify(body);
    const malformed = new Request("https://worker.test/internal/v1/runner/authorize", {
      method: "POST", headers: { "content-type": "application/json", "x-corelink-internal-auth": KEY }, body: requestBody,
    });
    const response = await handleRunnerAuthorize(malformed, env({}), "req-body");
    expect(response.status).toBe(400);
  });

  it("returns capacity without requiring or calling PAT mint", async () => {
    const mintCalls: number[] = [];
    const dbSql: string[] = [];
    const response = await handleRunnerAuthorize(
      request({ job_id: "job-1", repo_full_name: "acme/widgets", installation_id: "gh-1" }),
      env({ mintCalls, dbSql }),
      "req-1",
    );
    expect(response.status).toBe(200);
    await expect(response.json()).resolves.toEqual({ tenant: TENANT, max_concurrency: 4 });
    expect(mintCalls).toHaveLength(0);
    expect(dbSql.some((sql) => sql.includes("session_exchange_throttle"))).toBe(false);
    expect(dbSql.some((sql) => sql.includes("runner_credential_obligation"))).toBe(false);
  });

  it("matches a mixed-case allowlist entry to a lowercase repository request", async () => {
    const dbSql: string[] = [];
    const response = await handleRunnerAuthorize(
      request({ job_id: "job-1", repo_full_name: "acme/widgets", installation_id: "gh-1" }),
      env({ allowlist: { tenant: TENANT, repo: "Acme/Widgets" }, dbSql }),
      "req-repo-case",
    );
    expect(response.status).toBe(200);
    expect(dbSql.find((sql) => sql.includes("runner_repo_allowlist"))).toContain(
      "repo_full_name = ?2 COLLATE NOCASE",
    );
  });

  it("does not let another tenant's allowlist authorize the repository", async () => {
    const otherTenant = "22222222-2222-2222-2222-222222222222";
    const response = await handleRunnerAuthorize(
      request({ job_id: "job-1", repo_full_name: "acme/widgets", installation_id: "gh-1" }),
      env({
        tenant: otherTenant,
        allowlist: { tenant: TENANT, repo: "Acme/Widgets" },
      }),
      "req-tenant-case",
    );
    expect(response.status).toBe(403);
  });

  it("forbidden authorization has zero mint effects", async () => {
    let fetches = 0;
    const e = env({ suspended: true, mintCalls: [] });
    e.CORELINK_SERVER = { idFromName: () => ({}), get: () => ({ fetch: async () => { fetches++; return new Response("mint"); } }) } as never;
    const response = await handleRunnerAuthorize(request({ job_id: "job-1", repo_full_name: "acme/widgets", installation_id: "gh-1" }), e, "req-2");
    expect(response.status).toBe(403);
    expect(fetches).toBe(0);
  });

  it("rejects a whitespace-corrupted derived tenant before authz reads", async () => {
    const dbReads = { value: 0 };
    const response = await handleRunnerAuthorize(
      request({ job_id: "job-1", repo_full_name: "acme/widgets", installation_id: "gh-1" }),
      env({ tenant: ` ${TENANT}`, dbReads }),
      "req-tenant",
    );
    expect(response.status).toBe(403);
    expect(dbReads.value).toBe(1);
  });

  it.each([0, -5])("preserves existing behavior by omitting nonpositive max_vcpu_h (%p)", async (maxVcpuH) => {
    const response = await handleRunnerAuthorize(request({ job_id: "job-1", repo_full_name: "acme/widgets", installation_id: "gh-1" }), env({ maxVcpuH }), "req-vcpu");
    expect(response.status).toBe(200);
    expect((await response.json() as Record<string, unknown>)["max_vcpu_h"]).toBeUndefined();
  });

  it("requires the dedicated runner-mint binding and rejects an incorrect presented key", async () => {
    const missingBinding = env({}) as Env & { CORELINK_RUNNER_MINT_AUTH_KEY?: string };
    missingBinding.CORELINK_RUNNER_MINT_AUTH_KEY = undefined;
    const body = { job_id: "job-1", repo_full_name: "acme/widgets", installation_id: "gh-1" };
    const unavailable = await handleRunnerAuthorize(request(body), missingBinding, "req-no-binding");
    expect(unavailable.status).toBe(503);
    const missingHeader = await handleRunnerAuthorize(request(body, ""), env({}), "req-missing-key");
    expect(missingHeader.status).toBe(401);
    const wrongKey = await handleRunnerAuthorize(request(body, "wrong-runner-key"), env({}), "req-wrong-key");
    expect(wrongKey.status).toBe(401);
  });

  it("omits a null vCPU ceiling", async () => {
    const response = await handleRunnerAuthorize(request({ job_id: "job-1", repo_full_name: "acme/widgets", installation_id: "gh-1" }), env({ maxVcpuH: null }), "req-null");
    expect(response.status).toBe(200);
    expect((await response.json() as Record<string, unknown>)["max_vcpu_h"]).toBeUndefined();
  });
});
