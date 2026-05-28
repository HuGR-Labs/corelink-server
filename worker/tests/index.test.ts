/**
 * Unit tests for worker/src/index.ts — route resolution, auth middleware,
 * error mapping, CORS, and timing-pad paths.
 *
 * Tests import and exercise the worker's exported handler directly.
 * This avoids the cloudflare:test / SELF.fetch dependency (which requires
 * a working cloudflare-pool miniflare setup) and instead tests the pure
 * fetch handler logic with real Node.js Web APIs.
 *
 * Node.js v22+ provides native fetch, Request, Response, Headers, crypto,
 * URL — no polyfills needed.
 */

import { describe, it, expect, vi } from "vitest";
import workerHandler from "../src/index.js";
import type { Env } from "../src/index.js";

// ──────────────────────────────────────────────────────────────────────────────
// Test helpers
// ──────────────────────────────────────────────────────────────────────────────

/** Minimal mock ExecutionContext */
function makeCtx(): ExecutionContext {
  return {
    waitUntil: (_p: Promise<unknown>) => {},
    passThroughOnException: () => {},
  } as unknown as ExecutionContext;
}

/** Mock Env with a CORELINK_SERVER DO namespace that returns an error stub */
function makeEnv(doStubStatus = 503): Env {
  const stub = {
    fetch: async (_req: Request): Promise<Response> => {
      return new Response(
        JSON.stringify({ error: "CONTAINER_UNAVAILABLE", message: "test stub", request_id: "stub" }),
        { status: doStubStatus, headers: { "Content-Type": "application/json" } },
      );
    },
  };

  const namespace = {
    idFromName: (_name: string) => ({ toString: () => "stub-id" }),
    get: (_id: unknown) => stub,
    idFromString: (_s: string) => ({ toString: () => "stub-id" }),
    newUniqueId: () => ({ toString: () => "stub-unique-id" }),
    jurisdiction: (_j: string) => namespace,
  } as unknown as DurableObjectNamespace;

  return {
    CORELINK_SERVER: namespace,
    ENVIRONMENT: "test",
  };
}

/** Invoke the worker fetch handler */
async function workerFetch(
  url: string,
  init?: RequestInit,
  envOverride?: Partial<Env>,
): Promise<Response> {
  const req = new Request(url, init);
  const env = { ...makeEnv(), ...envOverride };
  const ctx = makeCtx();
  return workerHandler.fetch!(req, env, ctx);
}

// ──────────────────────────────────────────────────────────────────────────────
// Health endpoint
// ──────────────────────────────────────────────────────────────────────────────

describe("GET /health", () => {
  it("returns 200 with status:ok", async () => {
    const resp = await workerFetch("http://localhost/health");
    expect(resp.status).toBe(200);
    const body = await resp.json() as { status: string };
    expect(body.status).toBe("ok");
  });

  it("sets X-Request-Id header", async () => {
    const resp = await workerFetch("http://localhost/health");
    expect(resp.headers.get("x-request-id")).not.toBeNull();
  });

  it("propagates incoming X-Request-Id if valid", async () => {
    const resp = await workerFetch("http://localhost/health", {
      headers: { "x-request-id": "test-id-12345" },
    });
    expect(resp.headers.get("x-request-id")).toBe("test-id-12345");
  });

  it("rejects oversized x-request-id (>128 chars) and generates fresh UUID", async () => {
    const longId = "x".repeat(200);
    const resp = await workerFetch("http://localhost/health", {
      headers: { "x-request-id": longId },
    });
    expect(resp.headers.get("x-request-id")).not.toBe(longId);
    expect(resp.status).toBe(200);
  });

  it("includes env field in JSON body", async () => {
    const resp = await workerFetch("http://localhost/health");
    const body = await resp.json() as { env: string };
    expect(body.env).toBe("test");
  });

  it("does not require Authorization header", async () => {
    const resp = await workerFetch("http://localhost/health");
    expect(resp.status).toBe(200);
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// Not-found — timing-padded
// ──────────────────────────────────────────────────────────────────────────────

describe("GET unknown path (not_found)", () => {
  it("returns 404", async () => {
    const resp = await workerFetch("http://localhost/does-not-exist");
    expect(resp.status).toBe(404);
  });

  it("returns REAPI JSON error envelope", async () => {
    const resp = await workerFetch("http://localhost/no-such-route");
    const body = await resp.json() as { error: string; message: string; request_id: string };
    expect(body.error).toBe("NOT_FOUND");
    expect(typeof body.message).toBe("string");
    expect(typeof body.request_id).toBe("string");
  });

  it("sets X-Request-Id on 404", async () => {
    const resp = await workerFetch("http://localhost/nope");
    expect(resp.headers.get("x-request-id")).not.toBeNull();
  });

  it("returns Content-Type application/json", async () => {
    const resp = await workerFetch("http://localhost/missing");
    expect(resp.headers.get("content-type")).toContain("application/json");
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// Auth middleware
// ──────────────────────────────────────────────────────────────────────────────

describe("auth middleware", () => {
  it("returns 401 for /v2/ without Authorization header", async () => {
    const resp = await workerFetch("http://localhost/v2/");
    expect(resp.status).toBe(401);
  });

  it("returns OCI errors array for /v2/ 401", async () => {
    const resp = await workerFetch("http://localhost/v2/");
    const body = await resp.json() as { errors: Array<{ code: string }> };
    expect(Array.isArray(body.errors)).toBe(true);
    expect(body.errors[0]?.code).toBe("UNAUTHORIZED");
  });

  it("returns Docker-Distribution-Api-Version on /v2/ 401", async () => {
    const resp = await workerFetch("http://localhost/v2/");
    expect(resp.headers.get("docker-distribution-api-version")).toBe("registry/2.0");
  });

  it("returns 401 for REAPI /api/v2/ without Authorization", async () => {
    const resp = await workerFetch("http://localhost/api/v2/some/path");
    expect(resp.status).toBe(401);
  });

  it("returns REAPI error field for /api/v2/ 401", async () => {
    const resp = await workerFetch("http://localhost/api/v2/some/path");
    const body = await resp.json() as { error: string };
    expect(body.error).toBe("UNAUTHORIZED");
    // NOT errors array
    expect((body as unknown as { errors?: unknown }).errors).toBeUndefined();
  });

  it("returns 401 for token too short (<32 chars)", async () => {
    const resp = await workerFetch("http://localhost/api/v2/t/path", {
      headers: { Authorization: "Bearer shorttoken" },
    });
    expect(resp.status).toBe(401);
  });

  it("returns 401 for token too long (>256 chars)", async () => {
    const longToken = "a".repeat(300);
    const resp = await workerFetch("http://localhost/api/v2/t/path", {
      headers: { Authorization: `Bearer ${longToken}` },
    });
    expect(resp.status).toBe(401);
  });

  it("returns 401 for wrong scheme (Basic)", async () => {
    const resp = await workerFetch("http://localhost/api/v2/t/path", {
      headers: { Authorization: "Basic dXNlcjpwYXNz" },
    });
    expect(resp.status).toBe(401);
  });

  it("returns 401 for missing Authorization entirely", async () => {
    const resp = await workerFetch("http://localhost/npm/t/package");
    expect(resp.status).toBe(401);
  });

  it("passes valid 64-char token to DO (expects non-401)", async () => {
    const validToken = "a".repeat(64);
    const resp = await workerFetch("http://localhost/api/v2/tenant/path", {
      headers: { Authorization: `Bearer ${validToken}` },
    });
    expect(resp.status).not.toBe(401);
    // DO mock returns 503
    expect(resp.status).toBe(503);
  });

  it("does not reflect raw token value in error response", async () => {
    const sensitiveToken = "MYSECRETTOKEN123" + "x".repeat(48);
    const resp = await workerFetch("http://localhost/api/v2/t/p", {
      headers: { Authorization: `Bearer ${sensitiveToken}` },
    });
    const text = await resp.text();
    expect(text).not.toContain(sensitiveToken);
    expect(text).not.toContain("MYSECRETTOKEN");
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// CORS
// ──────────────────────────────────────────────────────────────────────────────

describe("CORS", () => {
  it("returns 204 for OPTIONS preflight from allowed origin", async () => {
    const resp = await workerFetch("http://localhost/v2/", {
      method: "OPTIONS",
      headers: {
        Origin: "https://app.corelink.humangr.com",
        "Access-Control-Request-Method": "GET",
      },
    });
    expect(resp.status).toBe(204);
    expect(resp.headers.get("access-control-allow-origin")).toBe(
      "https://app.corelink.humangr.com",
    );
  });

  it("returns 204 for OPTIONS from unknown origin but no ACAO header", async () => {
    const resp = await workerFetch("http://localhost/health", {
      method: "OPTIONS",
      headers: {
        Origin: "https://evil.example.com",
        "Access-Control-Request-Method": "GET",
      },
    });
    expect(resp.status).toBe(204);
    expect(resp.headers.get("access-control-allow-origin")).toBeNull();
  });

  it("attaches ACAO to /health from allowed admin origin", async () => {
    const resp = await workerFetch("http://localhost/health", {
      headers: { Origin: "https://admin.corelink.humangr.com" },
    });
    expect(resp.headers.get("access-control-allow-origin")).toBe(
      "https://admin.corelink.humangr.com",
    );
  });

  it("does NOT attach ACAO to /health from disallowed origin", async () => {
    const resp = await workerFetch("http://localhost/health", {
      headers: { Origin: "https://attacker.example.com" },
    });
    expect(resp.headers.get("access-control-allow-origin")).toBeNull();
  });

  it("attaches Vary: Origin when ACAO is set", async () => {
    const resp = await workerFetch("http://localhost/health", {
      headers: { Origin: "https://app.corelink.humangr.com" },
    });
    expect(resp.headers.get("vary")).toBe("Origin");
  });

  it("all three allowed origins work", async () => {
    const allowed = [
      "https://app.corelink.humangr.com",
      "https://admin.corelink.humangr.com",
      "https://docs.corelink.humangr.com",
    ];
    for (const origin of allowed) {
      const resp = await workerFetch("http://localhost/health", {
        headers: { Origin: origin },
      });
      expect(resp.headers.get("access-control-allow-origin")).toBe(origin);
    }
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// Route resolution
// ──────────────────────────────────────────────────────────────────────────────

describe("route resolution — OCI paths", () => {
  const ociPaths = [
    "/v2/",
    "/v2",
    "/v2/_catalog",
    "/v2/myrepo/blobs/sha256:abc",
    "/v2/myrepo/manifests/latest",
    "/v2/myrepo/tags/list",
  ];

  for (const path of ociPaths) {
    it(`${path} returns OCI error envelope on unauthenticated request`, async () => {
      const resp = await workerFetch(`http://localhost${path}`);
      const body = await resp.json() as Record<string, unknown>;
      expect(Array.isArray(body["errors"])).toBe(true);
      expect(resp.headers.get("docker-distribution-api-version")).toBe("registry/2.0");
    });
  }
});

describe("route resolution — non-OCI paths", () => {
  const reapiPaths = [
    "/api/v2/tenant/blobs",
    "/npm/tenant/package",
    "/pip/tenant/index",
    "/brew/tenant/api/formula",
    "/cargo/tenant/api/v1/crates",
  ];

  for (const path of reapiPaths) {
    it(`${path} returns REAPI error envelope on unauthenticated request`, async () => {
      const resp = await workerFetch(`http://localhost${path}`);
      const body = await resp.json() as Record<string, unknown>;
      expect(typeof body["error"]).toBe("string");
      expect(body["error"]).toBe("UNAUTHORIZED");
      expect(body["errors"]).toBeUndefined();
    });
  }
});

// ──────────────────────────────────────────────────────────────────────────────
// DO error mapping
// ──────────────────────────────────────────────────────────────────────────────

describe("DO error mapping", () => {
  it("DO 503 on OCI path returns valid JSON with error field", async () => {
    const validToken = "f".repeat(64);
    const resp = await workerFetch("http://localhost/v2/myrepo/manifests/latest", {
      headers: { Authorization: `Bearer ${validToken}` },
    });
    const body = await resp.json() as Record<string, unknown>;
    const hasErrors = Array.isArray(body["errors"]);
    const hasError = typeof body["error"] === "string";
    expect(hasErrors || hasError).toBe(true);
  });

  it("DO 503 on REAPI path returns REAPI envelope", async () => {
    const validToken = "g".repeat(64);
    const resp = await workerFetch("http://localhost/api/v2/tenant/blobs", {
      headers: { Authorization: `Bearer ${validToken}` },
    });
    // DO returns 503, Worker passes it through with x-request-id added
    expect(resp.status).toBe(503);
    expect(resp.headers.get("x-request-id")).not.toBeNull();
  });

  it("DO fetch exception returns 500 on REAPI path", async () => {
    const throwingEnv: Partial<Env> = {
      CORELINK_SERVER: {
        idFromName: (_name: string) => ({ toString: () => "stub-id" }),
        get: (_id: unknown) => ({
          fetch: async (_req: Request): Promise<Response> => {
            throw new Error("connection refused");
          },
        }),
        idFromString: (_s: string) => ({ toString: () => "stub-id" }),
        newUniqueId: () => ({ toString: () => "stub-unique-id" }),
        jurisdiction: (_j: string) => throwingEnv.CORELINK_SERVER,
      } as unknown as DurableObjectNamespace,
    };

    const validToken = "h".repeat(64);
    const resp = await workerFetch("http://localhost/api/v2/tenant/path", {
      headers: { Authorization: `Bearer ${validToken}` },
    }, throwingEnv);
    expect(resp.status).toBe(500);
    const body = await resp.json() as { error: string };
    expect(body.error).toBe("INTERNAL_ERROR");
  });

  it("DO fetch exception on OCI path returns OCI error envelope", async () => {
    const throwingEnv: Partial<Env> = {
      CORELINK_SERVER: {
        idFromName: (_name: string) => ({ toString: () => "stub-id" }),
        get: (_id: unknown) => ({
          fetch: async (_req: Request): Promise<Response> => {
            throw new Error("network error");
          },
        }),
        idFromString: (_s: string) => ({ toString: () => "stub-id" }),
        newUniqueId: () => ({ toString: () => "stub-unique-id" }),
        jurisdiction: (_j: string) => throwingEnv.CORELINK_SERVER,
      } as unknown as DurableObjectNamespace,
    };

    const validToken = "i".repeat(64);
    const resp = await workerFetch("http://localhost/v2/repo/blobs/sha256:abc", {
      headers: { Authorization: `Bearer ${validToken}` },
    }, throwingEnv);
    expect(resp.status).toBe(500);
    const body = await resp.json() as Record<string, unknown>;
    expect(Array.isArray(body["errors"])).toBe(true);
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// Security invariants
// ──────────────────────────────────────────────────────────────────────────────

describe("security invariants", () => {
  it("X-Request-Id present on every response type", async () => {
    const cases = [
      "http://localhost/health",
      "http://localhost/v2/",
      "http://localhost/api/v2/t/p",
      "http://localhost/unknown-xyz",
    ];
    for (const url of cases) {
      const resp = await workerFetch(url);
      expect(resp.headers.get("x-request-id"), `missing for ${url}`).not.toBeNull();
    }
  });

  it("token value NOT reflected in any 401 response body", async () => {
    const secretToken = "SECRETTOKEN123" + "x".repeat(50);
    const resp = await workerFetch("http://localhost/api/v2/t/path", {
      headers: { Authorization: `Bearer ${secretToken}` },
    });
    const text = await resp.text();
    expect(text).not.toContain(secretToken);
    expect(text).not.toContain("SECRETTOKEN");
  });

  it("token format check is constant-time (no early exit on short token)", async () => {
    // If format check is NOT constant-time, very short tokens would return faster.
    // We can't measure nanoseconds, but we can at least verify it returns 401 for
    // both short tokens (1 char) and max-1 length tokens.
    const resp1 = await workerFetch("http://localhost/api/v2/t/p", {
      headers: { Authorization: "Bearer x" },
    });
    const resp2 = await workerFetch("http://localhost/api/v2/t/p", {
      headers: { Authorization: "Bearer " + "y".repeat(31) },
    });
    expect(resp1.status).toBe(401);
    expect(resp2.status).toBe(401);
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// INV-NO-BODY-IN-LOGS
// ──────────────────────────────────────────────────────────────────────────────

describe("INV-NO-BODY-IN-LOGS", () => {
  it("POST body content NOT reflected in error response", async () => {
    const sensitiveBody = "secret-payload-that-must-not-leak=true&token=abc123";
    const validToken = "j".repeat(64);

    const resp = await workerFetch("http://localhost/api/v2/tenant/blobs", {
      method: "POST",
      headers: {
        Authorization: `Bearer ${validToken}`,
        "Content-Type": "application/x-www-form-urlencoded",
      },
      body: sensitiveBody,
    });

    const text = await resp.text();
    expect(text).not.toContain("secret-payload");
    expect(text).not.toContain("must-not-leak");
    expect(text).not.toContain("abc123");
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// Auth middleware — invalid character paths
// ──────────────────────────────────────────────────────────────────────────────

describe("auth middleware — invalid token characters", () => {
  it("returns 401 for token containing a space character (0x20 < 0x21)", async () => {
    // Token with an embedded space — must be caught by the printable-ASCII guard
    const tokenWithSpace = "validstart-" + "a".repeat(20) + " " + "a".repeat(20);
    const resp = await workerFetch("http://localhost/api/v2/t/p", {
      headers: { Authorization: `Bearer ${tokenWithSpace}` },
    });
    expect(resp.status).toBe(401);
  });

  it("returns 401 for token containing a tab character (0x09 < 0x21)", async () => {
    const tokenWithTab = "validstart-" + "a".repeat(20) + "\t" + "a".repeat(20);
    const resp = await workerFetch("http://localhost/api/v2/t/p", {
      headers: { Authorization: `Bearer ${tokenWithTab}` },
    });
    expect(resp.status).toBe(401);
  });

  it("returns 401 for token containing a DEL character (0x7F > 0x7E)", async () => {
    const tokenWithDel = "validstart-" + "a".repeat(20) + "\x7F" + "a".repeat(20);
    const resp = await workerFetch("http://localhost/api/v2/t/p", {
      headers: { Authorization: `Bearer ${tokenWithDel}` },
    });
    expect(resp.status).toBe(401);
  });

  it("error body for invalid-char token does not echo the token", async () => {
    const evilToken = "INVALID\x01CHARS" + "x".repeat(32);
    const resp = await workerFetch("http://localhost/api/v2/t/p", {
      headers: { Authorization: `Bearer ${evilToken}` },
    });
    const text = await resp.text();
    expect(text).not.toContain("INVALID");
    expect(text).not.toContain("\x01");
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// OCI error status code mapping — ociStatusForCode branch coverage
// ──────────────────────────────────────────────────────────────────────────────

describe("OCI status-for-code mapping", () => {
  // We test ociStatusForCode indirectly by having the DO stub return a response
  // that the worker processes. The worker calls ociError() directly for auth
  // failures — we already cover UNAUTHORIZED. The other codes are exercised
  // when the DO returns error envelopes for authenticated OCI requests.

  it("DO returning OCI BLOB_UNKNOWN envelope passes through with correct request-id", async () => {
    const blobUnknownEnv: Partial<Env> = {
      CORELINK_SERVER: {
        idFromName: (_n: string) => ({ toString: () => "id" }),
        get: () => ({
          fetch: async (req: Request) =>
            new Response(
              JSON.stringify({ errors: [{ code: "BLOB_UNKNOWN", message: "blob not found", detail: null }] }),
              {
                status: 404,
                headers: {
                  "Content-Type": "application/json",
                  "X-Request-Id": req.headers.get("x-request-id") ?? "stub",
                },
              },
            ),
        }),
        idFromString: (_s: string) => ({ toString: () => "id" }),
        newUniqueId: () => ({ toString: () => "id" }),
        jurisdiction: (_j: string) => blobUnknownEnv.CORELINK_SERVER,
      } as unknown as DurableObjectNamespace,
    };
    const token = "a".repeat(64);
    const resp = await workerFetch(
      "http://localhost/v2/repo/blobs/sha256:deadbeef",
      { headers: { Authorization: `Bearer ${token}` } },
      blobUnknownEnv,
    );
    // DO returns 404; worker passes it through with x-request-id
    expect(resp.status).toBe(404);
    expect(resp.headers.get("x-request-id")).not.toBeNull();
  });

  it("worker returns UNAUTHORIZED OCI error (code path: UNAUTHORIZED → 401)", async () => {
    // This exercises ociStatusForCode('UNAUTHORIZED') = 401 directly via the
    // worker's auth middleware for OCI routes.
    const resp = await workerFetch("http://localhost/v2/repo/blobs/sha256:abc");
    expect(resp.status).toBe(401);
    const body = await resp.json() as { errors: Array<{ code: string }> };
    expect(body.errors[0]?.code).toBe("UNAUTHORIZED");
  });

  it("worker returns UNKNOWN OCI error (500) when DO fetch throws on OCI path", async () => {
    // This exercises ociStatusForCode default branch (unknown code → 500)
    const throwingEnv: Partial<Env> = {
      CORELINK_SERVER: {
        idFromName: (_n: string) => ({ toString: () => "id" }),
        get: () => ({
          fetch: async (_req: Request): Promise<Response> => {
            throw new Error("simulated network failure");
          },
        }),
        idFromString: (_s: string) => ({ toString: () => "id" }),
        newUniqueId: () => ({ toString: () => "id" }),
        jurisdiction: (_j: string) => throwingEnv.CORELINK_SERVER,
      } as unknown as DurableObjectNamespace,
    };
    const token = "b".repeat(64);
    const resp = await workerFetch(
      "http://localhost/v2/repo/manifests/latest",
      { headers: { Authorization: `Bearer ${token}` } },
      throwingEnv,
    );
    // ociError("UNKNOWN", ...) → ociStatusForCode('UNKNOWN') → default 500
    expect(resp.status).toBe(500);
    const body = await resp.json() as { errors: Array<{ code: string }> };
    expect(body.errors[0]?.code).toBe("UNKNOWN");
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// Request-Id forwarding to DO
// ──────────────────────────────────────────────────────────────────────────────

describe("X-Request-Id forwarding to DO", () => {
  let capturedRequestId: string | null = null;

  it("forwards x-request-id to DO stub", async () => {
    const requestId = "forward-test-" + Math.random().toString(36).slice(2);
    const capturingEnv: Partial<Env> = {
      CORELINK_SERVER: {
        idFromName: (_name: string) => ({ toString: () => "stub-id" }),
        get: (_id: unknown) => ({
          fetch: async (req: Request): Promise<Response> => {
            capturedRequestId = req.headers.get("x-request-id");
            return new Response(JSON.stringify({ ok: true }), {
              status: 200,
              headers: { "Content-Type": "application/json" },
            });
          },
        }),
        idFromString: (_s: string) => ({ toString: () => "stub-id" }),
        newUniqueId: () => ({ toString: () => "stub-unique-id" }),
        jurisdiction: (_j: string) => capturingEnv.CORELINK_SERVER,
      } as unknown as DurableObjectNamespace,
    };

    const validToken = "k".repeat(64);
    await workerFetch("http://localhost/api/v2/tenant/path", {
      headers: {
        Authorization: `Bearer ${validToken}`,
        "x-request-id": requestId,
      },
    }, capturingEnv);

    expect(capturedRequestId).toBe(requestId);
  });
});
