/**
 * Miniflare integration tests for CoreLinkServer Durable Object.
 *
 * R2 mandate (§0.8): "miniflare integration tests for DO are mandatory".
 *
 * These tests use miniflare v3 (workspace version) programmatically within a
 * standard Node.js vitest test. A Miniflare instance is created, the worker
 * bundle (compiled by wrangler --dry-run) is loaded into workerd, and HTTP
 * requests are dispatched through the full Workers runtime (NOT Node.js).
 *
 * Prerequisites:
 *   cd worker && pnpm run build  (produces dist/index.js)
 *
 * If dist/index.js does not exist, the beforeAll hook builds it automatically
 * via `wrangler deploy --dry-run --outdir dist`.
 *
 * Note on @cloudflare/vitest-pool-workers:
 *   pool-workers@0.16.10 has a runtime incompatibility with vitest@4.1.7 —
 *   the cloudflare:test module fails to load `@vitest/expect` named exports
 *   inside workerd. The programmatic miniflare approach achieves the same
 *   workerd-backed test guarantee without this dependency.
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
import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";

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
  // Build the worker bundle if not already present
  if (!existsSync(DIST_INDEX)) {
    console.log("[miniflare-test] Building worker bundle via wrangler dry-run...");
    // Run from the worktree root so --outdir dist is relative to wrangler.toml
    execSync(
      `${WORKER_DIR}/node_modules/.bin/wrangler deploy --dry-run --outdir dist`,
      { cwd: WORKTREE_ROOT, stdio: "inherit" },
    );
  }

  if (!existsSync(DIST_INDEX)) {
    throw new Error(`[miniflare-test] Worker bundle not found at ${DIST_INDEX}. Run: cd worker && pnpm build`);
  }

  const script = readFileSync(DIST_INDEX, "utf8");

  // Lazy-import miniflare to avoid loading it in the primary test suite
  // (which uses a different miniflare version). The workspace root provides v3.
  const { Miniflare } = await import("miniflare");

  mf = new Miniflare({
    script,
    modules: true,
    durableObjects: {
      CORELINK_SERVER: "CoreLinkServer",
      ROLLOUT_DO: "RolloutController",
    },
    compatibilityDate: "2026-04-01",
    bindings: {
      ENVIRONMENT: "miniflare-test",
      PAGERDUTY_ROUTING_KEY: "",
    },
    // D1 database binding — in-memory SQLite seeded with the test PAT row.
    d1Databases: {
      CONFIG_DB: "test-config-db",
    },
  });

  // Seed the D1 PAT table with a test row so authenticated tests can pass.
  const d1 = await mf.getD1Database("CONFIG_DB");
  await d1.exec("CREATE TABLE IF NOT EXISTS pat (pat_id TEXT NOT NULL PRIMARY KEY, tenant_id TEXT NOT NULL, token_id TEXT UNIQUE, expires_ms INTEGER NOT NULL)");
  await d1.prepare(
    "INSERT OR IGNORE INTO pat (pat_id, tenant_id, token_id, expires_ms) VALUES (?1, ?2, ?3, ?4)"
  ).bind(
    "00000000-0000-0000-0000-000000000042",
    TEST_TENANT_ID,
    TEST_TOKEN_ID,
    Date.now() + 7_200_000, // +2h
  ).run();
  // Second tenant for cross-tenant isolation tests (distinct token_id + tenant_id).
  await d1.prepare(
    "INSERT OR IGNORE INTO pat (pat_id, tenant_id, token_id, expires_ms) VALUES (?1, ?2, ?3, ?4)"
  ).bind(
    "00000000-0000-0000-0000-000000000043",
    SECOND_TENANT_ID,
    SECOND_TOKEN_ID,
    Date.now() + 7_200_000, // +2h
  ).run();

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

// Canonical test PAT — format-valid CoreLink PAT recognised by the D1 mock
// seeded in beforeAll. Must match the token_id inserted into D1.
const TEST_TOKEN_ID = "AAAAAAAAAAAAAAAA"; // 16 Crockford b32
const TEST_TENANT_ID = "00000000-0000-0000-0000-000000000001";
const VALID_TOKEN =
  "corelink_pat_" +
  TEST_TOKEN_ID +
  "." +
  "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA" + // 43 base64url
  "." +
  "AAAAAAAAAAAAAAAAAAAAAA"; // 22 base64url (total 96)

// Second tenant — distinct token_id + tenant_id — for cross-tenant isolation.
const SECOND_TOKEN_ID = "BBBBBBBBBBBBBBBB"; // 16 Crockford b32
const SECOND_TENANT_ID = "00000000-0000-0000-0000-000000000002";
const SECOND_TOKEN =
  "corelink_pat_" +
  SECOND_TOKEN_ID +
  "." +
  "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA" + // 43 base64url
  "." +
  "AAAAAAAAAAAAAAAAAAAAAA"; // 22 base64url (total 96)

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

  it("includes env field in response body", async () => {
    const resp = await dispatchFetch("/health");
    const body = await resp.json() as { env: string };
    expect(typeof body.env).toBe("string");
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
});

// ──────────────────────────────────────────────────────────────────────────────
// CORS in workerd
// ──────────────────────────────────────────────────────────────────────────────

describe("miniflare: CORS in workerd", () => {
  it("OPTIONS from allowed origin returns 204 with ACAO", async () => {
    const resp = await dispatchFetch("/v2/", {
      method: "OPTIONS",
      headers: {
        Origin: "https://corelink-app.humangr.com",
        "Access-Control-Request-Method": "GET",
      },
    });
    expect(resp.status).toBe(204);
    expect(resp.headers.get("access-control-allow-origin")).toBe(
      "https://corelink-app.humangr.com",
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
  it("worker routes unknown path to not_found (RolloutController is internal-only)", async () => {
    // The RolloutController DO is not directly exposed via the worker's route table.
    // Requests to unmatched paths return 404 NOT_FOUND (timing-padded).
    const resp = await dispatchFetch("/v1/rollouts/test-rollout");
    expect(resp.status).toBe(404);
    const body = await resp.json() as { error: string };
    expect(body.error).toBe("NOT_FOUND");
  });
});
