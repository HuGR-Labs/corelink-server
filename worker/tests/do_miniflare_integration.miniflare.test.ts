/**
 * Miniflare integration tests for CoreLinkServer Durable Object.
 *
 * R2 mandate (§0.8): "miniflare integration tests for DO are mandatory".
 *
 * These tests use miniflare v4 (declared in worker/package.json devDependencies
 * — NOT a phantom hoisted dependency) programmatically within a
 * standard Node.js vitest test. A Miniflare instance is created, the worker
 * bundle (compiled by wrangler --dry-run) is loaded into workerd, and HTTP
 * requests are dispatched through the full Workers runtime (NOT Node.js).
 *
 * The beforeAll hook ALWAYS rebuilds dist/index.js from current source (via
 * `wrangler deploy --dry-run --outdir dist`, run from the worktree root) and
 * loads it into workerd by `scriptPath` (+ nodejs_compat). Always-rebuild is
 * load-bearing: a stale bundle silently tests old behaviour (that once diverged
 * 6 tests here); scriptPath (not an in-memory string) is required so miniflare
 * can resolve the `node:async_hooks` import @sentry/cloudflare injects.
 *
 * Note on @cloudflare/vitest-pool-workers: NOT used. (An older note here claimed
 * a pool-workers/vitest/miniflare version incompatibility — that was never the
 * real blocker; the past breakage was the stale-bundle + string-script loader,
 * now fixed.) The programmatic Miniflare v4 approach gives the same workerd-backed
 * guarantee directly.
 *
 * What is tested in workerd:
 *   - Worker HTTP handler (health, auth, routing, CORS, error envelopes)
 *   - DO state machine via health probe and stop endpoints
 *   - Per-tenant DO isolation (idFromName → separate DO instances)
 *   - Request ID correlation across worker → DO
 *
 * What is NOT testable (CF Containers beta, not in miniflare harness):
 *   - state.container API (getTcpPort, start, running, destroy)
 */

import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { execSync } from "node:child_process";
import { existsSync } from "node:fs";
import { resolve } from "node:path";
import { TEST_PAT_SIGNING_KEY, mintTestPat } from "./setup.js";

const WORKER_DIR = resolve(__dirname, "..");
// wrangler writes dist/ relative to wrangler.toml's directory (worktree root)
const WORKTREE_ROOT = resolve(WORKER_DIR, "..");
const DIST_INDEX = resolve(WORKTREE_ROOT, "dist/index.js");
const WRANGLER_TOML = resolve(WORKTREE_ROOT, "wrangler.toml");

// ──────────────────────────────────────────────────────────────────────────────
// Miniflare instance (shared across all tests in this file)
// ──────────────────────────────────────────────────────────────────────────────

// eslint-disable-next-line @typescript-eslint/no-explicit-any
let mf: any = null;

async function dispatchFetch(path: string, init?: RequestInit): Promise<Response> {
  const url = `https://corelink.test${path}`;
  return mf.dispatchFetch(url, {
    method: init?.method ?? "GET",
    headers: Object.fromEntries(new Headers(init?.headers ?? {}).entries()),
    body: init?.body ?? undefined,
  });
}

beforeAll(async () => {
  // ALWAYS rebuild the worker bundle from current source. A stale dist/index.js
  // silently tests OLD behaviour — the pre-Sentry (May-2026) bundle is exactly
  // what made 6 tests diverge here. Run from the worktree root so `--outdir dist`
  // resolves against the root wrangler.toml (there is no worker/wrangler.toml).
  console.log("[miniflare-test] Building worker bundle via wrangler dry-run...");
  execSync(
    // `--containers-rollout=none`: skip the Docker container image build. Without
    // it, on a runner WITH docker (CI ubuntu) `--dry-run` builds the full container
    // image before bundling the JS (slow / fails); the worker JS bundle is all this
    // workerd suite needs. (On macos-without-docker it was a silent no-op.)
    `${WORKER_DIR}/node_modules/.bin/wrangler deploy --dry-run --outdir dist --containers-rollout=none`,
    { cwd: WORKTREE_ROOT, stdio: "inherit" },
  );

  if (!existsSync(DIST_INDEX)) {
    throw new Error(`[miniflare-test] Worker bundle not found at ${DIST_INDEX}. Run: cd worker && pnpm build`);
  }

  // Lazy-import miniflare to avoid loading it in the primary test suite
  // (which uses the miniflare pinned by @cloudflare/vitest-pool-workers).
  // Resolved from worker/package.json devDependencies (miniflare v4).
  const { Miniflare } = await import("miniflare");

  mf = new Miniflare({
    // Load the bundle from its FILE on disk (not an in-memory string): the
    // current bundle imports `node:async_hooks` (via @sentry/cloudflare), which
    // miniflare can only resolve from a real scriptPath + nodejs_compat — a bare
    // string `script` errors ERR_MODULE_STRING_SCRIPT before any test runs.
    scriptPath: DIST_INDEX,
    modulesRoot: resolve(WORKTREE_ROOT, "dist"),
    modules: true,
    durableObjects: {
      CORELINK_SERVER: "CoreLinkServer",
      ROLLOUT_DO: "RolloutController",
    },
    compatibilityDate: "2026-04-01",
    // Matches the root wrangler.toml — required for the `node:async_hooks` import.
    compatibilityFlags: ["nodejs_compat"],
    bindings: {
      ENVIRONMENT: "miniflare-test",
      PAGERDUTY_ROUTING_KEY: "",
      // Honest auth harness (#345): the native plane (extractAuth) fails CLOSED
      // with 503 SERVICE_UNAVAILABLE unless PAT_SIGNING_KEY is bound AND the PAT
      // HMAC verifies. Bind the fixed valid test key so the auth/route logic is
      // actually exercised in workerd (a real 401 on bad PATs, the DO's
      // CONTAINER_UNAVAILABLE on valid ones) — not masked by a misconfig 503.
      PAT_SIGNING_KEY: TEST_PAT_SIGNING_KEY,
    },
    // D1 database binding — in-memory SQLite seeded with the test PAT row.
    d1Databases: {
      CONFIG_DB: "test-config-db",
    },
  });

  // Seed the D1 PAT table with a test row so authenticated tests can pass.
  //
  // The schema MIRRORS the real D1 `pat` table (migrations/d1/0037 + 0054 + 0086):
  // the Worker auth path (worker/src/index.ts extractAuth) runs
  //   SELECT tenant_id, expires_ms, scope, runner_job_ac_key FROM pat WHERE token_id = ?1
  // so every column it selects MUST exist here — a missing column makes the
  // D1 query throw, which extractAuth maps to a fail-closed 401
  // (reason: d1_lookup_error) and the authenticated tests silently degrade.
  // (The FOREIGN KEY to tenant(tenant_id) is intentionally omitted — the mock
  // seeds no tenant table and the auth query never joins it.)
  const d1 = await mf.getD1Database("CONFIG_DB");
  await d1.exec(
    "CREATE TABLE IF NOT EXISTS pat (" +
      "pat_id TEXT NOT NULL PRIMARY KEY, " +
      "tenant_id TEXT NOT NULL, " +
      "pat_hash TEXT NOT NULL UNIQUE, " +
      "scope TEXT NOT NULL DEFAULT 'read-write' CHECK (scope IN ('read-write', 'read-only', 'admin')), " +
      "expires_ms BIGINT NOT NULL, " +
      "shown_once_token TEXT NOT NULL UNIQUE, " +
      "shown_once_consumed INTEGER NOT NULL DEFAULT 0 CHECK (shown_once_consumed IN (0, 1)), " +
      "created_ms BIGINT NOT NULL, " +
      "token_id TEXT, " +
      // 0063: soft-revocation marker — NULL = active (worker filters AND revoked_at_ms IS NULL)
      "revoked_at_ms BIGINT, " +
      // 0086: cf-multitenant narrowed runner-job marker — NULL on normal PATs
      "runner_job_ac_key TEXT)",
  );
  await d1.exec("CREATE UNIQUE INDEX IF NOT EXISTS idx_pat_token_id ON pat (token_id)");
  const seedPat =
    "INSERT OR IGNORE INTO pat " +
    "(pat_id, tenant_id, pat_hash, scope, expires_ms, shown_once_token, shown_once_consumed, created_ms, token_id) " +
    "VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7, ?8)";
  await d1.prepare(seedPat).bind(
    "00000000-0000-0000-0000-000000000042",
    TEST_TENANT_ID,
    "mock-pat-hash-tenant-1", // never verified in the Worker (Argon2id is the DO's layer)
    "read-write",
    Date.now() + 7_200_000, // +2h
    "00000000-0000-0000-0000-0000000000a1",
    Date.now(),
    TEST_TOKEN_ID,
  ).run();
  // Second tenant for cross-tenant isolation tests (distinct token_id + tenant_id).
  await d1.prepare(seedPat).bind(
    "00000000-0000-0000-0000-000000000043",
    SECOND_TENANT_ID,
    "mock-pat-hash-tenant-2",
    "read-write",
    Date.now() + 7_200_000, // +2h
    "00000000-0000-0000-0000-0000000000a2",
    Date.now(),
    SECOND_TOKEN_ID,
  ).run();

  // Seed the `tenant` table with a primary_region (WI-MULTI-REGION-V1). After a
  // PAT authenticates, the Worker reads tenant.primary_region to decide regional
  // fan-out (backlog #29 residency). A MISSING `tenant` table makes that SELECT
  // throw → the Worker fails CLOSED with 503 RESIDENCY_UNAVAILABLE before
  // reaching the DO. Seeding both tenants with a `wnam` macro (maps to colo
  // `iad`, the local/non-fanout path via region-map) lets the request fall
  // through to the DO so the assertion sees the DO's CONTAINER_UNAVAILABLE
  // (not a residency fail-close). `wnam` = US-West, no cross-border fanout.
  await d1.exec(
    "CREATE TABLE IF NOT EXISTS tenant (" +
      "tenant_id TEXT NOT NULL PRIMARY KEY, " +
      "primary_region TEXT)",
  );
  const seedTenant =
    "INSERT OR IGNORE INTO tenant (tenant_id, primary_region) VALUES (?1, ?2)";
  await d1.prepare(seedTenant).bind(TEST_TENANT_ID, "wnam").run();
  await d1.prepare(seedTenant).bind(SECOND_TENANT_ID, "wnam").run();

  // Warm up: dispatch a health check to confirm the worker is ready
  const health = await mf.dispatchFetch("https://corelink.test/health");
  if (health.status !== 200) {
    throw new Error(`[miniflare-test] Worker health check failed (status=${health.status})`);
  }
}, 120_000);

afterAll(async () => {
  if (mf !== null) {
    await mf.dispose();
    mf = null;
  }
});

// Canonical test PAT — CRYPTOGRAPHICALLY VALID under TEST_PAT_SIGNING_KEY
// (honest auth harness, #345) and recognised by the D1 row seeded in beforeAll.
// Its token_id MUST match the token_id inserted into D1. With PAT_SIGNING_KEY
// bound in the harness, an unsigned all-"A" sig would now fail the HMAC
// fast-fail, so the token must be properly minted.
const TEST_TOKEN_ID = "AAAAAAAAAAAAAAAA"; // 16 Crockford b32
const TEST_TENANT_ID = "00000000-0000-0000-0000-000000000001";
const VALID_TOKEN = await mintTestPat({ tokenId: TEST_TOKEN_ID });

// Second tenant — distinct token_id + tenant_id — for cross-tenant isolation.
const SECOND_TOKEN_ID = "BBBBBBBBBBBBBBBB"; // 16 Crockford b32
const SECOND_TENANT_ID = "00000000-0000-0000-0000-000000000002";
const SECOND_TOKEN = await mintTestPat({ tokenId: SECOND_TOKEN_ID });

// ──────────────────────────────────────────────────────────────────────────────
// Health endpoint in workerd
// ──────────────────────────────────────────────────────────────────────────────

describe("miniflare: GET /health in workerd", () => {
  it("returns 200 with status:ok", async () => {
    const resp = await dispatchFetch("/health");
    expect(resp.status).toBe(200);
    const body = await resp.json() as { status: string; env: string };
    expect(body.status).toBe("ok");
  });

  it("sets X-Request-Id", async () => {
    const resp = await dispatchFetch("/health");
    expect(resp.headers.get("x-request-id")).not.toBeNull();
  });

  it("propagates x-request-id from client", async () => {
    const rid = "mf3-test-rid-abc123";
    const resp = await dispatchFetch("/health", {
      headers: { "x-request-id": rid },
    });
    expect(resp.headers.get("x-request-id")).toBe(rid);
  });

  it("omits env field from response body (F19: no env disclosure on unauth)", async () => {
    // F19 (src/index.ts): the deployment environment must NOT be disclosed on
    // unauthenticated endpoints. /health serves {"status":"ok"} only — no `env`.
    const resp = await dispatchFetch("/health");
    const body = await resp.json() as { status: string; env?: string };
    expect(body.status).toBe("ok");
    expect(body.env).toBeUndefined();
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// Auth middleware in workerd
// ──────────────────────────────────────────────────────────────────────────────

describe("miniflare: auth middleware in workerd", () => {
  it("does NOT 401 /v2/ at the Worker — forwards to the _oci DO (no Worker auth gate)", async () => {
    // PR #169: /v2/* is a pass-through. The Worker no longer auth-gates it or
    // builds an OCI envelope. In the miniflare harness CF Containers are not
    // available, so the real CoreLinkServer DO returns 503 CONTAINER_UNAVAILABLE
    // — the Worker forwards THAT verbatim (not a synthesized 401 OCI envelope).
    const resp = await dispatchFetch("/v2/myrepo/blobs/sha256:abc");
    expect(resp.status).toBe(503);
    const body = await resp.json() as Record<string, unknown>;
    expect(body["error"]).toBe("CONTAINER_UNAVAILABLE");
    // The Worker did NOT synthesize an OCI errors[] array.
    expect(body["errors"]).toBeUndefined();
  });

  it("does NOT set Docker-Distribution-Api-Version on /v2/ from the Worker", async () => {
    // The container owns the OCI-spec header; with no container in the harness
    // the forwarded DO 503 carries no such header, and the Worker adds none.
    const resp = await dispatchFetch("/v2/");
    expect(resp.headers.get("docker-distribution-api-version")).toBeNull();
  });

  it("returns REAPI 401 for /api/v2/ without auth", async () => {
    const resp = await dispatchFetch("/api/v2/tenant/blobs");
    expect(resp.status).toBe(401);
    const body = await resp.json() as { error: string };
    expect(body.error).toBe("UNAUTHORIZED");
    expect((body as unknown as { errors?: unknown }).errors).toBeUndefined();
  });

  it("returns 401 for token too short (<32 chars)", async () => {
    const resp = await dispatchFetch("/api/v2/t/p", {
      headers: { Authorization: "Bearer shorttoken" },
    });
    expect(resp.status).toBe(401);
  });

  it("returns 401 for token with space char (0x20 < 0x21 guard)", async () => {
    // Note: undici (used by miniflare v3) rejects requests with control chars (0x01-0x1F)
    // in HTTP headers per RFC 7230 strict validation. We test the space char (0x20)
    // which is also below the 0x21 threshold but is valid in HTTP headers.
    // The token format validates 0x21..0x7E — space (0x20) fails this check.
    const badToken = "a".repeat(32) + " " + "a".repeat(31);
    const resp = await dispatchFetch("/api/v2/t/p", {
      headers: { Authorization: `Bearer ${badToken}` },
    });
    expect(resp.status).toBe(401);
  });

  it("token value NOT reflected in 401 response body", async () => {
    const secretToken = "SECRET_WORKERD_TOKEN" + "x".repeat(44);
    const resp = await dispatchFetch("/api/v2/t/path", {
      headers: { Authorization: `Bearer ${secretToken}` },
    });
    const text = await resp.text();
    expect(text).not.toContain("SECRET_WORKERD_TOKEN");
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// Authenticated request → DO dispatch in workerd
// ──────────────────────────────────────────────────────────────────────────────

describe("miniflare: authenticated request → DO dispatch in workerd", () => {
  it("OCI request reaches the _oci DO via pass-through — returns 503 CONTAINER_UNAVAILABLE", async () => {
    // PR #169: the Worker forwards /v2/* to the _oci DO with no auth gate. With
    // no container bound the DO returns 503 CONTAINER_UNAVAILABLE. The repo NAME
    // in the path (here a UUID-shaped string) is NOT a tenant — it is forwarded
    // as-is; the request still lands on the shared "_oci" DO.
    const resp = await dispatchFetch(`/v2/myrepo/blobs/sha256:abc`);
    expect(resp.status).toBe(503);
    const body = await resp.json() as { error: string };
    expect(body.error).toBe("CONTAINER_UNAVAILABLE");
  });

  it("authenticated REAPI request reaches DO — returns 503", async () => {
    const resp = await dispatchFetch(`/api/v2/${TEST_TENANT_ID}/blobs`, {
      headers: { Authorization: `Bearer ${VALID_TOKEN}` },
    });
    expect(resp.status).toBe(503);
  });

  it("authenticated npm request reaches DO — returns 503", async () => {
    const resp = await dispatchFetch(`/npm/${TEST_TENANT_ID}/package`, {
      headers: { Authorization: `Bearer ${VALID_TOKEN}` },
    });
    expect(resp.status).toBe(503);
  });

  it("x-request-id is set on DO 503 response", async () => {
    const resp = await dispatchFetch(`/api/v2/${TEST_TENANT_ID}/path`, {
      headers: {
        Authorization: `Bearer ${VALID_TOKEN}`,
        "x-request-id": "mf3-do-req-id-test",
      },
    });
    expect(resp.headers.get("x-request-id")).not.toBeNull();
  });

  it("Stripe webhook reaches the _system DO via pass-through — NOT 401 (returns 503 with no container)", async () => {
    // LAUNCH-BLOCKER fix: /v1/billing/stripe-webhook carries only a
    // Stripe-Signature (no Bearer PAT). The Worker forwards it to the shared
    // _system DO with NO PAT gate. With no container bound the DO returns 503
    // CONTAINER_UNAVAILABLE — the key assertion is that it is NOT a Worker 401.
    const resp = await dispatchFetch("/v1/billing/stripe-webhook", {
      method: "POST",
      headers: { "Stripe-Signature": "t=1700000000,v1=deadbeefcafef00ddeadbeefcafef00d" },
      body: JSON.stringify({ id: "evt_1", type: "checkout.session.completed" }),
    });
    expect(resp.status).not.toBe(401);
    expect(resp.status).toBe(503);
    const body = await resp.json() as { error: string };
    expect(body.error).toBe("CONTAINER_UNAVAILABLE");
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// CORS in workerd
// ──────────────────────────────────────────────────────────────────────────────

describe("miniflare: CORS in workerd", () => {
  it("OPTIONS from allowed origin returns 204 with ACAO", async () => {
    const resp = await dispatchFetch("/v2/", {
      method: "OPTIONS",
      headers: {
        Origin: "https://humangr.com",
        "Access-Control-Request-Method": "GET",
      },
    });
    expect(resp.status).toBe(204);
    expect(resp.headers.get("access-control-allow-origin")).toBe(
      "https://humangr.com",
    );
  });

  it("OPTIONS from unknown origin returns 204 with no ACAO", async () => {
    const resp = await dispatchFetch("/health", {
      method: "OPTIONS",
      headers: {
        Origin: "https://evil.attacker.com",
        "Access-Control-Request-Method": "GET",
      },
    });
    expect(resp.status).toBe(204);
    expect(resp.headers.get("access-control-allow-origin")).toBeNull();
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// Security invariants in workerd
// ──────────────────────────────────────────────────────────────────────────────

describe("miniflare: security invariants in workerd", () => {
  it("X-Request-Id present on every response type", async () => {
    const paths = ["/health", "/v2/", "/api/v2/t/p", "/unknown-xyz"];
    for (const path of paths) {
      const resp = await dispatchFetch(path);
      expect(
        resp.headers.get("x-request-id"),
        `missing x-request-id for ${path}`,
      ).not.toBeNull();
    }
  });

  it("404 has correct JSON shape", async () => {
    const resp = await dispatchFetch("/completely-unknown-route");
    expect(resp.status).toBe(404);
    const body = await resp.json() as Record<string, unknown>;
    expect(body["error"]).toBe("NOT_FOUND");
    expect(typeof body["request_id"]).toBe("string");
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// DO tenant isolation in workerd
// ──────────────────────────────────────────────────────────────────────────────

describe("miniflare: DO tenant isolation in workerd", () => {
  it("two distinct tenants each reach their own DO namespace (cross-tenant isolation)", async () => {
    // Post-WP-T1: DO routing is keyed on the PAT-resolved tenant, and the
    // path-spoof gate requires the URL tenant segment to match the token's
    // tenant. Two DIFFERENT tenant tokens, each hitting their OWN namespace,
    // must both authenticate and dispatch to the DO (503 CONTAINER_UNAVAILABLE).
    // (Cross-tenant access — token of tenant A on tenant B's path — is rejected
    // with 403 and is covered by the spoof-gate unit tests.)
    const respA = await dispatchFetch(`/api/v2/${TEST_TENANT_ID}/blobs`, {
      headers: { Authorization: `Bearer ${VALID_TOKEN}` },
    });
    const respB = await dispatchFetch(`/api/v2/${SECOND_TENANT_ID}/blobs`, {
      headers: { Authorization: `Bearer ${SECOND_TOKEN}` },
    });
    expect(respA.status).toBe(503);
    expect(respB.status).toBe(503);

    const bodyA = await respA.json() as { error: string; request_id: string };
    const bodyB = await respB.json() as { error: string; request_id: string };
    expect(bodyA.error).toBe("CONTAINER_UNAVAILABLE");
    expect(bodyB.error).toBe("CONTAINER_UNAVAILABLE");
    // Each request gets its own request id (independent DO invocations).
    expect(bodyA.request_id).not.toBe(bodyB.request_id);
  });

  it("cross-tenant access is rejected — tenant A token on tenant B path → 403", async () => {
    // tenant A's valid token used against tenant B's namespace must be denied
    // by the path-spoof gate, never reaching the DO.
    const resp = await dispatchFetch(`/api/v2/${SECOND_TENANT_ID}/blobs`, {
      headers: { Authorization: `Bearer ${VALID_TOKEN}` },
    });
    expect(resp.status).toBe(403);
    const body = await resp.json() as { error: string };
    expect(body.error).toBe("FORBIDDEN");
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// RolloutController stub in workerd
// ──────────────────────────────────────────────────────────────────────────────

describe("miniflare: RolloutController DO in workerd", () => {
  it("worker never exposes RolloutController — /v1/rollouts/* falls into the PAT-gated reapi_v1 arm (401)", async () => {
    // The RolloutController DO is not directly exposed via the worker's route
    // table. Since the generic `/v1/*` fallthrough arm was added (reapi_v1 —
    // /v1/users/me, /v1/cas/*, …; see matchRoute in worker/src/index.ts),
    // /v1/rollouts/* no longer 404s: it matches reapi_v1, which requires a
    // PAT, so an unauthenticated probe is rejected 401 UNAUTHORIZED at the
    // Worker and never reaches any DO. (Genuinely unmatched paths still 404 —
    // covered by the "404 has correct JSON shape" test above.)
    const resp = await dispatchFetch("/v1/rollouts/test-rollout");
    expect(resp.status).toBe(401);
    const body = await resp.json() as { error: string };
    expect(body.error).toBe("UNAUTHORIZED");
  });
});
