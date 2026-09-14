/**
 * Real-workerd regression for F008's operation-backed runner path.
 *
 * The wrapper is only a spy: `_system` itself is the production CoreLinkServer
 * Durable Object from the freshly-built worker bundle. This matters because the
 * floor predicate lives behind the public handler in the DO preparation route;
 * a Node-only namespace stub can accidentally turn its D1 SELECT into a no-op.
 */

import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import { execSync } from "node:child_process";
import { existsSync } from "node:fs";
import { resolve } from "node:path";
import type { DurableObjectNamespace } from "@cloudflare/workers-types";
import type { Env } from "../src/index.js";
import { handleRunnerMint } from "../src/lib/runner_mint.js";

const WORKER_DIR = resolve(__dirname, "..");
const ROOT = resolve(WORKER_DIR, "..");
const DIST_INDEX = resolve(ROOT, "dist/index.js");
const KEY = "runner-mint-miniflare-key-0123456789";
const TENANT = "11111111-1111-4111-8111-111111111111";
const OPERATION = "11111111-2222-4333-8444-555555555555";

// eslint-disable-next-line @typescript-eslint/no-explicit-any
let mf: any;

beforeAll(async () => {
  execSync(
    `${WORKER_DIR}/node_modules/.bin/wrangler deploy --dry-run --outdir dist --containers-rollout=none`,
    { cwd: ROOT, stdio: "inherit" },
  );
  if (!existsSync(DIST_INDEX)) throw new Error("worker bundle missing");
  const { Miniflare } = await import("miniflare");
  mf = new Miniflare({
    scriptPath: DIST_INDEX,
    modulesRoot: resolve(ROOT, "dist"),
    modules: true,
    durableObjects: { CORELINK_SERVER: "CoreLinkServer" },
    compatibilityDate: "2026-04-01",
    compatibilityFlags: ["nodejs_compat"],
    bindings: {
      ENVIRONMENT: "miniflare-test",
      CORELINK_RUNNER_MINT_AUTH_KEY: KEY,
      CORELINK_PAT_MINT_AUTH_KEY: KEY,
      CORELINK_INTERNAL_AUTH_KEY: KEY,
      PAGERDUTY_ROUTING_KEY: "",
    },
    d1Databases: { CONFIG_DB: "runner-floor" },
  });
  const db = await mf.getD1Database("CONFIG_DB");
  await db.exec([
    "CREATE TABLE tenant_gh_installation_map (installation_id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL)",
    "CREATE TABLE tenant_offboarding_state (tenant_id TEXT PRIMARY KEY)",
    "CREATE TABLE runner_repo_allowlist (tenant_id TEXT NOT NULL, repo_full_name TEXT NOT NULL)",
    "CREATE TABLE runners_entitlement (tenant_id TEXT PRIMARY KEY, max_concurrency INTEGER NOT NULL, max_vcpu_h INTEGER)",
    "CREATE TABLE session_exchange_throttle (clerk_sub TEXT PRIMARY KEY, window_start_ms INTEGER NOT NULL, count INTEGER NOT NULL)",
    "CREATE TABLE tenant_credential_revocation_floor (tenant_id TEXT PRIMARY KEY, revoked_through TEXT NOT NULL)",
    "CREATE TABLE runner_credential_obligation (operation_id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, job_id TEXT NOT NULL, repo TEXT NOT NULL, state TEXT NOT NULL, deadline_ms INTEGER NOT NULL, lifecycle_generation TEXT NOT NULL, pat_id TEXT, token_id TEXT)",
    "CREATE TABLE pat (pat_id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, pat_hash TEXT NOT NULL, scope TEXT NOT NULL, expires_ms INTEGER NOT NULL, token_id TEXT NOT NULL, shown_once_token TEXT NOT NULL, shown_once_consumed INTEGER NOT NULL, created_ms INTEGER NOT NULL, runner_job_ac_key TEXT, lifecycle_generation TEXT)",
    `INSERT INTO tenant_gh_installation_map VALUES ('install-floor', '${TENANT}')`,
    `INSERT INTO runner_repo_allowlist VALUES ('${TENANT}', 'acme/widgets')`,
    `INSERT INTO runners_entitlement (tenant_id, max_concurrency) VALUES ('${TENANT}', 1)`,
    `INSERT INTO tenant_credential_revocation_floor VALUES ('${TENANT}', '9223372036854775807')`,
  ].join(";"));
}, 90_000);

afterAll(async () => { if (mf) await mf.dispose(); });

describe("F008 real _system DO floor", () => {
  it("rejects an operation at the signed-i64 floor ceiling before container mint and leaves no PAT", async () => {
    const db = await mf.getD1Database("CONFIG_DB");
    const real = await mf.getDurableObjectNamespace("CORELINK_SERVER");
    const upstreamPaths: string[] = [];
    const namespace = {
      idFromName: (name: string) => real.idFromName(name),
      get: (id: unknown) => {
        const stub = real.get(id);
        return {
          fetch: async (request: Request) => {
            upstreamPaths.push(new URL(request.url).pathname);
            return stub.fetch(request);
          },
        };
      },
    } as unknown as DurableObjectNamespace;
    const lifecycleFetch = vi.fn(async () => Response.json({ tenant_id: TENANT, generation: "9223372036854775807", suspended: false }));
    vi.stubGlobal("fetch", lifecycleFetch);
    try {
      const response = await handleRunnerMint(new Request("https://worker/internal/v1/runner/mint", {
        method: "POST",
        headers: { "content-type": "application/json", "x-corelink-internal-auth": KEY },
        body: JSON.stringify({ job_id: "job-floor", repo_full_name: "acme/widgets", installation_id: "install-floor", operation_id: OPERATION }),
      }), {
        ENVIRONMENT: "miniflare-test",
        CONFIG_DB: db,
        CORELINK_SERVER: namespace,
        CORELINK_RUNNER_MINT_AUTH_KEY: KEY,
        CORELINK_PAT_MINT_AUTH_KEY: KEY,
        CORELINK_INTERNAL_AUTH_KEY: KEY,
        FABRIC_CREDENTIAL_AUTHORITY_URL: "https://lifecycle.test",
        FABRIC_CREDENTIAL_ISSUER_AUTH_KEY: "credential-issuer-test-key-0123456789",
      } as Env, "req-floor");
      expect(response.status).toBe(503);
      expect(lifecycleFetch).toHaveBeenCalledTimes(1);
      expect(upstreamPaths).toEqual(["/_do/runner-cleanup/prepare"]);
      expect(await db.prepare("SELECT COUNT(*) AS n FROM pat").first<{ n: number }>()).toEqual({ n: 0 });
    } finally {
      vi.unstubAllGlobals();
    }
  });
});
