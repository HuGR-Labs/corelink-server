/**
 * Integration-layer tests — end-to-end pipeline tests using the worker handler
 * directly with a mock DO that simulates full request → response round trips.
 *
 * These tests verify the full pipeline from HTTP entry → auth → route → DO →
 * response, using a configurable mock DO stub that can simulate different
 * upstream scenarios (success, 404, 502, etc.).
 *
 * For live wrangler dev smoke tests, see scripts/test-wrangler-smoke.sh which
 * runs `wrangler dev --local` and hits `curl http://localhost:8787/health`.
 */

import { describe, it, expect } from "vitest";
import type { D1Database } from "@cloudflare/workers-types";
import workerHandler from "../src/index.js";
import type { Env } from "../src/index.js";

// ──────────────────────────────────────────────────────────────────────────────
// Helpers
// ──────────────────────────────────────────────────────────────────────────────

// Canonical test PAT — format-valid CoreLink PAT; D1 mock recognises its
// token_id (AAAAAAAAAAAAAAAA) and returns a non-expired row for it.
// Format: corelink_pat_<16-char-Crockford-b32>.<43-char-base64url>.<22-char-base64url>
const TEST_TOKEN_ID = "AAAAAAAAAAAAAAAA";
const TEST_TENANT_ID = "00000000-0000-0000-0000-000000000001";
const VALID_TOKEN =
  "corelink_pat_" +
  TEST_TOKEN_ID +
  "." +
  "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA" +
  "." +
  "AAAAAAAAAAAAAAAAAAAAAA"; // total 96 chars

/**
 * Build a minimal D1 mock that recognises the test token_id and returns a
 * non-expired tenant row. All other token_ids return null (→ 401).
 */
function makeTestD1(): D1Database {
  return {
    prepare: (_sql: string) => ({
      bind: (...args: unknown[]) => ({
        first: async <T>() => {
          const tokenId = args[0] as string;
          if (tokenId === TEST_TOKEN_ID) {
            return { tenant_id: TEST_TENANT_ID, expires_ms: Date.now() + 3_600_000 } as T;
          }
          return null as T | null;
        },
        all: async <T>() => ({ success: true as const, meta: {} as never, results: [] as T[] }),
        run: async <T>() => ({ success: true as const, meta: {} as never, results: [] as T[] }),
        raw: async <T>() => [] as T[],
      }),
      first: async <T>() => null as T | null,
      all: async <T>() => ({ success: true as const, meta: {} as never, results: [] as T[] }),
      run: async <T>() => ({ success: true as const, meta: {} as never, results: [] as T[] }),
      raw: async <T>() => [] as T[],
    }),
    batch: async () => [],
    exec: async () => ({ count: 0, duration: 0 }),
    withSession: () => null as never,
    dump: async () => new ArrayBuffer(0),
  } as unknown as D1Database;
}

function makeCtx(): ExecutionContext {
  return {
    waitUntil: () => {},
    passThroughOnException: () => {},
  } as unknown as ExecutionContext;
}

/** Create an env with a configurable DO stub. */
function makeEnvWithStub(
  statusCode: number,
  body: unknown,
  extraHeaders?: Record<string, string>,
): Env {
  const stub = {
    fetch: async (req: Request): Promise<Response> => {
      const respHeaders: Record<string, string> = {
        "Content-Type": "application/json",
        "X-Request-Id": req.headers.get("x-request-id") ?? "stub",
        ...extraHeaders,
      };
      return new Response(JSON.stringify(body), {
        status: statusCode,
        headers: respHeaders,
      });
    },
  };

  return {
    CORELINK_SERVER: {
      idFromName: (_n: string) => ({ toString: () => "id" }),
      get: (_id: unknown) => stub,
      idFromString: (_s: string) => ({ toString: () => "id" }),
      newUniqueId: () => ({ toString: () => "unique-id" }),
      jurisdiction: (_j: string) => ({}) as DurableObjectNamespace,
    } as unknown as DurableObjectNamespace,
    ENVIRONMENT: "test",
    CONFIG_DB: makeTestD1(),
  };
}

async function fetch_(
  url: string,
  init?: RequestInit,
  envOverride?: Partial<Env>,
): Promise<Response> {
  const req = new Request(url, init);
  const env = { ...makeEnvWithStub(200, { ok: true }), ...envOverride };
  return workerHandler.fetch!(req, env, makeCtx());
}

// ──────────────────────────────────────────────────────────────────────────────
// Full pipeline smoke
// ──────────────────────────────────────────────────────────────────────────────

describe("full pipeline smoke", () => {
  it("GET /health returns 200 with env field", async () => {
    const resp = await fetch_("http://localhost/health");
    expect(resp.status).toBe(200);
    const body = await resp.json() as { status: string; env: string };
    expect(body.status).toBe("ok");
    expect(typeof body.env).toBe("string");
  });

  it("GET /health does NOT require auth", async () => {
    const resp = await fetch_("http://localhost/health");
    expect(resp.status).toBe(200);
  });

  it("authenticated OCI v2 check — pipeline reaches DO and returns DO response", async () => {
    const env = makeEnvWithStub(200, { ok: true }, { "x-corelink-from-do": "1" });
    const resp = await workerHandler.fetch!(
      new Request("http://localhost/v2/", {
        headers: { Authorization: `Bearer ${VALID_TOKEN}` },
      }),
      env,
      makeCtx(),
    );
    expect(resp.status).toBe(200);
    // Response came from DO (not auth-blocked)
  });

  it("authenticated REAPI v2 — pipeline reaches DO", async () => {
    const env = makeEnvWithStub(200, { blobs: [] });
    const resp = await workerHandler.fetch!(
      new Request(`http://localhost/api/v2/${TEST_TENANT_ID}/blobs`, {
        headers: { Authorization: `Bearer ${VALID_TOKEN}` },
      }),
      env,
      makeCtx(),
    );
    expect(resp.status).toBe(200);
  });

  it("authenticated npm — pipeline reaches DO", async () => {
    const env = makeEnvWithStub(200, { name: "my-package" });
    const resp = await workerHandler.fetch!(
      new Request(`http://localhost/npm/${TEST_TENANT_ID}/my-package`, {
        headers: { Authorization: `Bearer ${VALID_TOKEN}` },
      }),
      env,
      makeCtx(),
    );
    expect(resp.status).toBe(200);
  });

  it("authenticated cargo — pipeline reaches DO", async () => {
    const env = makeEnvWithStub(200, { crates: [] });
    const resp = await workerHandler.fetch!(
      new Request(`http://localhost/cargo/${TEST_TENANT_ID}/api/v1/crates/my-crate`, {
        headers: { Authorization: `Bearer ${VALID_TOKEN}` },
      }),
      env,
      makeCtx(),
    );
    expect(resp.status).toBe(200);
  });

  it("brew path reaches DO", async () => {
    const env = makeEnvWithStub(200, { formulae: [] });
    const resp = await workerHandler.fetch!(
      new Request(`http://localhost/brew/${TEST_TENANT_ID}/api/formula`, {
        headers: { Authorization: `Bearer ${VALID_TOKEN}` },
      }),
      env,
      makeCtx(),
    );
    expect(resp.status).toBe(200);
  });

  it("pip path reaches DO", async () => {
    const env = makeEnvWithStub(200, { packages: [] });
    const resp = await workerHandler.fetch!(
      new Request(`http://localhost/pip/${TEST_TENANT_ID}/simple/my-pkg`, {
        headers: { Authorization: `Bearer ${VALID_TOKEN}` },
      }),
      env,
      makeCtx(),
    );
    expect(resp.status).toBe(200);
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// OCI Distribution Spec conformance
// ──────────────────────────────────────────────────────────────────────────────

describe("OCI spec conformance", () => {
  it("GET /v2/ without auth returns Docker-Distribution-Api-Version header", async () => {
    const resp = await fetch_("http://localhost/v2/");
    expect(resp.headers.get("docker-distribution-api-version")).toBe("registry/2.0");
  });

  it("GET /v2/ without auth returns Content-Type: application/json", async () => {
    const resp = await fetch_("http://localhost/v2/");
    expect(resp.headers.get("content-type")).toContain("application/json");
  });

  it("GET /v2/ without auth returns 401 with errors array", async () => {
    const resp = await fetch_("http://localhost/v2/");
    expect(resp.status).toBe(401);
    const body = await resp.json() as { errors: Array<{ code: string; message: string }> };
    expect(Array.isArray(body.errors)).toBe(true);
    expect(body.errors[0]?.code).toBe("UNAUTHORIZED");
  });

  it("GET /v2 (no trailing slash) returns OCI error envelope", async () => {
    const resp = await fetch_("http://localhost/v2");
    expect(resp.status).not.toBe(404);
    const body = await resp.json() as Record<string, unknown>;
    expect(Array.isArray(body["errors"])).toBe(true);
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// 404 timing-pad path
// ──────────────────────────────────────────────────────────────────────────────

describe("timing-pad on 404", () => {
  it("404 response completes in reasonable time (< 2s ceiling)", async () => {
    // The timing pad targets ~80ms. In tests we don't want to wait the full
    // pad time — but we verify the function completes (doesn't hang forever).
    // We use a short timeout: the pad uses Date.now() comparison, so in the
    // test it will see requestStart as ~0ms ago and compute a positive sleep.
    // We just verify it resolves without hanging.
    const start = Date.now();
    const resp = await fetch_("http://localhost/completely-unknown");
    const elapsed = Date.now() - start;
    expect(resp.status).toBe(404);
    // Must complete within 2s (the pad target is ~80ms; we allow 2s margin)
    expect(elapsed).toBeLessThan(2000);
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// DO 404 response — timing-pad applied
// ──────────────────────────────────────────────────────────────────────────────

describe("DO 404 response timing-pad", () => {
  it("DO 404 response is passed through with x-request-id and completes", async () => {
    const env = makeEnvWithStub(404, { errors: [{ code: "BLOB_UNKNOWN", message: "not found", detail: null }] });
    const resp = await workerHandler.fetch!(
      new Request(`http://localhost/v2/${TEST_TENANT_ID}/blobs/sha256:deadbeef`, {
        headers: { Authorization: `Bearer ${VALID_TOKEN}` },
      }),
      env,
      makeCtx(),
    );
    // 404 from DO is passed through; the timing-pad adds latency
    expect(resp.status).toBe(404);
    expect(resp.headers.get("x-request-id")).not.toBeNull();
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// Security headers on all responses
// ──────────────────────────────────────────────────────────────────────────────

describe("security headers", () => {
  it("X-Request-Id present on every response", async () => {
    const paths = [
      "http://localhost/health",
      "http://localhost/v2/",
      "http://localhost/api/v2/t/p",
      "http://localhost/unknown-xyz",
    ];
    for (const path of paths) {
      const resp = await fetch_(path);
      expect(resp.headers.get("x-request-id"), `missing x-request-id for ${path}`).not.toBeNull();
    }
  });

  it("Authorization header value NOT reflected in any response body", async () => {
    const secretToken = "SECRETTOKEN456" + "x".repeat(50);
    const resp = await fetch_("http://localhost/api/v2/t/path", {
      headers: { Authorization: `Bearer ${secretToken}` },
    });
    const text = await resp.text();
    expect(text).not.toContain(secretToken);
    expect(text).not.toContain("SECRETTOKEN456");
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// Request method coverage
// ──────────────────────────────────────────────────────────────────────────────

describe("HTTP method handling", () => {
  it("POST to authenticated route reaches DO", async () => {
    const env = makeEnvWithStub(201, { upload_url: "/v2/repo/blobs/uploads/uuid" });
    const resp = await workerHandler.fetch!(
      new Request(`http://localhost/v2/${TEST_TENANT_ID}/blobs/uploads/`, {
        method: "POST",
        headers: { Authorization: `Bearer ${VALID_TOKEN}` },
      }),
      env,
      makeCtx(),
    );
    expect(resp.status).toBe(201);
  });

  it("PUT to authenticated route reaches DO", async () => {
    const env = makeEnvWithStub(200, { digest: "sha256:abc" });
    const resp = await workerHandler.fetch!(
      new Request(`http://localhost/v2/${TEST_TENANT_ID}/manifests/latest`, {
        method: "PUT",
        headers: {
          Authorization: `Bearer ${VALID_TOKEN}`,
          "Content-Type": "application/vnd.oci.image.manifest.v1+json",
        },
        body: JSON.stringify({ schemaVersion: 2 }),
      }),
      env,
      makeCtx(),
    );
    expect(resp.status).toBe(200);
  });

  it("DELETE to unauthenticated route returns 401", async () => {
    const resp = await fetch_("http://localhost/v2/repo/manifests/latest", {
      method: "DELETE",
    });
    expect(resp.status).toBe(401);
  });
});
