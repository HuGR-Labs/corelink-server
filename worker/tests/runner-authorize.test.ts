import { describe, expect, it } from "vitest";
import type { D1Database } from "@cloudflare/workers-types";
import type { Env } from "../src/index.js";
import { handleRunnerAuthorize } from "../src/lib/runner_authorization.js";

const KEY = "runner-authorize-test-key-0123456789abcdef";
const TENANT = "11111111-1111-1111-1111-111111111111";

function env(opts: { suspended?: boolean; maxConcurrency?: unknown; maxVcpuH?: unknown; mintCalls?: number[] }): Env {
  const db = {
    prepare: (sql: string) => ({
      bind: (...args: unknown[]) => ({
        first: async <T>() => {
          if (sql.includes("tenant_gh_installation_map")) return { tenant_id: TENANT } as T;
          if (sql.includes("tenant_offboarding_state")) return (opts.suspended ? { 1: 1 } : null) as T;
          if (sql.includes("runner_repo_allowlist")) return { 1: 1 } as T;
          if (sql.includes("runners_entitlement")) {
            return (opts.maxConcurrency === undefined ? { max_concurrency: 4, max_vcpu_h: opts.maxVcpuH ?? null } :
              { max_concurrency: opts.maxConcurrency, max_vcpu_h: opts.maxVcpuH ?? null }) as T;
          }
          return null as T;
        },
        run: async () => ({ success: true, meta: { changes: 0 } }),
      }),
    }),
  } as unknown as D1Database;
  return {
    CORELINK_INTERNAL_AUTH_KEY: KEY,
    CONFIG_DB: db,
    CORELINK_SERVER: {
      idFromName: () => ({}),
      get: () => ({ fetch: async () => new Response("unexpected", { status: 500 }) }),
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
  it("requires POST and the same job identity as mint", async () => {
    const get = await handleRunnerAuthorize(request({ repo_full_name: "acme/widgets", installation_id: "gh-1" }, KEY, "GET"), env({}), "req-method");
    expect(get.status).toBe(405);
    const missing = await handleRunnerAuthorize(request({ repo_full_name: "acme/widgets", installation_id: "gh-1" }), env({}), "req-job");
    expect(missing.status).toBe(400);
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
    const response = await handleRunnerAuthorize(request({ job_id: "job-1", repo_full_name: "acme/widgets", installation_id: "gh-1" }), env({ maxVcpuH: 0 }), "req-1");
    expect(response.status).toBe(200);
    await expect(response.json()).resolves.toEqual({ tenant: TENANT, max_concurrency: 4, max_vcpu_h: 0 });
  });

  it("forbidden authorization has zero mint effects", async () => {
    let fetches = 0;
    const e = env({ suspended: true, mintCalls: [] });
    e.CORELINK_SERVER = { idFromName: () => ({}), get: () => ({ fetch: async () => { fetches++; return new Response("mint"); } }) } as never;
    const response = await handleRunnerAuthorize(request({ job_id: "job-1", repo_full_name: "acme/widgets", installation_id: "gh-1" }), e, "req-2");
    expect(response.status).toBe(403);
    expect(fetches).toBe(0);
  });

  it("rejects invalid concurrency while allowing a finite zero budget", async () => {
    const zero = await handleRunnerAuthorize(request({ job_id: "job-1", repo_full_name: "acme/widgets", installation_id: "gh-1" }), env({ maxVcpuH: 0 }), "req-3");
    expect(zero.status).toBe(200);
    const bad = await handleRunnerAuthorize(request({ job_id: "job-1", repo_full_name: "acme/widgets", installation_id: "gh-1" }), env({ maxConcurrency: 0 }), "req-4");
    expect(bad.status).toBe(403);
  });

  it.each([-1, Number.NaN, Number.POSITIVE_INFINITY, "zero"]) ("rejects invalid present vCPU ceiling %p", async (maxVcpuH) => {
    const response = await handleRunnerAuthorize(request({ job_id: "job-1", repo_full_name: "acme/widgets", installation_id: "gh-1" }), env({ maxVcpuH }), "req-vcpu");
    expect(response.status).toBe(403);
  });

  it("omits only a null vCPU ceiling", async () => {
    const response = await handleRunnerAuthorize(request({ job_id: "job-1", repo_full_name: "acme/widgets", installation_id: "gh-1" }), env({ maxVcpuH: null }), "req-null");
    expect(response.status).toBe(200);
    expect((await response.json() as Record<string, unknown>)["max_vcpu_h"]).toBeUndefined();
  });
});
